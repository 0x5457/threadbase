//! Claude Code harness over its stream-json control protocol.
//!
//! Claude ships a bidirectional JSON protocol for print mode: user messages go
//! in on stdin, session messages and control requests come out on stdout.
//! Permissions are answered over the same channel, so the transcript and the
//! approval cards are native.
//!
//! A session held by a background Claude process cannot be resumed (Claude
//! returns `session_held_by_background`), so this transport is single owner:
//! the CLI can attach only while threadbase is not running the session.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

use crate::bus::EventBus;
use crate::claude_sessions;
use crate::profile::AgentProfile;
use crate::{
    AgentIdentity, Capabilities, ElicitationAction, ForegroundState, Harness, HarnessError,
    HarnessResult, LaunchCtx, MessageRole, NewSession, PermissionOption, PermissionRequest,
    PromptPart, RawAgentEvent, SessionId, SteeringOutcome, SubagentEvent, ToolCallEvent,
    TurnHandle,
};
use threadbase_shared::{
    AgentConfigOptionDto, AgentConfigValueDto, ApprovalDecision, PermissionMode, SubagentStatus,
    Usage,
};

const CONTROL_TIMEOUT: Duration = Duration::from_secs(20);

struct PendingApproval {
    request_id: String,
    tool_name: String,
    input: Value,
    options: Vec<(String, String)>,
}

#[derive(Default)]
struct State {
    session: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    models: Vec<AgentConfigValueDto>,
    runtime: Option<Runtime>,
    /// A session to open on the first prompt, once flags are known.
    pending: Option<PendingSession>,
    approvals: HashMap<String, PendingApproval>,
    tools: HashMap<String, ToolCallEvent>,
    controls: HashMap<String, oneshot::Sender<Value>>,
    /// Message uuids whose text already streamed, so the whole-message frame
    /// does not repeat it.
    streamed: HashSet<String>,
    /// Task tool uses that are still running, by tool-use id.
    subagents: HashMap<String, SubagentEvent>,
    cancelled: bool,
    turn_seq: u64,
    running_turn: Option<String>,
}

enum PendingSession {
    New(String),
    Resume(String),
}

struct Runtime {
    outgoing: mpsc::UnboundedSender<Value>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    stop: watch::Sender<bool>,
}

type Shared = Arc<Mutex<State>>;

fn lock(state: &Shared) -> std::sync::MutexGuard<'_, State> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One Claude Code process per session, started on the first prompt.
pub struct ClaudeHarness {
    profile: AgentProfile,
    bus: EventBus,
    state: Shared,
    cwd: Option<PathBuf>,
    env: Vec<(String, String)>,
    capabilities: Capabilities,
    agent_info: Option<AgentIdentity>,
    session: Option<SessionId>,
    model: Option<String>,
    effort: Option<String>,
    mode: PermissionMode,
}

impl ClaudeHarness {
    #[must_use]
    pub fn new(profile: AgentProfile) -> Self {
        Self {
            profile,
            bus: EventBus::new(),
            state: Arc::new(Mutex::new(State::default())),
            cwd: None,
            env: Vec::new(),
            capabilities: Capabilities::default(),
            agent_info: None,
            session: None,
            model: None,
            effort: None,
            mode: PermissionMode::Supervised,
        }
    }

    /// Capabilities observed while starting the harness.
    #[must_use]
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// Agent identity reported by the CLI.
    #[must_use]
    pub fn agent_info(&self) -> Option<&AgentIdentity> {
        self.agent_info.as_ref()
    }

    fn binary(&self) -> String {
        self.env
            .iter()
            .find(|(name, _)| name == "THREADBASE_CLAUDE_BIN")
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var("THREADBASE_CLAUDE_BIN").ok())
            .unwrap_or_else(|| "claude".to_owned())
    }

    async fn ensure_started(&mut self) -> HarnessResult<()> {
        if lock(&self.state).runtime.is_some() {
            return Ok(());
        }
        let pending = lock(&self.state).pending.take().ok_or_else(|| {
            HarnessError::SessionNotFound("no Claude session has been opened".to_owned())
        })?;
        let mut command = Command::new(self.binary());
        command.args([
            "--print",
            "--output-format",
            "stream-json",
            "--input-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--permission-prompts",
            "host",
        ]);
        match &pending {
            PendingSession::New(id) => {
                command.args(["--session-id", id]);
            }
            PendingSession::Resume(id) => {
                command.args(["--resume", id]);
            }
        }
        if let Some(model) = lock(&self.state).model.clone() {
            command.args(["--model", &model]);
        }
        if let Some(effort) = lock(&self.state).effort.clone() {
            command.args(["--effort", &effort]);
        }
        command.args(claude_mode_args(self.mode));
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        command
            .envs(self.env.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                HarnessError::Unavailable(
                    "the claude CLI was not found; install it or set THREADBASE_CLAUDE_BIN"
                        .to_owned(),
                )
            } else {
                HarnessError::Io(error)
            }
        })?;

        if let Some(stderr) = child.stderr.take() {
            spawn_stderr_reader(stderr, self.bus.clone());
        }
        let stdin = child
            .stdin
            .take()
            .expect("claude stdin was requested piped");
        let stdout = child
            .stdout
            .take()
            .expect("claude stdout was requested piped");
        let (outgoing, outgoing_rx) = mpsc::unbounded_channel::<Value>();
        let (stop_tx, stop_rx) = watch::channel(false);
        let tasks = vec![
            spawn_writer(stdin, outgoing_rx),
            spawn_reader(
                stdout,
                outgoing.clone(),
                self.state.clone(),
                self.bus.clone(),
            ),
            spawn_process_monitor(child, stop_rx, self.bus.clone()),
        ];
        lock(&self.state).runtime = Some(Runtime {
            outgoing,
            tasks,
            stop: stop_tx,
        });
        match self.control("initialize", json!({})).await {
            Ok(response) => {
                // A session that was already initialized elsewhere can hold
                // pending permission requests; surface them instead of dropping
                // them.
                if let Ok(models) = self.control("list_models", json!({})).await {
                    let parsed = model_options(&models);
                    if !parsed.is_empty() {
                        lock(&self.state).models = parsed;
                    }
                }
                if let Some(pending) = response
                    .pointer("/response/pending_permission_requests")
                    .and_then(Value::as_array)
                    .cloned()
                {
                    let outgoing = lock(&self.state)
                        .runtime
                        .as_ref()
                        .map(|runtime| runtime.outgoing.clone());
                    if let Some(outgoing) = outgoing {
                        for request in pending {
                            handle_message(&self.state, &self.bus, &outgoing, request);
                        }
                    }
                }
                Ok(())
            }
            Err(error) => {
                // A CLI that cannot initialize is unusable; drop the process so
                // the next prompt starts clean.
                let runtime = lock(&self.state).runtime.take();
                if let Some(runtime) = runtime {
                    let _ = runtime.stop.send(true);
                    for task in runtime.tasks {
                        task.abort();
                    }
                }
                Err(error)
            }
        }
    }

    async fn control(&self, subtype: &str, extra: Value) -> HarnessResult<Value> {
        let (outgoing, request_id) = {
            let state = lock(&self.state);
            let runtime = state.runtime.as_ref().ok_or(HarnessError::NotRunning)?;
            (
                runtime.outgoing.clone(),
                format!("tb-{}", ulid::Ulid::new()),
            )
        };
        let (tx, rx) = oneshot::channel();
        lock(&self.state).controls.insert(request_id.clone(), tx);
        let mut request = json!({"subtype": subtype});
        if let (Some(object), Some(extra)) = (request.as_object_mut(), extra.as_object()) {
            object.extend(extra.clone());
        }
        outgoing
            .send(json!({
                "type": "control_request",
                "request_id": request_id,
                "request": request,
            }))
            .map_err(|_| HarnessError::NotRunning)?;
        tokio::time::timeout(CONTROL_TIMEOUT, rx)
            .await
            .map_err(|_| HarnessError::Protocol(format!("claude did not answer {subtype}")))?
            .map_err(|_| HarnessError::NotRunning)
    }

    fn send_user(&self, parts: Vec<PromptPart>) -> HarnessResult<()> {
        let state = lock(&self.state);
        let runtime = state.runtime.as_ref().ok_or(HarnessError::NotRunning)?;
        let session = state.session.clone();
        runtime
            .outgoing
            .send(json!({
                "type": "user",
                "message": {"role": "user", "content": prompt_blocks(parts)},
                "parent_tool_use_id": null,
                "session_id": session,
            }))
            .map_err(|_| HarnessError::NotRunning)
    }
}

#[async_trait::async_trait]
impl Harness for ClaudeHarness {
    fn id(&self) -> &str {
        &self.profile.id
    }

    async fn start(&mut self, ctx: LaunchCtx) -> HarnessResult<Capabilities> {
        if self.session.is_some() {
            return Err(HarnessError::Protocol(
                "Claude harness is already running".to_owned(),
            ));
        }
        self.cwd = Some(ctx.cwd.clone());
        self.env.clone_from(&ctx.env);
        self.model.clone_from(&ctx.model);
        self.effort.clone_from(&ctx.effort);
        self.mode = ctx.permission_mode.unwrap_or(PermissionMode::Supervised);

        let output = Command::new(self.binary())
            .arg("--version")
            .output()
            .await
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    HarnessError::Unavailable(
                        "the claude CLI was not found; install it or set THREADBASE_CLAUDE_BIN"
                            .to_owned(),
                    )
                } else {
                    HarnessError::Io(error)
                }
            })?;
        let version = String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
            .unwrap_or("unknown")
            .to_owned();
        self.agent_info = Some(AgentIdentity {
            name: "claude".to_owned(),
            version,
        });
        self.capabilities = Capabilities {
            steering: true,
            images: true,
            subagents: false,
            elicitation_form: false,
            elicitation_url: false,
            terminal: false,
        };
        Ok(self.capabilities)
    }

    async fn new_session(&mut self, _req: NewSession) -> HarnessResult<SessionId> {
        let id = ulid::Ulid::new().to_string();
        {
            let mut state = lock(&self.state);
            state.session = Some(id.clone());
            state.pending = Some(PendingSession::New(id.clone()));
        }
        self.session = Some(SessionId(id.clone()));
        Ok(SessionId(id))
    }

    async fn load_session(&mut self, id: &SessionId) -> HarnessResult<()> {
        {
            let mut state = lock(&self.state);
            state.session = Some(id.0.clone());
            state.pending = Some(PendingSession::Resume(id.0.clone()));
        }
        self.session = Some(id.clone());
        Ok(())
    }

    async fn prompt(&mut self, parts: Vec<PromptPart>) -> HarnessResult<TurnHandle> {
        self.ensure_started().await?;
        let turn_id = {
            let mut state = lock(&self.state);
            state.turn_seq += 1;
            let turn_id = format!("turn-{}", state.turn_seq);
            state.running_turn = Some(turn_id.clone());
            state.cancelled = false;
            turn_id
        };
        self.send_user(parts)?;
        self.bus.send(RawAgentEvent::Foreground {
            state: ForegroundState::Running,
        });
        Ok(TurnHandle { turn_id })
    }

    async fn cancel(&mut self, turn: &TurnHandle) -> HarnessResult<()> {
        {
            let state = lock(&self.state);
            if let Some(running) = state.running_turn.as_deref() {
                if running != turn.turn_id {
                    return Err(HarnessError::Protocol(format!(
                        "turn {} is not running",
                        turn.turn_id
                    )));
                }
            }
        }
        lock(&self.state).cancelled = true;
        self.control("interrupt", json!({})).await?;
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
        let response = match selected.as_deref() {
            Some("allow") => json!({
                "behavior": "allow",
                "updatedInput": pending.input,
            }),
            Some("allow_always") => json!({
                "behavior": "allow",
                "updatedInput": pending.input,
                "updatedPermissions": [{
                    "type": "addRules",
                    "rules": [{"toolName": pending.tool_name}],
                    "behavior": "allow",
                    "destination": "session",
                }],
            }),
            _ => json!({
                "behavior": "deny",
                "message": "Denied by the user",
            }),
        };
        let outgoing = {
            let state = lock(&self.state);
            state
                .runtime
                .as_ref()
                .map(|runtime| runtime.outgoing.clone())
                .ok_or(HarnessError::NotRunning)?
        };
        outgoing
            .send(json!({
                "type": "control_response",
                "response": {
                    "subtype": "success",
                    "request_id": pending.request_id,
                    "response": response,
                },
            }))
            .map_err(|_| HarnessError::NotRunning)
    }

    async fn respond_elicitation(
        &mut self,
        _request_id: &str,
        _action: ElicitationAction,
        _values: Option<Value>,
    ) -> HarnessResult<()> {
        Err(HarnessError::Unavailable(
            "Claude elicitations are not exposed by this transport".to_owned(),
        ))
    }

    async fn cancel_subagent(&mut self, _subagent_id: &str) -> HarnessResult<()> {
        Err(HarnessError::Unavailable(
            "Claude subagents share the session's turn".to_owned(),
        ))
    }

    fn config_options(&self) -> Vec<AgentConfigOptionDto> {
        let state = lock(&self.state);
        let model = state.model.clone().unwrap_or_else(|| "default".to_owned());
        let models = if state.models.is_empty() {
            vec![AgentConfigValueDto {
                value: model.clone(),
                name: model.clone(),
            }]
        } else {
            state.models.clone()
        };
        vec![
            AgentConfigOptionDto {
                id: "model".to_owned(),
                name: "Model".to_owned(),
                category: "model".to_owned(),
                current_value: model,
                options: models,
            },
            AgentConfigOptionDto {
                id: "thought_level".to_owned(),
                name: "Thinking".to_owned(),
                category: "thought_level".to_owned(),
                current_value: state.effort.clone().unwrap_or_else(|| "medium".to_owned()),
                options: ["low", "medium", "high", "xhigh", "max"]
                    .iter()
                    .map(|effort| AgentConfigValueDto {
                        value: (*effort).to_owned(),
                        name: (*effort).to_owned(),
                    })
                    .collect(),
            },
        ]
    }

    async fn set_permission_mode(&mut self, mode: PermissionMode) -> HarnessResult<()> {
        self.mode = mode;
        if lock(&self.state).runtime.is_some() {
            self.control("set_permission_mode", json!({"mode": claude_mode(mode)}))
                .await?;
        }
        Ok(())
    }

    async fn set_config(&mut self, model: Option<&str>, effort: Option<&str>) -> HarnessResult<()> {
        let running = lock(&self.state).runtime.is_some();
        if running && effort.is_some() {
            return Err(HarnessError::Unavailable(
                "Claude fixes the effort for the life of a session; start a new session to change it"
                    .to_owned(),
            ));
        }
        {
            let mut state = lock(&self.state);
            if let Some(model) = model {
                state.model = Some(model.to_owned());
                self.model = Some(model.to_owned());
            }
            if let Some(effort) = effort {
                state.effort = Some(effort.to_owned());
                self.effort = Some(effort.to_owned());
            }
        }
        if running {
            if let Some(model) = model {
                self.control("set_model", json!({"model": model})).await?;
            }
        }
        Ok(())
    }

    async fn shutdown(&mut self) -> HarnessResult<()> {
        let runtime = lock(&self.state).runtime.take();
        if let Some(runtime) = runtime {
            let _ = runtime.stop.send(true);
            for task in runtime.tasks {
                task.abort();
            }
        }
        self.bus.close();
        self.session = None;
        self.capabilities = Capabilities::default();
        Ok(())
    }

    fn events(&self) -> broadcast::Receiver<RawAgentEvent> {
        self.bus.subscribe()
    }

    async fn read_history(&mut self, id: &SessionId) -> HarnessResult<crate::SessionHistory> {
        let cwd = self.cwd.clone().ok_or(HarnessError::NotRunning)?;
        claude_sessions::read(&cwd, &id.0)
    }

    async fn steer(&mut self, parts: Vec<PromptPart>) -> HarnessResult<SteeringOutcome> {
        self.ensure_started().await?;
        self.send_user(parts)?;
        Ok(SteeringOutcome::Injected)
    }
}

/// Models from a `list_models` control response. The payload is not in the
/// published types, so accept the common shapes and ignore anything else.
fn model_options(response: &Value) -> Vec<AgentConfigValueDto> {
    let entries = response
        .pointer("/response/models")
        .and_then(Value::as_array)
        .or_else(|| response.pointer("/response/data").and_then(Value::as_array))
        .or_else(|| response.get("response").and_then(Value::as_array));
    let mut seen = HashSet::new();
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
                .or_else(|| entry.get("display_name"))
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

/// Claude names the delegating tool Task in older builds and Agent in newer
/// ones; both carry the same `description`/`prompt` input.
fn is_subagent_tool(name: &str) -> bool {
    matches!(name, "Task" | "Agent")
}

/// Usage from a completed Task tool's `tool_use_result`. The shape is
/// per-tool, so accept the common token keys and ignore anything else.
fn subagent_usage(value: &Value) -> Option<Usage> {
    let usage = value.get("usage").unwrap_or(value);
    let input_tokens = usage
        .get("inputTokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(Value::as_u64);
    let output_tokens = usage
        .get("outputTokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_u64);
    let total_tokens = usage
        .get("totalTokens")
        .or_else(|| usage.get("total_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| Some(input_tokens? + output_tokens?));
    if input_tokens.is_none() && output_tokens.is_none() && total_tokens.is_none() {
        return None;
    }
    Some(Usage {
        input_tokens: input_tokens.unwrap_or(0),
        output_tokens: output_tokens.unwrap_or(0),
        total_tokens: total_tokens.unwrap_or(0),
        context_used: None,
        context_size: None,
    })
}

fn claude_mode(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Supervised => "default",
        PermissionMode::AutoEdit => "acceptEdits",
        PermissionMode::FullAccess => "bypassPermissions",
    }
}

fn claude_mode_args(mode: PermissionMode) -> &'static [&'static str] {
    match mode {
        PermissionMode::Supervised => &["--permission-mode", "default"],
        PermissionMode::AutoEdit => &["--permission-mode", "acceptEdits"],
        PermissionMode::FullAccess => &[
            "--permission-mode",
            "bypassPermissions",
            "--allow-dangerously-skip-permissions",
        ],
    }
}

fn prompt_blocks(parts: Vec<PromptPart>) -> Value {
    Value::Array(
        parts
            .into_iter()
            .map(|part| match part {
                PromptPart::Text(text) => json!({"type": "text", "text": text}),
                PromptPart::Image { mime_type, data } => json!({
                    "type": "image",
                    "source": {"type": "base64", "media_type": mime_type, "data": data},
                }),
            })
            .collect(),
    )
}

fn decision_option(decision: ApprovalDecision, options: &[(String, String)]) -> Option<String> {
    let preferred: &[&str] = match decision {
        ApprovalDecision::AllowOnce => &["allow_once", "allow_always"],
        ApprovalDecision::AllowSession | ApprovalDecision::AllowThread => {
            &["allow_always", "allow_once"]
        }
        ApprovalDecision::Deny => &["deny"],
    };
    preferred.iter().find_map(|kind| {
        options
            .iter()
            .find(|(option, _)| option == kind)
            .map(|(_, id)| id.clone())
    })
}

fn spawn_writer(
    mut stdin: tokio::process::ChildStdin,
    mut rx: mpsc::UnboundedReceiver<Value>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let mut line = message.to_string();
            line.push('\n');
            if stdin.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            let _ = stdin.flush().await;
        }
    })
}

fn spawn_reader(
    stdout: tokio::process::ChildStdout,
    outgoing: mpsc::UnboundedSender<Value>,
    state: Shared,
    bus: EventBus,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                bus.send(RawAgentEvent::AgentLog {
                    level: "warning".to_owned(),
                    text: "claude emitted a non-JSON line".to_owned(),
                });
                continue;
            };
            handle_message(&state, &bus, &outgoing, message);
        }
        bus.close();
    })
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
        let mut held_reported = false;
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            if !held_reported && line.contains("session_held_by_background") {
                held_reported = true;
                bus.send(RawAgentEvent::Notice {
                    level: "warning".to_owned(),
                    message: "This Claude session is running in another client; stop it there before resuming here."
                        .to_owned(),
                });
            }
            bus.send(RawAgentEvent::AgentLog {
                level: "info".to_owned(),
                text: line,
            });
        }
    });
}

fn handle_message(
    state: &Shared,
    bus: &EventBus,
    outgoing: &mpsc::UnboundedSender<Value>,
    message: Value,
) {
    match message.get("type").and_then(Value::as_str) {
        Some("system") => system_message(state, &message),
        Some("stream_event") => stream_event(state, bus, &message),
        Some("assistant") => assistant_message(state, bus, &message),
        Some("user") => tool_results(state, bus, &message),
        Some("result") => result_message(state, bus, &message),
        Some("control_request") => control_request(state, bus, outgoing, &message),
        Some("control_response") => control_response(state, &message),
        Some("control_cancel_request") => cancel_request(state, bus, &message),
        _ => {}
    }
}

fn system_message(state: &Shared, message: &Value) {
    if message.get("subtype").and_then(Value::as_str) != Some("init") {
        return;
    }
    let mut state = lock(state);
    if let Some(session) = message.get("session_id").and_then(Value::as_str) {
        state.session = Some(session.to_owned());
    }
    if let Some(model) = message.get("model").and_then(Value::as_str) {
        state.model = Some(model.to_owned());
    }
}

fn stream_event(state: &Shared, bus: &EventBus, message: &Value) {
    let Some(event) = message.get("event") else {
        return;
    };
    if event.get("type").and_then(Value::as_str) != Some("content_block_delta") {
        return;
    }
    let Some(delta) = event.get("delta") else {
        return;
    };
    let Some(text) = delta.get("text").and_then(Value::as_str) else {
        if let Some(thinking) = delta.get("thinking").and_then(Value::as_str) {
            let message_id = message
                .get("uuid")
                .and_then(Value::as_str)
                .unwrap_or_default();
            lock(state).streamed.insert(message_id.to_owned());
            bus.send(RawAgentEvent::MessageChunk {
                role: MessageRole::Thought,
                message_id: message_id.to_owned(),
                text: thinking.to_owned(),
            });
        }
        return;
    };
    let message_id = message
        .get("uuid")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    lock(state).streamed.insert(message_id.clone());
    bus.send(RawAgentEvent::MessageChunk {
        role: MessageRole::Agent,
        message_id,
        text: text.to_owned(),
    });
}

fn assistant_message(state: &Shared, bus: &EventBus, message: &Value) {
    let uuid = message
        .get("uuid")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let Some(blocks) = message
        .pointer("/message/content")
        .and_then(Value::as_array)
    else {
        return;
    };
    let streamed = lock(state).streamed.contains(&uuid);
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if streamed {
                    continue;
                }
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    if !text.is_empty() {
                        bus.send(RawAgentEvent::MessageUpsert {
                            role: MessageRole::Agent,
                            message_id: uuid.clone(),
                            text: Some(text.to_owned()),
                        });
                    }
                }
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let name = block.get("name").and_then(Value::as_str).unwrap_or("Tool");
                let input = block.get("input").cloned().unwrap_or(Value::Null);
                let tool = claude_sessions::tool_event(&id, name, input.clone());
                lock(state).tools.insert(id.clone(), tool.clone());
                bus.send(RawAgentEvent::ToolUpsert { call: tool });
                if is_subagent_tool(name) {
                    let title = input
                        .get("description")
                        .and_then(Value::as_str)
                        .filter(|title| !title.trim().is_empty())
                        .unwrap_or("Subagent")
                        .to_owned();
                    let description = input
                        .get("prompt")
                        .and_then(Value::as_str)
                        .map(|prompt| prompt.chars().take(240).collect());
                    let event = SubagentEvent {
                        id: id.clone(),
                        parent_id: message
                            .get("parent_tool_use_id")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        title,
                        description,
                        status: SubagentStatus::Running,
                        cwd: None,
                        can_cancel: false,
                        usage: None,
                    };
                    lock(state).subagents.insert(id, event.clone());
                    bus.send(RawAgentEvent::Subagent { event });
                }
            }
            _ => {}
        }
    }
}

fn tool_results(shared: &Shared, bus: &EventBus, message: &Value) {
    let Some(blocks) = message
        .pointer("/message/content")
        .and_then(Value::as_array)
    else {
        return;
    };
    for block in blocks {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            continue;
        }
        let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
            continue;
        };
        let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
        let tool = {
            let mut state = lock(shared);
            state.tools.get_mut(id).map(|tool| {
                tool.output = block.get("content").map(text_of);
                tool.status = if failed {
                    threadbase_shared::ToolStatus::Failed
                } else {
                    threadbase_shared::ToolStatus::Completed
                };
                tool.clone()
            })
        };
        if let Some(tool) = tool {
            bus.send(RawAgentEvent::ToolUpsert { call: tool });
        }
        let subagent = {
            let mut state = lock(shared);
            state.subagents.remove(id).map(|mut event| {
                event.status = if failed {
                    SubagentStatus::Error
                } else {
                    SubagentStatus::Idle
                };
                event.usage = message.get("tool_use_result").and_then(subagent_usage);
                event
            })
        };
        if let Some(event) = subagent {
            bus.send(RawAgentEvent::Subagent { event });
        }
    }
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

fn result_message(state: &Shared, bus: &EventBus, message: &Value) {
    let subtype = message
        .get("subtype")
        .and_then(Value::as_str)
        .unwrap_or("success");
    let mut state = lock(state);
    let cancelled = state.cancelled;
    state.cancelled = false;
    state.running_turn = None;
    let unfinished = std::mem::take(&mut state.subagents);
    let usage = message.get("usage").map(|usage| Usage {
        input_tokens: usage
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        output_tokens: usage
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        total_tokens: usage
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            + usage
                .get("output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        context_used: None,
        context_size: None,
    });
    drop(state);
    for mut event in unfinished.into_values() {
        event.status = if cancelled {
            SubagentStatus::Cancelled
        } else {
            SubagentStatus::Idle
        };
        bus.send(RawAgentEvent::Subagent { event });
    }
    let stop_reason = if cancelled {
        "cancelled"
    } else if subtype == "success" {
        "end_turn"
    } else {
        "error"
    };
    if stop_reason == "error" {
        if let Some(error) = message.get("result").and_then(Value::as_str) {
            bus.send(RawAgentEvent::Notice {
                level: "error".to_owned(),
                message: error.to_owned(),
            });
        }
    }
    bus.send(RawAgentEvent::Foreground {
        state: ForegroundState::Idle {
            stop_reason: Some(stop_reason.to_owned()),
            usage,
        },
    });
}

fn control_request(
    state: &Shared,
    bus: &EventBus,
    outgoing: &mpsc::UnboundedSender<Value>,
    message: &Value,
) {
    let Some(request) = message.get("request") else {
        return;
    };
    let request_id = message
        .get("request_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    match request.get("subtype").and_then(Value::as_str) {
        Some("can_use_tool") => {}
        Some("elicitation") => {
            let _ = outgoing.send(json!({
                "type": "control_response",
                "response": {
                    "subtype": "success",
                    "request_id": request_id,
                    "response": {"action": "cancel"},
                },
            }));
            return;
        }
        _ => {
            let _ = outgoing.send(json!({
                "type": "control_response",
                "response": {
                    "subtype": "error",
                    "request_id": request_id,
                    "error": "threadbase does not handle this request",
                },
            }));
            return;
        }
    }
    let tool_name = request
        .get("tool_name")
        .and_then(Value::as_str)
        .unwrap_or("Tool")
        .to_owned();
    let input = request.get("input").cloned().unwrap_or(Value::Null);
    let tool = claude_sessions::tool_event("pending", &tool_name, input.clone());
    let request_id_internal = format!("claude-{}", ulid::Ulid::new());
    let options = vec![
        PermissionOption {
            id: "allow".to_owned(),
            kind: "allow_once".to_owned(),
            label: "Allow once".to_owned(),
        },
        PermissionOption {
            id: "allow_always".to_owned(),
            kind: "allow_always".to_owned(),
            label: "Always allow this tool".to_owned(),
        },
        PermissionOption {
            id: "deny".to_owned(),
            kind: "reject_once".to_owned(),
            label: "Deny".to_owned(),
        },
    ];
    lock(state).approvals.insert(
        request_id_internal.clone(),
        PendingApproval {
            request_id,
            tool_name,
            input,
            options: options
                .iter()
                .map(|option| (option.kind.clone(), option.id.clone()))
                .collect(),
        },
    );
    bus.send(RawAgentEvent::PermissionRequest {
        request: PermissionRequest {
            id: request_id_internal,
            title: tool.title.clone(),
            description: request
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_owned),
            kind: tool.kind,
            command: tool
                .raw
                .get("command")
                .and_then(Value::as_str)
                .map(str::to_owned),
            cwd: tool
                .raw
                .get("cwd")
                .and_then(Value::as_str)
                .map(str::to_owned),
            paths: tool
                .locations
                .iter()
                .map(|location| location.path.clone())
                .collect(),
            options,
            raw: request.clone(),
        },
    });
}

fn control_response(state: &Shared, message: &Value) {
    let Some(response) = message.get("response") else {
        return;
    };
    let Some(request_id) = response.get("request_id").and_then(Value::as_str) else {
        return;
    };
    let tx = lock(state).controls.remove(request_id);
    if let Some(tx) = tx {
        let _ = tx.send(response.clone());
    }
}

fn cancel_request(state: &Shared, bus: &EventBus, message: &Value) {
    let Some(request_id) = message.get("request_id").and_then(Value::as_str) else {
        return;
    };
    let removed = {
        let mut state = lock(state);
        state
            .approvals
            .iter()
            .find(|(_, pending)| pending.request_id == request_id)
            .map(|(id, _)| id.clone())
            .and_then(|id| state.approvals.remove(&id).map(|_| id))
    };
    if let Some(id) = removed {
        bus.send(RawAgentEvent::PermissionCancelled { request_id: id });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_map_to_cli_flags() {
        assert_eq!(claude_mode(PermissionMode::AutoEdit), "acceptEdits");
        assert!(claude_mode_args(PermissionMode::FullAccess).contains(&"bypassPermissions"));
    }
}
