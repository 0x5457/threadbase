//! Production ACP harness.
//!
//! One [`AcpHarness`] owns one adapter subprocess and one ACP v1 connection.
//! Protocol types stay in this module: everything the rest of the server sees
//! is a [`RawAgentEvent`].

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::v1::schema::{
    CancelNotification, ClientCapabilities, ContentBlock, CreateElicitationRequest,
    CreateElicitationResponse, ElicitationAcceptAction, ElicitationAction as AcpElicitationAction,
    ElicitationCapabilities, ElicitationContentValue, ElicitationFormCapabilities, ElicitationMode,
    ElicitationScope, FileSystemCapabilities, IdleStopReason, ImageContent, InitializeRequest,
    LoadSessionRequest, McpServer, NewSessionRequest, PermissionOptionId, PermissionOptionKind,
    Plan as AcpPlan, PlanEntry as AcpPlanEntry, PlanEntryPriority as AcpPlanPriority,
    PlanEntryStatus as AcpPlanStatus, PromptRequest, ReadTextFileRequest, ReadTextFileResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigKind, SessionConfigOptionCategory,
    SessionConfigSelectOptions, SessionId as AcpSessionId, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, StateUpdate, StopReason, SubagentCapabilities, SubagentUpdate,
    TextContent, ToolCall, ToolCallContent, ToolCallStatus, ToolCallUpdate,
    ToolKind as AcpToolKind, Usage as AcpUsage, UsageUpdate, WriteTextFileRequest,
    WriteTextFileResponse,
};
use agent_client_protocol::v1::{ConnectionTo, Responder};
use agent_client_protocol::{Agent, Client, Lines};
use futures_util::{Sink, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use similar::{ChangeTag, TextDiff};
use threadbase_shared::{
    ApprovalDecision, DiffSummary, ElicitationAction, PlanEntry, PlanPriority, PlanStatus,
    SubagentStatus, ToolKind, ToolLocation, ToolStatus, Usage,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, oneshot, watch};

use crate::profile::{self, AgentProfile, ResolvedProfile};
use crate::{
    Capabilities, Harness, HarnessError, HarnessResult, LaunchCtx, NewSession, PermissionOption,
    PermissionRequest, PromptPart, RawAgentEvent, SessionId, SteeringOutcome, SubagentEvent,
    ToolCallEvent, TurnHandle,
};

const EVENT_CHANNEL_CAPACITY: usize = 1024;
const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;
const MAX_WRITE_BYTES: usize = 4 * 1024 * 1024;

type OutgoingLines = Pin<Box<dyn Sink<String, Error = io::Error> + Send>>;
type IncomingLines = Pin<Box<dyn Stream<Item = io::Result<String>> + Send>>;

/// Identity reported by the adapter during `initialize`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentIdentity {
    pub name: String,
    pub version: String,
}

/// ACP v1 harness driving one adapter subprocess.
pub struct AcpHarness {
    profile: AgentProfile,
    bus: EventBus,
    state: Arc<Mutex<HarnessState>>,
    connection: Option<ConnectionTo<Agent>>,
    session: Option<SessionId>,
    cwd: Option<PathBuf>,
    model: Option<String>,
    effort: Option<String>,
    capabilities: Capabilities,
    agent_info: Option<AgentIdentity>,
    shutdown: Option<watch::Sender<bool>>,
    driver: Option<tokio::task::JoinHandle<()>>,
    stopping: Arc<AtomicBool>,
}

impl AcpHarness {
    #[must_use]
    pub fn new(profile: AgentProfile) -> Self {
        Self {
            profile,
            bus: EventBus::new(),
            state: Arc::new(Mutex::new(HarnessState::default())),
            connection: None,
            session: None,
            cwd: None,
            model: None,
            effort: None,
            capabilities: Capabilities::default(),
            agent_info: None,
            shutdown: None,
            driver: None,
            stopping: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Capabilities observed while starting the harness.
    #[must_use]
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// Adapter identity reported during `initialize`.
    #[must_use]
    pub fn agent_info(&self) -> Option<&AgentIdentity> {
        self.agent_info.as_ref()
    }

    fn stop_signal(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Some(stop) = self.shutdown.take() {
            let _ = stop.send(true);
        }
    }

    fn connection(&self) -> HarnessResult<&ConnectionTo<Agent>> {
        self.connection.as_ref().ok_or(HarnessError::NotRunning)
    }

    fn active_session(&self) -> HarnessResult<&SessionId> {
        self.session.as_ref().ok_or_else(|| {
            HarnessError::SessionNotFound("no ACP session has been created".to_owned())
        })
    }

    fn send_cancel(&self, session: &str) -> HarnessResult<()> {
        self.connection()?
            .send_notification(CancelNotification::new(AcpSessionId::new(session)))
            .map_err(|error| HarnessError::Protocol(error.to_string()))
    }
}

#[async_trait::async_trait]
impl Harness for AcpHarness {
    fn id(&self) -> &'static str {
        self.profile.id
    }

    async fn start(&mut self, ctx: LaunchCtx) -> HarnessResult<Capabilities> {
        if self.connection.is_some() {
            return Err(HarnessError::Protocol(
                "ACP harness is already running".to_owned(),
            ));
        }
        self.stopping.store(false, Ordering::SeqCst);
        self.model.clone_from(&ctx.model);
        self.effort.clone_from(&ctx.effort);
        // The adapter may only touch files below the launch directory; resolve
        // it once here so every fs request is checked against the same root.
        let fs_root = ctx.cwd.canonicalize().unwrap_or_else(|_| ctx.cwd.clone());

        let resolved = profile::resolve(&self.profile);
        if resolved.via_npx {
            tracing::info!(
                program = %resolved.program,
                args = ?resolved.args,
                "launching ACP adapter through npx"
            );
        }
        let mut child = spawn_adapter(&resolved, &ctx)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| HarnessError::Process("adapter stdin is unavailable".to_owned()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| HarnessError::Process("adapter stdout is unavailable".to_owned()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| HarnessError::Process("adapter stderr is unavailable".to_owned()))?;

        spawn_stderr_reader(stderr, self.bus.clone());

        let incoming: IncomingLines = Box::pin(incoming_lines(stdout, self.bus.clone()));
        let outgoing: OutgoingLines = Box::pin(futures_util::sink::unfold(
            stdin,
            |mut stdin, line: String| async move {
                stdin.write_all(line.as_bytes()).await?;
                stdin.write_all(b"\n").await?;
                stdin.flush().await?;
                Ok::<_, io::Error>(stdin)
            },
        ));

        let (connection_tx, connection_rx) = oneshot::channel();
        let (stop_tx, stop_rx) = watch::channel(false);
        let stop_for_driver = stop_rx.clone();
        let stopping = self.stopping.clone();
        let driver_bus = self.bus.clone();
        let update_state = self.state.clone();
        let update_bus = self.bus.clone();
        let permission_state = self.state.clone();
        let permission_bus = self.bus.clone();
        let elicitation_state = self.state.clone();
        let elicitation_bus = self.bus.clone();
        let read_root = fs_root.clone();
        let write_root = fs_root;

        let driver = tokio::spawn(async move {
            let result = Client
                .v1()
                .name("threadbase")
                .on_receive_notification(
                    async move |notification: SessionNotification, _connection| {
                        handle_session_notification(&update_state, &update_bus, notification);
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .on_receive_request(
                    async move |request: RequestPermissionRequest, responder, _connection| {
                        let (request_id, previous) = {
                            let mut state = lock_state(&permission_state);
                            state.request_seq += 1;
                            let request_id = format!("perm-{}", state.request_seq);
                            let previous = state
                                .tools
                                .get(&request.tool_call.tool_call_id.0.to_string())
                                .cloned();
                            state.permissions.insert(
                                request_id.clone(),
                                PendingPermission {
                                    responder,
                                    options: request
                                        .options
                                        .iter()
                                        .map(|option| (option.kind, option.option_id.clone()))
                                        .collect(),
                                },
                            );
                            (request_id, previous)
                        };
                        permission_bus.send(RawAgentEvent::PermissionRequest {
                            request: build_permission_request(
                                request_id,
                                &request,
                                previous.as_ref(),
                            ),
                        });
                        Ok(())
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: CreateElicitationRequest, responder, _connection| {
                        let request_id = {
                            let mut state = lock_state(&elicitation_state);
                            state.request_seq += 1;
                            let request_id = format!("elicit-{}", state.request_seq);
                            state
                                .elicitations
                                .insert(request_id.clone(), PendingElicitation { responder });
                            request_id
                        };
                        elicitation_bus.send(RawAgentEvent::ElicitationRequest {
                            request: build_elicitation_request(request_id, &request),
                        });
                        Ok(())
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: ReadTextFileRequest, responder, _connection| {
                        let path = match confine_path(&read_root, Path::new(&request.path)) {
                            Ok(path) => path,
                            Err(message) => {
                                return responder.respond_with_error(
                                    agent_client_protocol::Error::internal_error().data(message),
                                );
                            }
                        };
                        let metadata = match tokio::fs::metadata(&path).await {
                            Ok(metadata) => metadata,
                            Err(error) => {
                                return responder.respond_with_error(
                                    agent_client_protocol::Error::internal_error().data(format!(
                                        "failed to read {}: {error}",
                                        request.path.display()
                                    )),
                                );
                            }
                        };
                        if metadata.len() > MAX_READ_BYTES {
                            return responder.respond_with_error(
                                agent_client_protocol::Error::internal_error().data(format!(
                                    "file too large to read: {}",
                                    request.path.display()
                                )),
                            );
                        }
                        match tokio::fs::read_to_string(&path).await {
                            Ok(content) => responder.respond(ReadTextFileResponse::new(
                                read_window(&content, request.line, request.limit),
                            )),
                            Err(error) => responder.respond_with_error(
                                agent_client_protocol::Error::internal_error().data(format!(
                                    "failed to read {}: {error}",
                                    request.path.display()
                                )),
                            ),
                        }
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: WriteTextFileRequest, responder, _connection| {
                        let path = match confine_path(&write_root, Path::new(&request.path)) {
                            Ok(path) => path,
                            Err(message) => {
                                return responder.respond_with_error(
                                    agent_client_protocol::Error::internal_error().data(message),
                                );
                            }
                        };
                        if request.content.len() > MAX_WRITE_BYTES {
                            return responder.respond_with_error(
                                agent_client_protocol::Error::internal_error().data(format!(
                                    "file too large to write: {}",
                                    request.path.display()
                                )),
                            );
                        }
                        match tokio::fs::write(&path, request.content).await {
                            Ok(()) => responder.respond(WriteTextFileResponse::new()),
                            Err(error) => responder.respond_with_error(
                                agent_client_protocol::Error::internal_error().data(format!(
                                    "failed to write {}: {error}",
                                    request.path.display()
                                )),
                            ),
                        }
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .connect_with(Lines::new(outgoing, incoming), async move |connection| {
                    let _ = connection_tx.send(connection.clone());
                    let mut stop_rx = stop_for_driver;
                    tokio::select! {
                        _ = stop_rx.changed() => {}
                        () = connection.incoming_closed() => {}
                    }
                    Ok(())
                })
                .await;

            if let Err(error) = result {
                if !stopping.load(Ordering::SeqCst) {
                    driver_bus.send(RawAgentEvent::Error {
                        message: format!("ACP connection ended: {error}"),
                    });
                }
            }
        });

        let connection = match connection_rx.await {
            Ok(connection) => connection,
            Err(_) => {
                self.stop_signal();
                driver.abort();
                return Err(HarnessError::Process(
                    "ACP connection closed before it was established".to_owned(),
                ));
            }
        };

        let client_capabilities = client_capabilities(ctx.subagents);
        let initialize = InitializeRequest::new(ProtocolVersion::V1)
            .client_capabilities(client_capabilities)
            .client_info(agent_client_protocol::v1::schema::Implementation::new(
                "threadbase",
                env!("CARGO_PKG_VERSION"),
            ));
        let response = connection
            .send_request(initialize)
            .block_task()
            .await
            .map_err(|error| HarnessError::Protocol(error.to_string()))?;
        if response.protocol_version != ProtocolVersion::V1 {
            self.stop_signal();
            driver.abort();
            return Err(HarnessError::Protocol(format!(
                "adapter selected unsupported protocol version {}",
                response.protocol_version.as_u16()
            )));
        }

        let steering = response
            .meta
            .as_ref()
            .and_then(|meta| meta.get("steering"))
            .and_then(|steering| steering.get("supported"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.agent_info = response.agent_info.map(|info| AgentIdentity {
            name: info.name,
            version: info.version,
        });
        self.capabilities = Capabilities {
            steering,
            images: response.agent_capabilities.prompt_capabilities.image,
            subagents: ctx.subagents,
            elicitation_form: true,
            elicitation_url: false,
            terminal: false,
        };
        self.cwd = Some(ctx.cwd);
        self.connection = Some(connection);
        self.shutdown = Some(stop_tx);
        self.driver = Some(driver);
        self.stopping.store(false, Ordering::SeqCst);

        spawn_process_monitor(child, stop_rx, self.bus.clone());

        Ok(self.capabilities)
    }

    async fn new_session(&mut self, req: NewSession) -> HarnessResult<SessionId> {
        let connection = self.connection()?.clone();
        let mcp_servers = parse_mcp_servers(&req.mcp_servers)?;
        lock_state(&self.state).replaying = false;
        let response = connection
            .send_request(NewSessionRequest::new(req.cwd).mcp_servers(mcp_servers))
            .block_task()
            .await
            .map_err(|error| HarnessError::Protocol(error.to_string()))?;
        let session = SessionId(response.session_id.0.to_string());
        {
            let mut state = lock_state(&self.state);
            state.config_options = response.config_options.clone().unwrap_or_default();
        }
        reset_session(&mut lock_state(&self.state), &session.0);
        self.session = Some(session.clone());
        if let (Some(model), Some(option)) = (
            self.model.as_deref(),
            response.config_options.as_ref().and_then(|options| {
                options
                    .iter()
                    .find(|option| {
                        matches!(option.category, Some(SessionConfigOptionCategory::Model))
                    })
                    .map(|option| option.id.clone())
            }),
        ) {
            connection
                .send_request(SetSessionConfigOptionRequest::new(
                    AcpSessionId::new(session.0.as_str()),
                    option,
                    model,
                ))
                .block_task()
                .await
                .map_err(|error| HarnessError::Protocol(error.to_string()))?;
        } else if self.model.is_some() {
            self.bus.send(RawAgentEvent::Notice {
                message: "agent did not advertise a model configuration option".into(),
            });
        }
        if let (Some(effort), Some(option)) = (
            self.effort.as_deref(),
            response.config_options.as_ref().and_then(|options| {
                options
                    .iter()
                    .find(|option| {
                        matches!(
                            option.category,
                            Some(SessionConfigOptionCategory::ThoughtLevel)
                        )
                    })
                    .map(|option| option.id.clone())
            }),
        ) {
            connection
                .send_request(SetSessionConfigOptionRequest::new(
                    AcpSessionId::new(session.0.as_str()),
                    option,
                    effort,
                ))
                .block_task()
                .await
                .map_err(|error| HarnessError::Protocol(error.to_string()))?;
        } else if self.effort.is_some() {
            self.bus.send(RawAgentEvent::Notice {
                message: "agent did not advertise an effort configuration option".into(),
            });
        }
        Ok(session)
    }

    async fn load_session(&mut self, id: &SessionId) -> HarnessResult<()> {
        let connection = self.connection()?.clone();
        let cwd = self.cwd.clone().ok_or(HarnessError::NotRunning)?;
        // `session/load` replays the stored conversation through session
        // updates. Mark the session as replaying before the request because
        // the adapter may emit those updates before the response is observed;
        // the first prompt re-opens the stream.
        lock_state(&self.state).replaying = true;
        let response = match connection
            .send_request(LoadSessionRequest::new(
                AcpSessionId::new(id.0.as_str()),
                cwd,
            ))
            .block_task()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                lock_state(&self.state).replaying = false;
                return Err(HarnessError::Protocol(error.to_string()));
            }
        };
        reset_session(&mut lock_state(&self.state), &id.0);
        // `session/load` re-announces the session configuration; without this a
        // resumed thread reported "does not advertise models" even though the
        // agent advertised them moments earlier in `session/new`.
        if let Some(options) = response.config_options {
            lock_state(&self.state).config_options = options;
        }
        self.session = Some(id.clone());
        Ok(())
    }

    fn config_options(&self) -> Vec<threadbase_shared::AgentConfigOptionDto> {
        lock_state(&self.state)
            .config_options
            .iter()
            .map(|option| {
                use threadbase_shared::{AgentConfigOptionDto, AgentConfigValueDto};
                let (current_value, values) = match &option.kind {
                    SessionConfigKind::Select(select) => {
                        let values = match &select.options {
                            SessionConfigSelectOptions::Ungrouped(options) => options
                                .iter()
                                .map(|value| AgentConfigValueDto {
                                    value: value.value.0.to_string(),
                                    name: value.name.clone(),
                                })
                                .collect(),
                            SessionConfigSelectOptions::Grouped(groups) => groups
                                .iter()
                                .flat_map(|group| {
                                    group.options.iter().map(|value| AgentConfigValueDto {
                                        value: value.value.0.to_string(),
                                        name: value.name.clone(),
                                    })
                                })
                                .collect(),
                            _ => Vec::new(),
                        };
                        (select.current_value.0.to_string(), values)
                    }
                    SessionConfigKind::Boolean(value) => (
                        value.current_value.to_string(),
                        vec![
                            AgentConfigValueDto {
                                value: "true".into(),
                                name: "On".into(),
                            },
                            AgentConfigValueDto {
                                value: "false".into(),
                                name: "Off".into(),
                            },
                        ],
                    ),
                    _ => (String::new(), Vec::new()),
                };
                let category = match option.category {
                    Some(SessionConfigOptionCategory::Model) => "model",
                    Some(SessionConfigOptionCategory::ThoughtLevel) => "thought_level",
                    _ => "other",
                };
                AgentConfigOptionDto {
                    id: option.id.0.to_string(),
                    name: option.name.clone(),
                    category: category.to_owned(),
                    current_value,
                    options: values,
                }
            })
            .collect()
    }

    async fn set_config(&mut self, model: Option<&str>, effort: Option<&str>) -> HarnessResult<()> {
        let connection = self.connection()?.clone();
        let session = self.active_session()?.clone();
        let advertised = lock_state(&self.state).config_options.clone();
        let set_one = |category: SessionConfigOptionCategory, value: &str| {
            advertised
                .iter()
                .find(|option| matches!(option.category, Some(ref c) if *c == category))
                .map(|option| (option.id.clone(), value.to_owned()))
        };
        let mut requests = Vec::new();
        if let Some(model) = model {
            if let Some((id, value)) = set_one(SessionConfigOptionCategory::Model, model) {
                requests.push((id, value));
            } else {
                self.bus.send(RawAgentEvent::Notice {
                    message: "agent did not advertise a model configuration option".into(),
                });
            }
        }
        if let Some(effort) = effort {
            if let Some((id, value)) = set_one(SessionConfigOptionCategory::ThoughtLevel, effort) {
                requests.push((id, value));
            } else {
                self.bus.send(RawAgentEvent::Notice {
                    message: "agent did not advertise an effort configuration option".into(),
                });
            }
        }
        let mut updated = None;
        for (id, value) in requests {
            let response = connection
                .send_request(SetSessionConfigOptionRequest::new(
                    AcpSessionId::new(session.0.as_str()),
                    id,
                    value.as_str(),
                ))
                .block_task()
                .await
                .map_err(|error| HarnessError::Protocol(error.to_string()))?;
            updated = Some(response.config_options);
        }
        if let Some(options) = updated {
            if !options.is_empty() {
                lock_state(&self.state).config_options = options;
            }
        }
        Ok(())
    }

    async fn prompt(&mut self, parts: Vec<PromptPart>) -> HarnessResult<TurnHandle> {
        let connection = self.connection()?.clone();
        let session = self.active_session()?.clone();
        let turn_id = {
            let mut state = lock_state(&self.state);
            state.turn_seq += 1;
            let turn_id = format!("turn-{}", state.turn_seq);
            state.running_turn = Some(turn_id.clone());
            state.replaying = false;
            turn_id
        };
        let handle = TurnHandle {
            turn_id: turn_id.clone(),
        };
        let request =
            PromptRequest::new(AcpSessionId::new(session.0.as_str()), prompt_blocks(parts));
        let bus = self.bus.clone();
        let state = self.state.clone();
        tokio::spawn(async move {
            let result = connection.send_request(request).block_task().await;
            if let Ok(response) = &result {
                if let Some(usage) = &response.usage {
                    bus.send(RawAgentEvent::Usage {
                        usage: usage_from_acp(usage),
                    });
                }
                bus.send(RawAgentEvent::PromptDone {
                    stop_reason: stop_reason_name(&response.stop_reason),
                });
            }
            if let Err(error) = result {
                bus.send(RawAgentEvent::Error {
                    message: format!("prompt failed: {error}"),
                });
            }
            let mut state = lock_state(&state);
            if state.running_turn.as_deref() == Some(turn_id.as_str()) {
                state.running_turn = None;
            }
        });
        Ok(handle)
    }

    async fn cancel(&mut self, turn: &TurnHandle) -> HarnessResult<()> {
        let session = self.active_session()?.0.clone();
        {
            let mut state = lock_state(&self.state);
            if let Some(running) = state.running_turn.as_deref() {
                if running != turn.turn_id {
                    return Err(HarnessError::Protocol(format!(
                        "turn {} is not running",
                        turn.turn_id
                    )));
                }
            }
            state.running_turn = None;
            for (_, pending) in state.permissions.drain() {
                let _ = pending.responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Cancelled,
                ));
            }
            for (_, pending) in state.elicitations.drain() {
                let _ = pending
                    .responder
                    .respond(CreateElicitationResponse::new(AcpElicitationAction::Cancel));
            }
        }
        self.send_cancel(&session)
    }

    async fn decide(&mut self, request_id: &str, decision: ApprovalDecision) -> HarnessResult<()> {
        let pending = lock_state(&self.state)
            .permissions
            .remove(request_id)
            .ok_or_else(|| HarnessError::RequestNotFound(request_id.to_owned()))?;
        let outcome = option_for_decision(decision, &pending.options)
            .map(|option_id| {
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id))
            })
            .unwrap_or(RequestPermissionOutcome::Cancelled);
        pending
            .responder
            .respond(RequestPermissionResponse::new(outcome))
            .map_err(|error| HarnessError::Protocol(error.to_string()))
    }

    async fn respond_elicitation(
        &mut self,
        request_id: &str,
        action: ElicitationAction,
        values: Option<Value>,
    ) -> HarnessResult<()> {
        let response_action = match action {
            ElicitationAction::Accept => {
                let content = match values {
                    None | Some(Value::Null) => None,
                    Some(Value::Object(fields)) => Some(
                        serde_json::from_value::<BTreeMap<String, ElicitationContentValue>>(
                            Value::Object(fields),
                        )
                        .map_err(|error| {
                            HarnessError::Protocol(format!(
                                "invalid elicitation response content: {error}"
                            ))
                        })?,
                    ),
                    Some(_) => {
                        return Err(HarnessError::Protocol(
                            "elicitation response content must be a JSON object".to_owned(),
                        ));
                    }
                };
                AcpElicitationAction::Accept(ElicitationAcceptAction::new().content(content))
            }
            ElicitationAction::Decline => AcpElicitationAction::Decline,
            ElicitationAction::Cancel => AcpElicitationAction::Cancel,
        };
        let pending = lock_state(&self.state)
            .elicitations
            .remove(request_id)
            .ok_or_else(|| HarnessError::RequestNotFound(request_id.to_owned()))?;
        pending
            .responder
            .respond(CreateElicitationResponse::new(response_action))
            .map_err(|error| HarnessError::Protocol(error.to_string()))
    }

    async fn cancel_subagent(&mut self, subagent_id: &str) -> HarnessResult<()> {
        let can_cancel = lock_state(&self.state)
            .subagents
            .get(subagent_id)
            .map(|subagent| subagent.can_cancel)
            .ok_or_else(|| HarnessError::SessionNotFound(subagent_id.to_owned()))?;
        if !can_cancel {
            return Err(HarnessError::Protocol(format!(
                "subagent {subagent_id} did not declare cancel support"
            )));
        }
        self.send_cancel(subagent_id)
    }

    async fn shutdown(&mut self) -> HarnessResult<()> {
        self.stop_signal();
        if let Some(driver) = self.driver.take() {
            let _ = driver.await;
        }
        self.connection = None;
        self.session = None;
        self.capabilities = Capabilities::default();
        Ok(())
    }

    fn events(&self) -> broadcast::Receiver<RawAgentEvent> {
        self.bus.subscribe()
    }

    async fn steer(&mut self, parts: Vec<PromptPart>) -> HarnessResult<SteeringOutcome> {
        if !self.capabilities.steering {
            return Err(HarnessError::Unavailable(
                "agent did not advertise steering support".to_owned(),
            ));
        }
        if PromptPart::has_image(&parts) && !self.capabilities.images {
            return Err(HarnessError::Unavailable(
                "agent did not advertise image prompts".to_owned(),
            ));
        }
        let connection = self.connection()?.clone();
        let session = self.active_session()?.0.clone();
        let request = SteeringRequest {
            session_id: AcpSessionId::new(session),
            prompt: prompt_blocks(parts),
            meta: Some(steering_meta()),
        };
        let response = connection
            .send_request(request)
            .block_task()
            .await
            .map_err(|error| HarnessError::Protocol(error.to_string()))?;
        let outcome = match response.outcome.as_str() {
            "injected" => SteeringOutcome::Injected,
            "startedNewTurn" => SteeringOutcome::StartedNewTurn,
            "promptRequired" => SteeringOutcome::PromptRequired,
            "failed" => SteeringOutcome::Failed {
                reason: response.reason.clone(),
            },
            other => SteeringOutcome::Failed {
                reason: Some(format!("unknown steering outcome `{other}`")),
            },
        };
        if let SteeringOutcome::Failed { reason } = &outcome {
            self.bus.send(RawAgentEvent::Notice {
                message: match reason {
                    Some(reason) => format!("steering failed: {reason}"),
                    None => "steering failed".to_owned(),
                },
            });
        }
        Ok(outcome)
    }
}

impl Drop for AcpHarness {
    fn drop(&mut self) {
        self.stop_signal();
        if let Some(driver) = self.driver.take() {
            driver.abort();
        }
    }
}

#[derive(Default)]
struct HarnessState {
    session_id: Option<String>,
    /// Options the agent advertised for the current session. They arrive in the
    /// `session/new` or `session/load` response and can be replaced at any time
    /// by a `config_option_update` notification, so they are shared state.
    config_options: Vec<agent_client_protocol::v1::schema::SessionConfigOption>,
    turn_seq: u64,
    request_seq: u64,
    running_turn: Option<String>,
    /// Set while `session/load` replays stored history so replayed output is
    /// not mistaken for live output. Cleared by the first `prompt` or a new
    /// session.
    replaying: bool,
    tools: HashMap<String, ToolCallEvent>,
    subagents: HashMap<String, SubagentEvent>,
    permissions: HashMap<String, PendingPermission>,
    elicitations: HashMap<String, PendingElicitation>,
}

struct PendingPermission {
    responder: Responder<RequestPermissionResponse>,
    options: Vec<(PermissionOptionKind, PermissionOptionId)>,
}

struct PendingElicitation {
    responder: Responder<CreateElicitationResponse>,
}

fn lock_state(state: &Mutex<HarnessState>) -> MutexGuard<'_, HarnessState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn reset_session(state: &mut HarnessState, session: &str) {
    state.session_id = Some(session.to_owned());
    state.tools.clear();
    state.subagents.clear();
    state.running_turn = None;
}

#[derive(Clone)]
struct EventBus {
    sender: Arc<Mutex<Option<broadcast::Sender<RawAgentEvent>>>>,
}

impl EventBus {
    fn new() -> Self {
        let (sender, _receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            sender: Arc::new(Mutex::new(Some(sender))),
        }
    }

    fn send(&self, event: RawAgentEvent) {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(sender) = sender.as_ref() {
            let _ = sender.send(event);
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<RawAgentEvent> {
        let sender = self
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match sender.as_ref() {
            Some(sender) => sender.subscribe(),
            None => {
                let (sender, receiver) = broadcast::channel(1);
                drop(sender);
                receiver
            }
        }
    }

    fn close(&self) {
        self.sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}

fn spawn_adapter(resolved: &ResolvedProfile, ctx: &LaunchCtx) -> HarnessResult<Child> {
    let mut command = Command::new(&resolved.program);
    command
        .args(&resolved.args)
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
    command.spawn().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            HarnessError::Unavailable(format!(
                "agent adapter `{}` was not found in PATH or through npx",
                resolved.program
            ))
        } else {
            HarnessError::Io(error)
        }
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

fn spawn_process_monitor(mut child: Child, mut stop: watch::Receiver<bool>, bus: EventBus) {
    tokio::spawn(async move {
        let status = tokio::select! {
            status = child.wait() => status,
            _ = stop.changed() => {
                let _ = child.start_kill();
                child.wait().await
            }
        };
        match status {
            Ok(status) => bus.send(RawAgentEvent::ProcessExited {
                code: status.code(),
                signal: exit_signal(&status),
            }),
            Err(error) => bus.send(RawAgentEvent::Error {
                message: format!("failed to observe agent process exit: {error}"),
            }),
        }
        bus.close();
    });
}

#[cfg(unix)]
fn exit_signal(status: &std::process::ExitStatus) -> Option<String> {
    use std::os::unix::process::ExitStatusExt;
    status.signal().map(|signal| signal.to_string())
}

#[cfg(not(unix))]
fn exit_signal(_status: &std::process::ExitStatus) -> Option<String> {
    None
}

fn incoming_lines(
    stdout: tokio::process::ChildStdout,
    bus: EventBus,
) -> impl Stream<Item = io::Result<String>> {
    futures_util::stream::unfold(
        (BufReader::new(stdout), bus),
        |(mut reader, bus)| async move {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => None,
                Ok(_) => {
                    let line = line.trim_end_matches(['\r', '\n']).to_owned();
                    if let Err(message) = validate_jsonrpc_line(&line) {
                        bus.send(RawAgentEvent::Error { message });
                    }
                    Some((Ok(line), (reader, bus)))
                }
                Err(error) => Some((Err(error), (reader, bus))),
            }
        },
    )
}

fn validate_jsonrpc_line(line: &str) -> Result<(), String> {
    let value: Value = serde_json::from_str(line)
        .map_err(|error| format!("malformed JSON-RPC message from agent: {error}"))?;
    let Some(object) = value.as_object() else {
        return Err("malformed JSON-RPC message from agent: expected an object".to_owned());
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err("malformed JSON-RPC message from agent: missing jsonrpc 2.0".to_owned());
    }
    let is_request_or_notification = object.contains_key("method");
    let is_response = object.contains_key("result") || object.contains_key("error");
    if !is_request_or_notification && !is_response {
        return Err(
            "malformed JSON-RPC message from agent: neither method nor response payload".to_owned(),
        );
    }
    Ok(())
}

fn client_capabilities(subagents: bool) -> ClientCapabilities {
    let capabilities = ClientCapabilities::new()
        .fs(FileSystemCapabilities::new()
            .read_text_file(true)
            .write_text_file(true))
        .terminal(false)
        .elicitation(ElicitationCapabilities::new().form(ElicitationFormCapabilities::new()));
    if subagents {
        capabilities.subagents(SubagentCapabilities::new())
    } else {
        capabilities
    }
}

fn parse_mcp_servers(values: &[Value]) -> HarnessResult<Vec<McpServer>> {
    values
        .iter()
        .map(|value| {
            serde_json::from_value(value.clone()).map_err(|error| {
                HarnessError::Protocol(format!("invalid MCP server configuration: {error}"))
            })
        })
        .collect()
}

fn handle_session_notification(
    state: &Mutex<HarnessState>,
    bus: &EventBus,
    notification: SessionNotification,
) {
    let replaying = lock_state(state).replaying;
    let origin = notification.session_id.0.to_string();
    match notification.update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            if replaying {
                return;
            }
            if let Some(text) = content_text(&chunk.content) {
                bus.send(RawAgentEvent::AgentMessageChunk { text });
            }
        }
        SessionUpdate::AgentThoughtChunk(chunk) => {
            if replaying {
                return;
            }
            if let Some(text) = content_text(&chunk.content) {
                bus.send(RawAgentEvent::AgentThoughtChunk { text });
            }
        }
        SessionUpdate::ToolCall(call) => {
            if replaying {
                return;
            }
            let event = tool_call_event(&call);
            lock_state(state)
                .tools
                .insert(event.id.clone(), event.clone());
            bus.send(RawAgentEvent::ToolCall { call: event });
        }
        SessionUpdate::ToolCallUpdate(update) => {
            if replaying {
                return;
            }
            let mut state = lock_state(state);
            let id = update.tool_call_id.0.to_string();
            let event = apply_tool_update(state.tools.remove(&id), &update);
            state.tools.insert(id, event.clone());
            drop(state);
            bus.send(RawAgentEvent::ToolCallUpdate { call: event });
        }
        SessionUpdate::Plan(plan) => {
            bus.send(RawAgentEvent::Plan {
                entries: plan_entries(&plan),
            });
        }
        SessionUpdate::UsageUpdate(usage) => {
            bus.send(RawAgentEvent::Usage {
                usage: usage_from_update(&usage),
            });
        }
        SessionUpdate::Notice(notice) => {
            let message = match notice.description {
                Some(description) => format!("{}: {description}", notice.title),
                None => notice.title,
            };
            bus.send(RawAgentEvent::Notice { message });
        }
        SessionUpdate::SubagentUpdate(update) => {
            handle_subagent_update(state, bus, &origin, &update);
        }
        SessionUpdate::ConfigOptionUpdate(update) => {
            // Agents re-announce the full option set when the session is
            // reloaded or a model changes underneath us.
            lock_state(state).config_options = update.config_options;
        }
        SessionUpdate::SessionMessage(message) => {
            let text = message
                .content
                .as_opt_ref()
                .flatten()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(content_text)
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            if text.is_empty() {
                return;
            }
            let sender = message
                .sender_session_id
                .as_opt_ref()
                .flatten()
                .map(|value| value.0.to_string());
            let recipient = message
                .recipient_session_id
                .as_opt_ref()
                .flatten()
                .map(|value| value.0.to_string());
            let direction = if sender.as_deref() == Some(origin.as_str()) {
                "sent"
            } else {
                "received"
            };
            bus.send(RawAgentEvent::AgentMessage {
                direction: direction.to_owned(),
                from: sender.unwrap_or_else(|| origin.clone()),
                to: recipient.unwrap_or_else(|| origin.clone()),
                text,
            });
        }
        _ => {}
    }
}

fn content_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(text) => Some(text.text.clone()),
        _ => None,
    }
}

fn tool_call_event(call: &ToolCall) -> ToolCallEvent {
    ToolCallEvent {
        id: call.tool_call_id.0.to_string(),
        title: call.title.clone(),
        kind: map_tool_kind(call.kind),
        status: map_tool_status(call.status),
        input: call.raw_input.as_ref().map(Value::to_string),
        output: call.raw_output.as_ref().map(Value::to_string),
        locations: map_locations(&call.locations),
        terminal_id: terminal_id(&call.content),
        diff: diff_summary(&call.content),
        raw: to_value(call),
    }
}

fn apply_tool_update(previous: Option<ToolCallEvent>, update: &ToolCallUpdate) -> ToolCallEvent {
    let fields = &update.fields;
    let mut event = previous.unwrap_or_else(|| ToolCallEvent {
        id: update.tool_call_id.0.to_string(),
        title: fields
            .title
            .clone()
            .unwrap_or_else(|| "Tool call".to_owned()),
        kind: fields.kind.map(map_tool_kind).unwrap_or(ToolKind::Other),
        status: fields
            .status
            .map(map_tool_status)
            .unwrap_or(ToolStatus::Pending),
        input: None,
        output: None,
        locations: Vec::new(),
        terminal_id: None,
        diff: None,
        raw: Value::Null,
    });
    if let Some(title) = &fields.title {
        event.title.clone_from(title);
    }
    if let Some(kind) = fields.kind {
        event.kind = map_tool_kind(kind);
    }
    if let Some(status) = fields.status {
        event.status = map_tool_status(status);
    }
    if let Some(input) = &fields.raw_input {
        event.input = Some(input.to_string());
    }
    if let Some(output) = &fields.raw_output {
        event.output = Some(output.to_string());
    }
    // pi-acp streams a terminal-backed command's stdout/stderr as `_meta`
    // deltas instead of `raw_output`; fold them into the tool output the UI
    // renders (equivalent to Zed's embedded terminal view).
    if let Some(meta) = &update.meta {
        apply_terminal_meta(&mut event, meta);
    }
    if let Some(locations) = &fields.locations {
        event.locations = map_locations(locations);
    }
    if let Some(content) = &fields.content {
        event.terminal_id = terminal_id(content);
        if let Some(diff) = diff_summary(content) {
            event.diff = Some(diff);
        }
    }
    event.raw = to_value(update);
    event
}

fn apply_terminal_meta(event: &mut ToolCallEvent, meta: &Map<String, Value>) {
    if let Some(delta) = meta
        .get("terminal_output")
        .and_then(|value| value.get("data"))
        .and_then(Value::as_str)
    {
        let mut output = event.output.take().unwrap_or_default();
        output.push_str(delta);
        event.output = Some(output);
    }
    if let Some(exit_code) = meta
        .get("terminal_exit")
        .and_then(|value| value.get("exit_code"))
        .and_then(Value::as_i64)
    {
        if exit_code != 0 {
            let mut output = event.output.take().unwrap_or_default();
            if !output.ends_with('\n') && !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&format!("exit code: {exit_code}"));
            event.output = Some(output);
        }
    }
}

fn map_tool_kind(kind: AcpToolKind) -> ToolKind {
    match kind {
        AcpToolKind::Read => ToolKind::Read,
        AcpToolKind::Edit => ToolKind::Edit,
        AcpToolKind::Delete => ToolKind::Delete,
        AcpToolKind::Move => ToolKind::Move,
        AcpToolKind::Search => ToolKind::Search,
        AcpToolKind::Execute => ToolKind::Execute,
        AcpToolKind::Think => ToolKind::Think,
        AcpToolKind::Fetch => ToolKind::Fetch,
        _ => ToolKind::Other,
    }
}

fn map_tool_status(status: ToolCallStatus) -> ToolStatus {
    match status {
        ToolCallStatus::Pending => ToolStatus::Pending,
        ToolCallStatus::InProgress => ToolStatus::InProgress,
        ToolCallStatus::Completed => ToolStatus::Completed,
        ToolCallStatus::Failed => ToolStatus::Failed,
        _ => ToolStatus::Pending,
    }
}

fn map_locations(
    locations: &[agent_client_protocol::v1::schema::ToolCallLocation],
) -> Vec<ToolLocation> {
    locations
        .iter()
        .map(|location| ToolLocation {
            path: location.path.to_string_lossy().into_owned(),
            line: location.line,
        })
        .collect()
}

fn diff_summary(content: &[ToolCallContent]) -> Option<DiffSummary> {
    for item in content {
        let ToolCallContent::Diff(diff) = item else {
            continue;
        };
        let old = diff.old_text.as_deref().unwrap_or_default();
        let changes = TextDiff::from_lines(old, &diff.new_text);
        let additions = changes
            .iter_all_changes()
            .filter(|change| change.tag() == ChangeTag::Insert)
            .count();
        let deletions = changes
            .iter_all_changes()
            .filter(|change| change.tag() == ChangeTag::Delete)
            .count();
        let path = diff.path.to_string_lossy().into_owned();
        let mut patch = format!("--- a/{path}\n+++ b/{path}\n");
        patch.push_str(&changes.unified_diff().context_radius(3).to_string());
        return Some(DiffSummary {
            path,
            additions: u32::try_from(additions).unwrap_or(u32::MAX),
            deletions: u32::try_from(deletions).unwrap_or(u32::MAX),
            patch: Some(patch),
        });
    }
    None
}

fn terminal_id(content: &[ToolCallContent]) -> Option<String> {
    content.iter().find_map(|item| match item {
        ToolCallContent::Terminal(terminal) => Some(terminal.terminal_id.0.to_string()),
        _ => None,
    })
}

fn plan_entries(plan: &AcpPlan) -> Vec<PlanEntry> {
    plan.entries
        .iter()
        .map(|entry: &AcpPlanEntry| PlanEntry {
            content: entry.content.clone(),
            priority: map_plan_priority(&entry.priority),
            status: map_plan_status(&entry.status),
        })
        .collect()
}

fn map_plan_priority(priority: &AcpPlanPriority) -> PlanPriority {
    match priority {
        AcpPlanPriority::High => PlanPriority::High,
        AcpPlanPriority::Medium => PlanPriority::Medium,
        AcpPlanPriority::Low => PlanPriority::Low,
        _ => PlanPriority::Medium,
    }
}

fn map_plan_status(status: &AcpPlanStatus) -> PlanStatus {
    match status {
        AcpPlanStatus::Pending => PlanStatus::Pending,
        AcpPlanStatus::InProgress => PlanStatus::InProgress,
        AcpPlanStatus::Completed => PlanStatus::Completed,
        _ => PlanStatus::Pending,
    }
}

fn usage_from_update(update: &UsageUpdate) -> Usage {
    Usage {
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        context_used: Some(update.used),
        context_size: Some(update.size),
    }
}

fn usage_from_acp(usage: &AcpUsage) -> Usage {
    Usage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        context_used: None,
        context_size: None,
    }
}

fn handle_subagent_update(
    state: &Mutex<HarnessState>,
    bus: &EventBus,
    origin: &str,
    update: &SubagentUpdate,
) {
    let child_id = update.session_id.0.to_string();
    let mut state = lock_state(state);
    let parent_id = if child_id == origin {
        state
            .subagents
            .get(&child_id)
            .and_then(|subagent| subagent.parent_id.clone())
    } else if state.session_id.as_deref() == Some(origin) {
        None
    } else if state.subagents.contains_key(origin) {
        Some(origin.to_owned())
    } else {
        None
    };

    let fallback_title = format!("Subagent {child_id}");
    let event = state
        .subagents
        .entry(child_id.clone())
        .or_insert_with(|| SubagentEvent {
            id: child_id.clone(),
            parent_id: parent_id.clone(),
            title: fallback_title.clone(),
            description: None,
            status: SubagentStatus::Running,
            cwd: None,
            can_cancel: false,
            usage: None,
        });

    if event.parent_id.is_none() {
        event.parent_id = parent_id;
    }
    match update.title.as_opt_ref() {
        None => {}
        Some(None) => event.title = fallback_title,
        Some(Some(title)) => event.title.clone_from(title),
    }
    match update.description.as_opt_ref() {
        None => {}
        Some(Some(description)) => event.description = Some(description.clone()),
        Some(None) => event.description = None,
    }
    match update.capabilities.as_opt_ref() {
        None => {}
        Some(Some(capabilities)) => event.can_cancel = capabilities.cancel.is_some(),
        Some(None) => event.can_cancel = false,
    }
    match update.cwd.as_opt_ref() {
        None => {}
        Some(Some(cwd)) => event.cwd = Some(cwd.to_string_lossy().into_owned()),
        Some(None) => event.cwd = None,
    }
    if let Some(Some(subagent_state)) = update.state.as_opt_ref() {
        apply_subagent_state(event, subagent_state);
    }

    let event = event.clone();
    drop(state);
    bus.send(RawAgentEvent::Subagent { event });
}

fn apply_subagent_state(event: &mut SubagentEvent, state: &StateUpdate) {
    match state {
        StateUpdate::Running(_) => event.status = SubagentStatus::Running,
        StateUpdate::RequiresAction(_) => event.status = SubagentStatus::RequiresAction,
        StateUpdate::Idle(idle) => {
            event.status = match &idle.stop_reason {
                Some(IdleStopReason::Cancelled) => SubagentStatus::Cancelled,
                Some(IdleStopReason::Error(reason)) => {
                    event.description = Some(format!("Stopped with error: {reason:?}"));
                    SubagentStatus::Error
                }
                _ => SubagentStatus::Idle,
            };
            if let Some(usage) = &idle.usage {
                event.usage = Some(usage_from_acp(usage));
            }
        }
        StateUpdate::Unknown(_) => event.status = SubagentStatus::Idle,
        _ => {}
    }
}

fn build_permission_request(
    id: String,
    request: &RequestPermissionRequest,
    previous: Option<&ToolCallEvent>,
) -> PermissionRequest {
    let tool_call = &request.tool_call;
    let raw_input: Option<Value> = tool_call.fields.raw_input.clone().or_else(|| {
        previous
            .and_then(|call| call.input.as_deref())
            .and_then(|input| serde_json::from_str(input).ok())
    });
    PermissionRequest {
        id,
        title: tool_call
            .fields
            .title
            .clone()
            .or_else(|| previous.map(|call| call.title.clone()))
            .unwrap_or_else(|| "Permission requested".to_owned()),
        kind: tool_call
            .fields
            .kind
            .map(map_tool_kind)
            .or_else(|| previous.map(|call| call.kind))
            .unwrap_or(ToolKind::Other),
        command: raw_input
            .as_ref()
            .and_then(|input| input.get("command"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        cwd: raw_input
            .as_ref()
            .and_then(|input| input.get("cwd"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        paths: tool_call
            .fields
            .locations
            .as_deref()
            .map(|locations| {
                locations
                    .iter()
                    .map(|location| location.path.to_string_lossy().into_owned())
                    .collect()
            })
            .or_else(|| {
                previous.map(|call| {
                    call.locations
                        .iter()
                        .map(|location| location.path.clone())
                        .collect()
                })
            })
            .unwrap_or_default(),
        options: request
            .options
            .iter()
            .map(|option| PermissionOption {
                id: option.option_id.to_string(),
                kind: permission_kind_name(option.kind).to_owned(),
                label: option.name.clone(),
            })
            .collect(),
        raw: to_value(request),
    }
}

fn permission_kind_name(kind: PermissionOptionKind) -> &'static str {
    match kind {
        PermissionOptionKind::AllowOnce => "allow_once",
        PermissionOptionKind::AllowAlways => "allow_always",
        PermissionOptionKind::RejectOnce => "reject_once",
        PermissionOptionKind::RejectAlways => "reject_always",
        _ => "other",
    }
}

fn option_for_decision(
    decision: ApprovalDecision,
    options: &[(PermissionOptionKind, PermissionOptionId)],
) -> Option<PermissionOptionId> {
    let preferred: &[PermissionOptionKind] = match decision {
        ApprovalDecision::AllowOnce => &[
            PermissionOptionKind::AllowOnce,
            PermissionOptionKind::AllowAlways,
        ],
        ApprovalDecision::AllowSession | ApprovalDecision::AllowThread => &[
            PermissionOptionKind::AllowAlways,
            PermissionOptionKind::AllowOnce,
        ],
        ApprovalDecision::Deny => &[
            PermissionOptionKind::RejectOnce,
            PermissionOptionKind::RejectAlways,
        ],
    };
    preferred.iter().find_map(|kind| {
        options
            .iter()
            .find(|(option_kind, _)| option_kind == kind)
            .map(|(_, option_id)| option_id.clone())
    })
}

fn build_elicitation_request(
    id: String,
    request: &CreateElicitationRequest,
) -> crate::ElicitationRequest {
    let (mode, requested_schema) = match &request.mode {
        ElicitationMode::Form(form) => ("form", to_value(&form.requested_schema)),
        ElicitationMode::Url(url) => ("url", to_value(url)),
        ElicitationMode::Other(other) => (other.mode.as_str(), Value::Null),
        _ => ("unknown", Value::Null),
    };
    let tool_call_id = match request.mode.scope() {
        ElicitationScope::Session(scope) => scope.tool_call_id.as_ref().map(|id| id.0.to_string()),
        _ => None,
    };
    let blocking = request
        .meta
        .as_ref()
        .and_then(|meta| meta.get("blocking"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    crate::ElicitationRequest {
        id,
        message: request.message.clone(),
        requested_schema,
        mode: mode.to_owned(),
        blocking,
        tool_call_id,
        raw: to_value(request),
    }
}

fn steering_meta() -> Map<String, Value> {
    let mut steering = Map::new();
    steering.insert(
        "idleBehavior".to_owned(),
        Value::String("promptRequired".to_owned()),
    );
    let mut meta = Map::new();
    meta.insert("steering".to_owned(), Value::Object(steering));
    meta
}

fn read_window(content: &str, line: Option<u32>, limit: Option<u32>) -> String {
    if line.is_none() && limit.is_none() {
        return content.to_owned();
    }
    let skip = line.unwrap_or(1).saturating_sub(1) as usize;
    let take = limit.map_or(usize::MAX, |limit| limit as usize);
    content
        .lines()
        .skip(skip)
        .take(take)
        .collect::<Vec<_>>()
        .join("\n")
}

fn outside_workspace(candidate: &Path) -> String {
    format!("path outside workspace: {}", candidate.display())
}

/// Resolve an adapter-supplied path against the launch directory and reject
/// anything that escapes it, including through symlinked ancestors.
fn confine_path(root: &Path, candidate: &Path) -> Result<PathBuf, String> {
    if candidate.as_os_str().to_string_lossy().contains('\0') {
        return Err(outside_workspace(candidate));
    }
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        canonical_root.join(candidate)
    };

    // Lexically fold `.` and `..` before touching the filesystem. The root is
    // the join base, so a `..` that would pop above it is an escape.
    let root_components: Vec<Component<'_>> = canonical_root.components().collect();
    let joined_components: Vec<Component<'_>> = joined.components().collect();
    if joined_components.len() < root_components.len()
        || joined_components[..root_components.len()] != root_components[..]
    {
        return Err(outside_workspace(candidate));
    }
    let mut remainder = PathBuf::new();
    for component in &joined_components[root_components.len()..] {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !remainder.pop() {
                    return Err(outside_workspace(candidate));
                }
            }
            Component::Normal(part) => remainder.push(part),
            _ => return Err(outside_workspace(candidate)),
        }
    }
    let lexical = if remainder.as_os_str().is_empty() {
        canonical_root.clone()
    } else {
        canonical_root.join(&remainder)
    };

    // Canonicalize the nearest existing ancestor so a symlink in any existing
    // component cannot redirect the request, while still allowing new files.
    let mut ancestor = lexical.as_path();
    let mut missing: Vec<OsString> = Vec::new();
    let canonical = loop {
        if let Ok(path) = ancestor.canonicalize() {
            break Some((path, missing));
        }
        // A broken symlink is an existing entry whose target cannot be
        // resolved; rejecting it closes the write-through-a-dangling-link case.
        if ancestor.symlink_metadata().is_ok() {
            return Err(outside_workspace(candidate));
        }
        let Some(name) = ancestor.file_name() else {
            break None;
        };
        missing.push(name.to_os_string());
        let Some(parent) = ancestor.parent() else {
            break None;
        };
        ancestor = parent;
    };
    let Some((mut resolved, missing)) = canonical else {
        return Err(outside_workspace(candidate));
    };
    for name in missing.iter().rev() {
        resolved.push(name);
    }
    if !resolved.starts_with(&canonical_root) {
        return Err(outside_workspace(candidate));
    }
    Ok(resolved)
}

/// Map prompt parts to ACP content blocks. Text stays plain; images carry the
/// base64 payload the protocol expects.
fn prompt_blocks(parts: Vec<PromptPart>) -> Vec<ContentBlock> {
    parts
        .into_iter()
        .map(|part| match part {
            PromptPart::Text(text) => ContentBlock::Text(TextContent::new(text)),
            PromptPart::Image { mime_type, data } => {
                ContentBlock::Image(ImageContent::new(data, mime_type))
            }
        })
        .collect()
}

fn stop_reason_name(reason: &StopReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned())
}

fn to_value<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

#[derive(Debug, Clone, Serialize, Deserialize, agent_client_protocol::JsonRpcRequest)]
#[request(method = "_session/steering", response = SteeringResponse)]
struct SteeringRequest {
    #[serde(rename = "sessionId")]
    session_id: AcpSessionId,
    prompt: Vec<ContentBlock>,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    meta: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, agent_client_protocol::JsonRpcResponse)]
struct SteeringResponse {
    outcome: String,
    #[serde(default)]
    reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn workspace() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp directory");
        let root = dir.path().canonicalize().expect("canonical workspace");
        (dir, root)
    }

    #[test]
    fn confine_path_allows_relative_inside_workspace() {
        let (_dir, root) = workspace();
        fs::write(root.join("notes.txt"), "ok").expect("write fixture");
        assert_eq!(
            confine_path(&root, Path::new("notes.txt")).expect("inside path"),
            root.join("notes.txt")
        );
    }

    #[test]
    fn confine_path_allows_absolute_inside_workspace() {
        let (_dir, root) = workspace();
        fs::write(root.join("notes.txt"), "ok").expect("write fixture");
        assert_eq!(
            confine_path(&root, &root.join("notes.txt")).expect("inside path"),
            root.join("notes.txt")
        );
    }

    #[test]
    fn confine_path_rejects_parent_escape() {
        let (_dir, root) = workspace();
        assert!(confine_path(&root, Path::new("../outside.txt")).is_err());
        assert!(confine_path(&root, Path::new("nested/../../outside.txt")).is_err());
        assert!(confine_path(&root, Path::new("nul\0name")).is_err());
    }

    #[test]
    fn confine_path_rejects_absolute_outside_workspace() {
        let (_dir, root) = workspace();
        let outside = tempfile::tempdir().expect("outside directory");
        let path = outside.path().join("secret.txt");
        assert!(confine_path(&root, &path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn confine_path_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let (_dir, root) = workspace();
        let outside = tempfile::tempdir().expect("outside directory");
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, "secret").expect("write fixture");
        symlink(outside.path(), root.join("linked-dir")).expect("directory symlink");
        symlink(&secret, root.join("linked-file")).expect("file symlink");
        symlink(outside.path().join("new-file"), root.join("dangling-link"))
            .expect("dangling symlink");
        assert!(confine_path(&root, Path::new("linked-dir/secret.txt")).is_err());
        assert!(confine_path(&root, Path::new("linked-file")).is_err());
        assert!(confine_path(&root, Path::new("dangling-link")).is_err());
    }

    fn terminal_update(
        id: &agent_client_protocol::v1::schema::ToolCallId,
        text: Option<&str>,
        exit_code: Option<i64>,
        status: ToolCallStatus,
    ) -> ToolCallUpdate {
        let mut meta = Map::new();
        if let Some(text) = text {
            meta.insert(
                "terminal_output".to_owned(),
                serde_json::json!({ "terminal_id": id.0.as_ref(), "data": text }),
            );
        }
        if let Some(exit_code) = exit_code {
            meta.insert(
                "terminal_exit".to_owned(),
                serde_json::json!({
                    "terminal_id": id.0.as_ref(),
                    "exit_code": exit_code,
                    "signal": null
                }),
            );
        }
        ToolCallUpdate::new(
            id.clone(),
            agent_client_protocol::v1::schema::ToolCallUpdateFields::new().status(status),
        )
        .meta(meta)
    }

    #[test]
    fn terminal_meta_deltas_become_the_command_output() {
        let id = agent_client_protocol::v1::schema::ToolCallId::new("bash-1");
        let mut event = None;
        for (text, status) in [
            (Some("hel"), ToolCallStatus::InProgress),
            (Some("lo\n"), ToolCallStatus::InProgress),
        ] {
            event = Some(apply_tool_update(
                event,
                &terminal_update(&id, text, None, status),
            ));
        }
        event = Some(apply_tool_update(
            event,
            &terminal_update(&id, None, Some(2), ToolCallStatus::Failed),
        ));
        assert_eq!(
            event.unwrap().output.as_deref(),
            Some("hello\nexit code: 2")
        );
    }
}
