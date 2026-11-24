//! Codex app-server harness.
//!
//! Codex ships its own JSON-RPC server with the thread API a client needs to
//! share a conversation with the Codex TUI: list, resume, read items, start a
//! turn, steer it, interrupt it and answer approvals. This module drives that
//! server over newline-delimited JSON on stdio; the server owns the session,
//! threadbase mirrors it into [`RawAgentEvent`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;

use crate::bus::EventBus;
use crate::profile::AgentProfile;
use crate::{
    Capabilities, ElicitationAction, ExternalSession, ForegroundState, Harness, HarnessError,
    HarnessResult, LaunchCtx, MessageRole, NewSession, PermissionOption, PermissionRequest,
    PromptPart, RawAgentEvent, SessionHistory, SessionId, SessionItem, SessionTurn,
    SteeringOutcome, ToolCallEvent, TurnHandle,
};
use threadbase_shared::{
    AgentConfigOptionDto, AgentConfigValueDto, ApprovalDecision, DiffChangeDto, DiffSummary,
    PlanEntry, PlanPriority, PlanStatus, ToolKind, ToolLocation, ToolStatus, Usage,
};

const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(20);

/// One pending approval request from the server. The request id is the
/// server's, and the answer goes back on the same connection.
struct PendingApproval {
    request_id: Value,
    method: String,
    options: Vec<(String, String)>,
}

#[derive(Default)]
struct State {
    thread: Option<String>,
    turn: Option<String>,
    /// Tool items by item id, so deltas patch the row they belong to.
    tools: HashMap<String, ToolCallEvent>,
    approvals: HashMap<String, PendingApproval>,
    /// A `turn/start` is in flight, so the next `turn/started` is ours.
    awaiting_turn: bool,
    /// The active turn was started by this client.
    owning: bool,
    last_status: Option<String>,
    models: Vec<AgentConfigValueDto>,
    model: Option<String>,
    effort: Option<String>,
    /// Applied to the next turn, because Codex takes both on `turn/start`.
    approval_policy: Option<String>,
    sandbox_policy: Option<String>,
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

struct Client {
    outgoing: mpsc::UnboundedSender<Value>,
    pending: Pending,
    next_id: AtomicU64,
}

impl Client {
    fn new(outgoing: mpsc::UnboundedSender<Value>) -> Self {
        Self {
            outgoing,
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
        }
    }

    fn send_value(&self, message: Value) -> HarnessResult<()> {
        self.outgoing
            .send(message)
            .map_err(|_| HarnessError::NotRunning)
    }

    async fn request(&self, method: &str, params: Value) -> HarnessResult<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, tx);
        self.send_value(json!({"id": id, "method": method, "params": params}))?;
        rx.await
            .map_err(|_| HarnessError::NotRunning)?
            .map_err(HarnessError::Protocol)
    }
}

/// ACP-shaped harness over Codex's own app-server.
pub struct CodexHarness {
    profile: AgentProfile,
    bus: EventBus,
    state: Arc<Mutex<State>>,
    client: Option<Arc<Client>>,
    session: Option<SessionId>,
    cwd: Option<PathBuf>,
    model: Option<String>,
    effort: Option<String>,
    capabilities: Capabilities,
    agent_info: Option<crate::AgentIdentity>,
    shutdown: Option<watch::Sender<bool>>,
    driver: Option<tokio::task::JoinHandle<()>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl CodexHarness {
    #[must_use]
    pub fn new(profile: AgentProfile) -> Self {
        Self {
            profile,
            bus: EventBus::new(),
            state: Arc::new(Mutex::new(State::default())),
            client: None,
            session: None,
            cwd: None,
            model: None,
            effort: None,
            capabilities: Capabilities::default(),
            agent_info: None,
            shutdown: None,
            driver: None,
            tasks: Vec::new(),
        }
    }

    /// Capabilities observed while starting the harness.
    #[must_use]
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// Agent identity reported during `initialize`.
    #[must_use]
    pub fn agent_info(&self) -> Option<&crate::AgentIdentity> {
        self.agent_info.as_ref()
    }

    fn client(&self) -> HarnessResult<&Arc<Client>> {
        self.client.as_ref().ok_or(HarnessError::NotRunning)
    }

    fn thread(&self) -> HarnessResult<String> {
        self.active_session().map(|session| session.0.clone())
    }

    fn active_session(&self) -> HarnessResult<&SessionId> {
        self.session.as_ref().ok_or_else(|| {
            HarnessError::SessionNotFound("no Codex thread has been opened".to_owned())
        })
    }

    fn apply_permission_mode(&mut self, mode: threadbase_shared::PermissionMode) {
        let (approval, sandbox) = match mode {
            threadbase_shared::PermissionMode::Supervised => ("on-request", "workspace-write"),
            threadbase_shared::PermissionMode::AutoEdit => ("on-request", "workspace-write"),
            threadbase_shared::PermissionMode::FullAccess => ("never", "danger-full-access"),
        };
        let mut state = lock(&self.state);
        state.approval_policy = Some(approval.to_owned());
        state.sandbox_policy = Some(sandbox.to_owned());
    }

    fn apply_launch_config(&mut self) {
        if let Some(model) = self.model.clone() {
            lock(&self.state).model = Some(model);
        }
        if let Some(effort) = self.effort.clone() {
            lock(&self.state).effort = Some(effort);
        }
    }
}

#[async_trait::async_trait]
impl Harness for CodexHarness {
    fn id(&self) -> &str {
        &self.profile.id
    }

    async fn start(&mut self, ctx: LaunchCtx) -> HarnessResult<Capabilities> {
        if self.client.is_some() {
            return Err(HarnessError::Protocol(
                "Codex harness is already running".to_owned(),
            ));
        }
        self.model.clone_from(&ctx.model);
        self.effort.clone_from(&ctx.effort);

        let wire = open_wire(&ctx, self.state.clone(), self.bus.clone()).await?;
        let client = wire.client.clone();

        let initialize = tokio::time::timeout(
            INITIALIZE_TIMEOUT,
            client.request(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": "threadbase",
                        "title": "Threadbase",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": {"experimentalApi": true},
                }),
            ),
        )
        .await
        .map_err(|_| HarnessError::Protocol("codex did not answer initialize".to_owned()))??;
        let user_agent = initialize
            .get("userAgent")
            .and_then(Value::as_str)
            .unwrap_or("codex")
            .to_owned();
        self.agent_info = Some(identity(&user_agent));

        // Best effort: an empty list only hides the model picker.
        if let Ok(list) = client.request("model/list", json!({})).await {
            let models = model_options(&list);
            lock(&self.state).models = models;
        }

        self.cwd = Some(ctx.cwd);
        self.client = Some(client);
        self.shutdown = Some(wire.shutdown);
        self.driver = Some(wire.driver);
        self.tasks = wire.tasks;
        self.apply_launch_config();
        self.apply_permission_mode(
            ctx.permission_mode
                .unwrap_or(threadbase_shared::PermissionMode::Supervised),
        );
        self.capabilities = Capabilities {
            steering: true,
            images: false,
            subagents: false,
            elicitation_form: false,
            elicitation_url: false,
            terminal: false,
        };
        Ok(self.capabilities)
    }

    async fn new_session(&mut self, req: NewSession) -> HarnessResult<SessionId> {
        let mut params = json!({ "cwd": req.cwd.to_string_lossy() });
        if let Some(model) = lock(&self.state).model.clone() {
            params["model"] = json!(model);
        }
        let (approval, sandbox) = {
            let state = lock(&self.state);
            (state.approval_policy.clone(), state.sandbox_policy.clone())
        };
        if let Some(approval) = approval {
            params["approvalPolicy"] = json!(approval);
        }
        if let Some(sandbox) = sandbox {
            params["sandbox"] = json!(sandbox);
        }
        let response = self.client()?.request("thread/start", params).await?;
        let thread_id = response
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or_else(|| HarnessError::Protocol("thread/start returned no thread".to_owned()))?
            .to_owned();
        self.read_thread_config(&response);
        let session = SessionId(thread_id.clone());
        self.session = Some(session.clone());
        lock(&self.state).thread = Some(thread_id);
        Ok(session)
    }

    async fn load_session(&mut self, id: &SessionId) -> HarnessResult<()> {
        let response = self
            .client()?
            .request("thread/resume", json!({ "threadId": id.0 }))
            .await?;
        self.read_thread_config(&response);
        lock(&self.state).thread = Some(id.0.clone());
        self.session = Some(id.clone());
        Ok(())
    }

    async fn prompt(&mut self, parts: Vec<PromptPart>) -> HarnessResult<TurnHandle> {
        let thread = self.thread()?;
        let mut params = json!({
            "threadId": thread,
            "input": prompt_input(parts),
        });
        {
            let state = lock(&self.state);
            if let Some(model) = state.model.clone() {
                params["model"] = json!(model);
            }
            if let Some(effort) = state.effort.clone() {
                params["effort"] = json!(effort);
            }
            if let Some(approval) = state.approval_policy.clone() {
                params["approvalPolicy"] = json!(approval);
            }
            if let Some(sandbox) = state.sandbox_policy.clone() {
                params["sandboxPolicy"] = sandbox_policy(&sandbox);
            }
        }
        lock(&self.state).awaiting_turn = true;
        let response = self.client()?.request("turn/start", params).await;
        lock(&self.state).awaiting_turn = false;
        let response = response?;
        let turn_id = response
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or_else(|| HarnessError::Protocol("turn/start returned no turn".to_owned()))?
            .to_owned();
        lock(&self.state).turn = Some(turn_id.clone());
        self.bus.send(RawAgentEvent::Foreground {
            state: ForegroundState::Running,
        });
        Ok(TurnHandle { turn_id })
    }

    async fn cancel(&mut self, turn: &TurnHandle) -> HarnessResult<()> {
        let thread = self.thread()?;
        self.client()?
            .request(
                "turn/interrupt",
                json!({"threadId": thread, "turnId": turn.turn_id}),
            )
            .await?;
        lock(&self.state).turn = None;
        Ok(())
    }

    async fn decide(
        &mut self,
        request_id: &str,
        decision: ApprovalDecision,
        option_id: Option<&str>,
    ) -> HarnessResult<()> {
        let pending = lock(&self.state)
            .approvals
            .remove(request_id)
            .ok_or_else(|| HarnessError::RequestNotFound(request_id.to_owned()))?;
        let selected = match option_id {
            Some(id) => pending
                .options
                .iter()
                .find(|(_, option)| option == id)
                .map(|(_, option)| option.clone()),
            None => decision_option(decision, &pending.options),
        };
        let value = selected.unwrap_or_else(|| "denied".to_owned());
        let decision = if pending.method.starts_with("item/") {
            json!({ "decision": value })
        } else {
            json!({ "decision": legacy_decision(&value) })
        };
        self.client()?
            .send_value(json!({ "id": pending.request_id, "result": decision }))
    }

    async fn respond_elicitation(
        &mut self,
        _request_id: &str,
        _action: ElicitationAction,
        _values: Option<Value>,
    ) -> HarnessResult<()> {
        Err(HarnessError::Unavailable(
            "Codex stops MCP elicitations itself; threadbase does not answer them".to_owned(),
        ))
    }

    async fn cancel_subagent(&mut self, _subagent_id: &str) -> HarnessResult<()> {
        Err(HarnessError::Unavailable(
            "Codex subagents are cancelled by interrupting their turn".to_owned(),
        ))
    }

    fn config_options(&self) -> Vec<AgentConfigOptionDto> {
        let state = lock(&self.state);
        let mut options = Vec::new();
        if !state.models.is_empty() {
            options.push(AgentConfigOptionDto {
                id: "model".to_owned(),
                name: "Model".to_owned(),
                category: "model".to_owned(),
                current_value: state.model.clone().unwrap_or_else(|| {
                    state
                        .models
                        .first()
                        .map(|model| model.value.clone())
                        .unwrap_or_default()
                }),
                options: state.models.clone(),
            });
        }
        options.push(AgentConfigOptionDto {
            id: "thought_level".to_owned(),
            name: "Thinking".to_owned(),
            category: "thought_level".to_owned(),
            current_value: state.effort.clone().unwrap_or_else(|| "medium".to_owned()),
            options: ["minimal", "low", "medium", "high", "xhigh"]
                .iter()
                .map(|effort| AgentConfigValueDto {
                    value: (*effort).to_owned(),
                    name: (*effort).to_owned(),
                })
                .collect(),
        });
        options
    }

    async fn set_permission_mode(
        &mut self,
        mode: threadbase_shared::PermissionMode,
    ) -> HarnessResult<()> {
        self.apply_permission_mode(mode);
        Ok(())
    }

    async fn set_config(&mut self, model: Option<&str>, effort: Option<&str>) -> HarnessResult<()> {
        let mut state = lock(&self.state);
        if let Some(model) = model {
            state.model = Some(model.to_owned());
        }
        if let Some(effort) = effort {
            state.effort = Some(effort.to_owned());
        }
        Ok(())
    }

    async fn shutdown(&mut self) -> HarnessResult<()> {
        if let Some(stop) = self.shutdown.take() {
            let _ = stop.send(true);
        }
        for task in self.tasks.drain(..) {
            task.abort();
        }
        if let Some(driver) = self.driver.take() {
            let _ = driver.await;
        }
        self.bus.close();
        self.client = None;
        self.session = None;
        self.capabilities = Capabilities::default();
        Ok(())
    }

    fn events(&self) -> broadcast::Receiver<RawAgentEvent> {
        self.bus.subscribe()
    }

    async fn list_sessions(
        &mut self,
        cwd: Option<&std::path::Path>,
    ) -> HarnessResult<Vec<ExternalSession>> {
        let mut params = json!({ "limit": 100 });
        if let Some(cwd) = cwd {
            params["cwd"] = json!(cwd.to_string_lossy());
        }
        let response = self.client()?.request("thread/list", params).await?;
        let sessions = response
            .get("data")
            .and_then(Value::as_array)
            .map(|threads| threads.iter().map(external_session).collect())
            .unwrap_or_default();
        Ok(sessions)
    }

    async fn read_history(&mut self, id: &SessionId) -> HarnessResult<SessionHistory> {
        let mut history = SessionHistory::default();
        let mut cursor: Option<String> = None;
        for _ in 0..100 {
            let mut params = json!({
                "threadId": id.0,
                "sortDirection": "asc",
                "itemsView": "full",
                "limit": 50,
            });
            if let Some(cursor) = &cursor {
                params["cursor"] = json!(cursor);
            }
            let response = self.client()?.request("thread/turns/list", params).await?;
            for turn in response
                .get("data")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
            {
                let mut session = SessionTurn {
                    id: turn.get("id").and_then(Value::as_str).map(str::to_owned),
                    completed_at: turn
                        .get("completedAt")
                        .and_then(Value::as_i64)
                        .map(|seconds| seconds.saturating_mul(1000)),
                    ..SessionTurn::default()
                };
                for item in turn
                    .get("items")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                {
                    match item.get("type").and_then(Value::as_str).unwrap_or("") {
                        "userMessage" => session.user = user_text(&item),
                        "agentMessage" => {
                            if let Some(text) = item.get("text").and_then(Value::as_str) {
                                if !text.is_empty() {
                                    session.items.push(SessionItem::Message {
                                        role: MessageRole::Agent,
                                        text: text.to_owned(),
                                    });
                                }
                            }
                        }
                        "reasoning" | "hookPrompt" => {}
                        _ => session
                            .items
                            .push(SessionItem::Tool(Box::new(tool_event(&item)))),
                    }
                }
                if !session.user.is_empty() || !session.items.is_empty() {
                    history.turns.push(session);
                }
            }
            cursor = response
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(history)
    }

    async fn steer(&mut self, parts: Vec<PromptPart>) -> HarnessResult<SteeringOutcome> {
        let thread = self.thread()?;
        let turn = lock(&self.state)
            .turn
            .clone()
            .ok_or_else(|| HarnessError::Unavailable("no Codex turn is running".to_owned()))?;
        self.client()?
            .request(
                "turn/steer",
                json!({
                    "threadId": thread,
                    "expectedTurnId": turn,
                    "input": prompt_input(parts),
                }),
            )
            .await?;
        Ok(SteeringOutcome::Injected)
    }
}

impl CodexHarness {
    fn read_thread_config(&self, response: &Value) {
        let mut state = lock(&self.state);
        if let Some(model) = response.get("model").and_then(Value::as_str) {
            state.model = Some(model.to_owned());
        }
        if let Some(effort) = response.get("reasoningEffort").and_then(Value::as_str) {
            state.effort = Some(effort.to_owned());
        }
    }
}

impl Drop for CodexHarness {
    fn drop(&mut self) {
        if let Some(stop) = self.shutdown.take() {
            let _ = stop.send(true);
        }
        for task in self.tasks.drain(..) {
            task.abort();
        }
    }
}

fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `THREADBASE_CODEX_BIN` wins for tests; otherwise prefer the managed CLI the
/// Codex daemon already runs, because the PATH shim does not serve app-server.
fn codex_binary() -> String {
    if let Ok(path) = std::env::var("THREADBASE_CODEX_BIN") {
        if !path.trim().is_empty() {
            return path;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let managed =
            PathBuf::from(home).join(".codex/packages/app-server-daemon/current/bin/codex");
        if managed.is_file() {
            return managed.to_string_lossy().into_owned();
        }
    }
    "codex".to_owned()
}

struct Wire {
    client: Arc<Client>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    driver: tokio::task::JoinHandle<()>,
    shutdown: watch::Sender<bool>,
}

/// Prefer the shared daemon: the Codex TUI talks WebSocket over its control
/// socket, so a WS client joins the same threads. Fall back to a private
/// app-server over stdio when no daemon is running (and for the test fixture).
async fn open_wire(
    ctx: &LaunchCtx,
    state: Arc<Mutex<State>>,
    bus: EventBus,
) -> HarnessResult<Wire> {
    let (outgoing, outgoing_rx) = mpsc::unbounded_channel::<Value>();
    let client = Arc::new(Client::new(outgoing));
    let pinned_binary = ctx
        .env
        .iter()
        .any(|(name, _)| name == "THREADBASE_CODEX_BIN")
        || std::env::var_os("THREADBASE_CODEX_BIN").is_some();
    let force_stdio = pinned_binary || std::env::var_os("THREADBASE_CODEX_STDIO").is_some();
    let socket = (!force_stdio).then(codex_socket).flatten();

    let mut tasks = Vec::new();
    let (stop_tx, stop_rx) = watch::channel(false);
    let driver = if let Some(socket) = socket {
        let stream = UnixStream::connect(&socket)
            .await
            .map_err(HarnessError::Io)?;
        let (ws, _response) = tokio_tungstenite::client_async("ws://localhost/", stream)
            .await
            .map_err(|error| HarnessError::Protocol(error.to_string()))?;
        let (mut sink, mut source) = ws.split();
        let mut write_rx = outgoing_rx;
        tasks.push(tokio::spawn(async move {
            while let Some(message) = write_rx.recv().await {
                if sink.send(Message::text(message.to_string())).await.is_err() {
                    break;
                }
            }
        }));
        let read_client = client.clone();
        let read_state = state.clone();
        let read_bus = bus.clone();
        tasks.push(tokio::spawn(async move {
            while let Some(Ok(message)) = source.next().await {
                match message {
                    Message::Text(text) => {
                        dispatch(&read_client, &read_state, &read_bus, text.as_str());
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            read_bus.close();
        }));
        let mut stop = stop_rx.clone();
        tokio::spawn(async move {
            let _ = stop.changed().await;
        })
    } else {
        let binary = ctx
            .env
            .iter()
            .find(|(name, _)| name == "THREADBASE_CODEX_BIN")
            .map(|(_, value)| value.clone())
            .unwrap_or_else(codex_binary);
        let mut command = Command::new(binary);
        command
            .args(["app-server", "-c", "thread_unload_delay_secs=0"])
            .current_dir(&ctx.cwd)
            .envs(
                ctx.env
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str())),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                HarnessError::Unavailable(
                    "the codex CLI was not found; install it or set THREADBASE_CODEX_BIN"
                        .to_owned(),
                )
            } else {
                HarnessError::Io(error)
            }
        })?;
        if let Some(stderr) = child.stderr.take() {
            spawn_stderr_reader(stderr, bus.clone());
        }
        let stdin = child.stdin.take().expect("codex stdin was requested piped");
        let stdout = child
            .stdout
            .take()
            .expect("codex stdout was requested piped");
        let mut write_rx = outgoing_rx;
        tasks.push(tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(message) = write_rx.recv().await {
                let mut line = message.to_string();
                line.push('\n');
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
        }));
        let read_client = client.clone();
        let read_state = state.clone();
        let read_bus = bus.clone();
        tasks.push(tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                dispatch(&read_client, &read_state, &read_bus, &line);
            }
            read_bus.close();
        }));
        spawn_process_monitor(child, stop_rx.clone(), bus.clone())
    };

    Ok(Wire {
        client,
        tasks,
        driver,
        shutdown: stop_tx,
    })
}

fn codex_socket() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_APP_SERVER_SOCKET") {
        let path = PathBuf::from(path);
        return path.exists().then_some(path);
    }
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))?;
    let path = home.join("app-server-control/app-server-control.sock");
    path.exists().then_some(path)
}

/// One incoming JSON message: a response, a server request or a notification.
fn dispatch(client: &Client, state: &Mutex<State>, bus: &EventBus, line: &str) {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        bus.send(RawAgentEvent::Error {
            message: "malformed JSON-RPC message from codex".to_owned(),
        });
        return;
    };
    let Some(object) = message.as_object() else {
        return;
    };
    if let Some(id) = object.get("id") {
        if !object.contains_key("method") {
            let Some(id) = id.as_u64() else {
                return;
            };
            let result = match object.get("error") {
                Some(error) => Err(error.to_string()),
                None => Ok(object.get("result").cloned().unwrap_or(Value::Null)),
            };
            if let Some(tx) = client
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&id)
            {
                let _ = tx.send(result);
            }
            return;
        }
        let method = object
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        handle_server_request(state, bus, client, id, method, object.get("params"));
        return;
    }
    if let Some(method) = object.get("method").and_then(Value::as_str) {
        handle_notification(state, bus, method, object.get("params"));
    }
}

fn spawn_process_monitor(
    mut child: Child,
    mut stop: watch::Receiver<bool>,
    bus: EventBus,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let status = tokio::select! {
            status = child.wait() => status,
            _ = stop.changed() => {
                let _ = child.start_kill();
                child.wait().await
            }
        };
        if let Ok(status) = status {
            bus.send(RawAgentEvent::ProcessExited {
                code: status.code(),
                signal: None,
            });
        }
        bus.close();
    })
}

fn spawn_stderr_reader(stderr: tokio::process::ChildStderr, bus: EventBus) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if !line.trim().is_empty() {
                bus.send(RawAgentEvent::AgentLog {
                    level: "info".to_owned(),
                    text: line,
                });
            }
        }
    });
}

fn identity(user_agent: &str) -> crate::AgentIdentity {
    let mut parts = user_agent.split_whitespace();
    let first = parts.next().unwrap_or("codex");
    let (_, version) = first
        .split_once('/')
        .map_or((first, "unknown"), |(_, version)| (first, version));
    crate::AgentIdentity {
        name: "codex".to_owned(),
        version: version.to_owned(),
    }
}

fn prompt_input(parts: Vec<PromptPart>) -> Value {
    let text = parts
        .into_iter()
        .filter_map(|part| match part {
            PromptPart::Text(text) => Some(text),
            PromptPart::Image { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    json!([{ "type": "text", "text": text }])
}

fn sandbox_policy(sandbox: &str) -> Value {
    match sandbox {
        "danger-full-access" => json!({"type": "dangerFullAccess"}),
        "workspace-write" => json!({"type": "workspaceWrite"}),
        _ => json!({"type": "readOnly"}),
    }
}

fn model_options(response: &Value) -> Vec<AgentConfigValueDto> {
    let entries = response
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| response.get("models").and_then(Value::as_array));
    let mut seen = std::collections::HashSet::new();
    entries
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let value = entry
                .get("id")
                .or_else(|| entry.get("model"))
                .and_then(Value::as_str)?;
            if !seen.insert(value.to_owned()) {
                return None;
            }
            let name = entry
                .get("displayName")
                .or_else(|| entry.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(value);
            Some(AgentConfigValueDto {
                value: value.to_owned(),
                name: name.to_owned(),
            })
        })
        .collect()
}

fn external_session(thread: &Value) -> ExternalSession {
    let id = thread
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let title = thread
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .or_else(|| thread.get("preview").and_then(Value::as_str))
        .unwrap_or("Untitled thread");
    ExternalSession {
        id,
        title: title.chars().take(120).collect(),
        cwd: thread
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        updated_at: thread.get("updatedAt").and_then(Value::as_i64).unwrap_or(0) * 1000,
    }
}

fn user_text(item: &Value) -> String {
    item.get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn decision_option(decision: ApprovalDecision, options: &[(String, String)]) -> Option<String> {
    let preferred: &[&str] = match decision {
        ApprovalDecision::AllowOnce => &["accept", "approved"],
        ApprovalDecision::AllowSession | ApprovalDecision::AllowThread => {
            &["acceptForSession", "approved_for_session"]
        }
        ApprovalDecision::Deny => &["decline", "denied"],
    };
    preferred.iter().find_map(|kind| {
        options
            .iter()
            .find(|(option, _)| option == kind)
            .map(|(_, id)| id.clone())
    })
}

fn legacy_decision(value: &str) -> &str {
    match value {
        "accept" => "approved",
        "acceptForSession" => "approved_for_session",
        "decline" => "denied",
        "cancel" => "abort",
        other => other,
    }
}

fn handle_server_request(
    state: &Mutex<State>,
    bus: &EventBus,
    client: &Client,
    id: &Value,
    method: &str,
    params: Option<&Value>,
) {
    let params = params.cloned().unwrap_or(Value::Null);
    let (kind, options, title, command, cwd, tool_call_id) = match method {
        "item/commandExecution/requestApproval" | "execCommandApproval" => (
            ToolKind::Execute,
            approval_options(&["accept", "acceptForSession", "decline", "cancel"]),
            params
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("Run command")
                .to_owned(),
            params
                .get("command")
                .and_then(Value::as_str)
                .map(str::to_owned),
            params.get("cwd").and_then(Value::as_str).map(str::to_owned),
            params
                .get("itemId")
                .and_then(Value::as_str)
                .map(str::to_owned),
        ),
        "item/fileChange/requestApproval" | "applyPatchApproval" => (
            ToolKind::Edit,
            approval_options(&["accept", "acceptForSession", "decline", "cancel"]),
            "Edit files".to_owned(),
            None,
            params.get("cwd").and_then(Value::as_str).map(str::to_owned),
            params
                .get("itemId")
                .and_then(Value::as_str)
                .map(str::to_owned),
        ),
        "item/permissions/requestApproval" => (
            ToolKind::Other,
            approval_options(&["accept", "acceptForSession", "decline", "cancel"]),
            "Grant permissions".to_owned(),
            None,
            params.get("cwd").and_then(Value::as_str).map(str::to_owned),
            params
                .get("itemId")
                .and_then(Value::as_str)
                .map(str::to_owned),
        ),
        // Threadbase does not answer MCP elicitations; cancelling keeps Codex moving.
        "mcpServer/elicitation/request" => {
            let _ = client.send_value(json!({"id": id, "result": {"action": "cancel"}}));
            return;
        }
        _ => {
            let _ = client.send_value(
                json!({"id": id, "error": {"code": -32601, "message": "unsupported request"}}),
            );
            return;
        }
    };

    let request_id = format!("codex-approval-{}", ulid::Ulid::new());
    let (paths, description) = {
        let mut state = lock(state);
        let paths = tool_call_id
            .as_ref()
            .and_then(|item| state.tools.get(item))
            .map(|tool| tool.locations.iter().map(|l| l.path.clone()).collect())
            .unwrap_or_default();
        state.approvals.insert(
            request_id.clone(),
            PendingApproval {
                request_id: id.clone(),
                method: method.to_owned(),
                options: options
                    .iter()
                    .map(|option| (option.kind.clone(), option.id.clone()))
                    .collect(),
            },
        );
        (
            paths,
            params
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_owned),
        )
    };
    bus.send(RawAgentEvent::PermissionRequest {
        request: PermissionRequest {
            id: request_id,
            title,
            description,
            kind,
            command,
            cwd,
            paths,
            options,
            raw: params,
        },
    });
}

fn approval_options(values: &[&str]) -> Vec<PermissionOption> {
    values
        .iter()
        .map(|value| {
            let (kind, label) = match *value {
                "accept" => ("allow_once", "Allow once"),
                "acceptForSession" => ("allow_always", "Allow for this session"),
                "decline" => ("reject_once", "Deny"),
                "cancel" => ("reject_always", "Stop the turn"),
                _ => ("other", *value),
            };
            PermissionOption {
                id: (*value).to_owned(),
                kind: kind.to_owned(),
                label: label.to_owned(),
            }
        })
        .collect()
}

fn handle_notification(state: &Mutex<State>, bus: &EventBus, method: &str, params: Option<&Value>) {
    let params = params.cloned().unwrap_or(Value::Null);
    match method {
        "turn/started" => {
            // A turn started while threadbase has none of its own was started
            // by another client on the shared daemon. Mirror it and say who.
            let external = {
                let mut state = lock(state);
                let external = !state.awaiting_turn;
                state.awaiting_turn = false;
                state.owning = !external;
                external
            };
            if let Some(turn) = params.pointer("/turn/id").and_then(Value::as_str) {
                lock(state).turn = Some(turn.to_owned());
            }
            if external {
                bus.send(RawAgentEvent::Notice {
                    level: "info".to_owned(),
                    message:
                        "Another client started a turn in this thread; threadbase is mirroring it."
                            .to_owned(),
                });
            }
            bus.send(RawAgentEvent::Foreground {
                state: ForegroundState::Running,
            });
        }
        "turn/completed" => {
            let turn = params.get("turn").cloned().unwrap_or(Value::Null);
            let status = turn.get("status").and_then(Value::as_str).unwrap_or("");
            let stop_reason = match status {
                "interrupted" => "cancelled",
                "failed" => "error",
                _ => "end_turn",
            };
            if let Some(error) = turn.pointer("/error/message").and_then(Value::as_str) {
                bus.send(RawAgentEvent::Notice {
                    level: "error".to_owned(),
                    message: error.to_owned(),
                });
            }
            {
                let mut state = lock(state);
                state.turn = None;
                state.awaiting_turn = false;
                state.owning = false;
            }
            if let Some(id) = turn.get("id").and_then(Value::as_str) {
                bus.send(RawAgentEvent::NativeTurn { id: id.to_owned() });
            }
            bus.send(RawAgentEvent::PeerActivity { active: false });
            bus.send(RawAgentEvent::Foreground {
                state: ForegroundState::Idle {
                    stop_reason: Some(stop_reason.to_owned()),
                    usage: None,
                },
            });
        }
        "item/agentMessage/delta" => {
            if let (Some(item), Some(delta)) = (
                params.get("itemId").and_then(Value::as_str),
                params.get("delta").and_then(Value::as_str),
            ) {
                bus.send(RawAgentEvent::MessageChunk {
                    role: MessageRole::Agent,
                    message_id: item.to_owned(),
                    text: delta.to_owned(),
                });
            }
        }
        "item/reasoning/textDelta" | "item/reasoning/summaryTextDelta" => {
            if let (Some(item), Some(delta)) = (
                params.get("itemId").and_then(Value::as_str),
                params.get("delta").and_then(Value::as_str),
            ) {
                bus.send(RawAgentEvent::MessageChunk {
                    role: MessageRole::Thought,
                    message_id: item.to_owned(),
                    text: delta.to_owned(),
                });
            }
        }
        "item/commandExecution/outputDelta" => {
            if let (Some(item), Some(delta)) = (
                params.get("itemId").and_then(Value::as_str),
                params.get("delta").and_then(Value::as_str),
            ) {
                if let Some(tool) = append_output(state, item, delta) {
                    bus.send(RawAgentEvent::ToolUpsert { call: tool });
                }
            }
        }
        "item/fileChange/patchUpdated" => {
            if let (Some(item), Some(changes)) = (
                params.get("itemId").and_then(Value::as_str),
                params.get("changes").and_then(Value::as_array),
            ) {
                let mut state = lock(state);
                if let Some(tool) = state.tools.get_mut(item) {
                    tool.diff = Some(diff_from_changes(changes));
                    bus.send(RawAgentEvent::ToolUpsert { call: tool.clone() });
                }
            }
        }
        "item/started" | "item/completed" => {
            let Some(item) = params.get("item") else {
                return;
            };
            let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
            if item_type == "agentMessage" {
                if let (Some(id), Some(text)) = (
                    item.get("id").and_then(Value::as_str),
                    item.get("text").and_then(Value::as_str),
                ) {
                    bus.send(RawAgentEvent::MessageUpsert {
                        role: MessageRole::Agent,
                        message_id: id.to_owned(),
                        text: Some(text.to_owned()),
                    });
                }
                return;
            }
            if matches!(
                item_type,
                "userMessage" | "reasoning" | "plan" | "hookPrompt"
            ) {
                return;
            }
            let event = tool_event(item);
            lock(state).tools.insert(event.id.clone(), event.clone());
            bus.send(RawAgentEvent::ToolUpsert { call: event });
        }
        "turn/plan/updated" => {
            let entries = params
                .get("plan")
                .and_then(Value::as_array)
                .map(|steps| {
                    steps
                        .iter()
                        .map(|step| PlanEntry {
                            content: step
                                .get("step")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned(),
                            priority: PlanPriority::Medium,
                            status: match step.get("status").and_then(Value::as_str) {
                                Some("inProgress") => PlanStatus::InProgress,
                                Some("completed") => PlanStatus::Completed,
                                _ => PlanStatus::Pending,
                            },
                        })
                        .collect()
                })
                .unwrap_or_default();
            bus.send(RawAgentEvent::Plan { entries });
        }
        "thread/tokenUsage/updated" => {
            let usage = &params["tokenUsage"]["last"];
            bus.send(RawAgentEvent::Usage {
                usage: Usage {
                    input_tokens: usage
                        .get("inputTokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    output_tokens: usage
                        .get("outputTokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    total_tokens: usage
                        .get("totalTokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    context_used: Some(
                        usage
                            .get("totalTokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    ),
                    context_size: params
                        .pointer("/tokenUsage/modelContextWindow")
                        .and_then(Value::as_u64),
                },
            });
        }
        "thread/status/changed" => {
            let status = params.get("status").cloned().unwrap_or(Value::Null);
            let kind = status
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let waiting = status
                .get("activeFlags")
                .and_then(Value::as_array)
                .map(|flags| {
                    flags.iter().any(|flag| {
                        matches!(
                            flag.as_str(),
                            Some("waitingOnApproval" | "waitingOnUserInput")
                        )
                    })
                })
                .unwrap_or(false);
            let key = format!("{kind}:{waiting}");
            let (changed, owning, has_approvals) = {
                let mut state = lock(state);
                let changed = state.last_status.as_deref() != Some(key.as_str());
                state.last_status = Some(key);
                (changed, state.owning, !state.approvals.is_empty())
            };
            if !changed {
                return;
            }
            if kind == "systemError" {
                bus.send(RawAgentEvent::Notice {
                    level: "error".to_owned(),
                    message: "The Codex thread reported a system error.".to_owned(),
                });
            } else if waiting && !owning && !has_approvals {
                bus.send(RawAgentEvent::Notice {
                    level: "info".to_owned(),
                    message:
                        "Another client is waiting on approval in this thread; answer it there."
                            .to_owned(),
                });
            }
        }
        "warning" => {
            if let Some(message) = params.get("message").and_then(Value::as_str) {
                bus.send(RawAgentEvent::Notice {
                    level: "warning".to_owned(),
                    message: message.to_owned(),
                });
            }
        }
        "error" => {
            let message = params
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Codex reported an error");
            bus.send(RawAgentEvent::Notice {
                level: "error".to_owned(),
                message: message.to_owned(),
            });
        }
        _ => {}
    }
}

fn append_output(state: &Mutex<State>, item: &str, delta: &str) -> Option<ToolCallEvent> {
    let mut state = lock(state);
    let tool = state.tools.get_mut(item)?;
    let mut output = tool.output.take().unwrap_or_default();
    output.push_str(delta);
    tool.output = Some(output);
    Some(tool.clone())
}

fn tool_event(item: &Value) -> ToolCallEvent {
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
    let status = tool_status(item.get("status").and_then(Value::as_str));
    let mut tool = ToolCallEvent {
        id: id.clone(),
        title: item_type.to_owned(),
        kind: ToolKind::Other,
        status,
        input: None,
        output: None,
        locations: Vec::new(),
        terminal_id: None,
        diff: None,
        raw: item.clone(),
    };
    match item_type {
        "commandExecution" => {
            tool.kind = ToolKind::Execute;
            tool.title = item
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("Run command")
                .to_owned();
            tool.input = item.get("command").cloned().map(|value| value.to_string());
            tool.output = item
                .get("aggregatedOutput")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let Some(cwd) = item.get("cwd").and_then(Value::as_str) {
                tool.locations = vec![ToolLocation {
                    path: cwd.to_owned(),
                    line: None,
                }];
            }
        }
        "fileChange" => {
            tool.kind = ToolKind::Edit;
            let changes = item
                .get("changes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if let Some(first) = changes.first() {
                tool.title = format!(
                    "Edit {}",
                    first.get("path").and_then(Value::as_str).unwrap_or("file")
                );
            }
            tool.locations = changes
                .iter()
                .filter_map(|change| change.get("path").and_then(Value::as_str))
                .map(|path| ToolLocation {
                    path: path.to_owned(),
                    line: None,
                })
                .collect();
            tool.diff = Some(diff_from_changes(&changes));
        }
        "mcpToolCall" => {
            tool.kind = ToolKind::Other;
            tool.title = format!(
                "{}.{}",
                item.get("server").and_then(Value::as_str).unwrap_or("mcp"),
                item.get("tool").and_then(Value::as_str).unwrap_or("tool")
            );
            tool.input = item
                .get("arguments")
                .cloned()
                .map(|value| value.to_string());
            tool.output = item
                .get("result")
                .or_else(|| item.get("error"))
                .cloned()
                .map(|value| value.to_string());
        }
        "webSearch" => {
            tool.kind = ToolKind::Search;
            tool.title = format!(
                "Search {}",
                item.get("query")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            );
        }
        "imageView" => {
            tool.kind = ToolKind::Read;
            tool.title = format!(
                "View {}",
                item.get("path").and_then(Value::as_str).unwrap_or("image")
            );
            if let Some(path) = item.get("path").and_then(Value::as_str) {
                tool.locations = vec![ToolLocation {
                    path: path.to_owned(),
                    line: None,
                }];
            }
        }
        "functionCallOutput" => {
            tool.title = item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Tool")
                .to_owned();
            tool.output = item.get("output").cloned().map(|value| value.to_string());
        }
        "collabAgentToolCall" => {
            tool.title = format!(
                "{} agents",
                item.get("receiverThreadIds")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len)
            );
        }
        "subAgentActivity" => {
            tool.title = format!(
                "Subagent {}",
                item.get("agentThreadId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            );
        }
        other => tool.title = other.to_owned(),
    }
    tool
}

fn diff_from_changes(changes: &[Value]) -> DiffSummary {
    let mut patch = String::new();
    let mut operations = Vec::new();
    for change in changes {
        let path = change
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if let Some(diff) = change.get("diff").and_then(Value::as_str) {
            patch.push_str(diff);
            if !diff.ends_with('\n') {
                patch.push('\n');
            }
        }
        operations.push(DiffChangeDto {
            operation: match change.get("kind").and_then(Value::as_str) {
                Some("add") => "add",
                Some("delete") => "delete",
                _ => "modify",
            }
            .to_owned(),
            path,
            old_path: None,
            file_type: Some("text".to_owned()),
        });
    }
    let (additions, deletions) = patch_line_counts(&patch);
    DiffSummary {
        path: operations
            .first()
            .map(|change| change.path.clone())
            .unwrap_or_default(),
        additions,
        deletions,
        patch: (!patch.is_empty()).then_some(patch),
        changes: operations,
    }
}

/// Count added and removed lines in a `git_patch`-format diff.
fn patch_line_counts(patch: &str) -> (u32, u32) {
    let mut additions = 0;
    let mut deletions = 0;
    for line in patch.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            additions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }
    (additions, deletions)
}

fn tool_status(status: Option<&str>) -> ToolStatus {
    match status {
        Some("inProgress") => ToolStatus::InProgress,
        Some("completed") => ToolStatus::Completed,
        Some("failed") | Some("declined") => ToolStatus::Failed,
        _ => ToolStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_decisions_map_to_codex_names() {
        assert_eq!(legacy_decision("accept"), "approved");
        assert_eq!(legacy_decision("acceptForSession"), "approved_for_session");
        assert_eq!(legacy_decision("decline"), "denied");
        assert_eq!(legacy_decision("cancel"), "abort");
    }

    #[test]
    fn file_changes_become_a_structured_diff() {
        let changes = vec![json!({
            "path": "/tmp/a.rs",
            "kind": "update",
            "diff": "--- a\n+++ b\n@@ -1 +1 @@\n-a\n+b\n",
        })];
        let diff = diff_from_changes(&changes);
        assert_eq!(diff.path, "/tmp/a.rs");
        assert_eq!((diff.additions, diff.deletions), (1, 1));
        assert_eq!(diff.changes[0].operation, "modify");
    }

    #[test]
    fn identity_parses_the_user_agent() {
        let identity = identity("threadbase/0.171.0 (Arch Linux) unknown (threadbase; 0.1.0)");
        assert_eq!(identity.name, "codex");
        assert_eq!(identity.version, "0.171.0");
    }
}
