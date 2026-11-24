//! Agent harness abstraction and ACP adapter profiles.
//!
//! The server never talks to an SDK type directly: ACP updates are normalized
//! into [`RawAgentEvent`] and persisted by the thread event log.

pub mod acp;
mod bus;
pub mod claude;
pub mod claude_sessions;
pub mod codex;
pub mod profile;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use threadbase_shared::{
    ApprovalDecision, ElicitationAction, PlanEntry, SubagentStatus, ToolKind, ToolLocation,
    ToolStatus, Usage,
};
use tokio::sync::broadcast;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_a_unique_id() {
        let mut ids = std::collections::HashSet::new();
        for profile in all_profiles() {
            assert!(ids.insert(profile.id.as_str()));
        }
    }

    #[test]
    fn profiles_cover_documented_agents() {
        for id in ["codex", "claude", "opencode", "pi"] {
            assert!(find_profile(id).is_some(), "missing profile {id}");
        }
    }
}

pub use acp::AcpHarness;
pub use claude::ClaudeHarness;
pub use codex::CodexHarness;
pub use profile::{all_profiles, find_profile, AgentProfile};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionId(pub String);

/// Identity the adapter reported during its handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentIdentity {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCtx {
    pub thread_id: String,
    pub cwd: PathBuf,
    pub agent: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// Advertise ACP subagent sessions when the caller has opted in.
    pub subagents: bool,
    /// The thread's permission mode, applied to the provider's own session mode
    /// when it has an equivalent.
    pub permission_mode: Option<threadbase_shared::PermissionMode>,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    pub cwd: PathBuf,
    pub mcp_servers: Vec<Value>,
}

/// One block of a prompt. Images are gated on [`Capabilities::images`]; the
/// server checks that before it builds a turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptPart {
    Text(String),
    Image { mime_type: String, data: String },
}

impl PromptPart {
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    #[must_use]
    pub fn has_image(parts: &[Self]) -> bool {
        parts.iter().any(|part| matches!(part, Self::Image { .. }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub steering: bool,
    pub images: bool,
    pub subagents: bool,
    pub elicitation_form: bool,
    pub elicitation_url: bool,
    pub terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnHandle {
    pub turn_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub id: String,
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub id: String,
    pub title: String,
    /// Supporting copy for the decision, from the agent's permission request.
    pub description: Option<String>,
    pub kind: ToolKind,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub paths: Vec<String>,
    pub options: Vec<PermissionOption>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationRequest {
    pub id: String,
    pub message: String,
    pub requested_schema: Value,
    pub mode: String,
    pub blocking: bool,
    pub tool_call_id: Option<String>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallEvent {
    pub id: String,
    pub title: String,
    pub kind: ToolKind,
    pub status: ToolStatus,
    pub input: Option<String>,
    pub output: Option<String>,
    pub locations: Vec<ToolLocation>,
    pub terminal_id: Option<String>,
    pub diff: Option<threadbase_shared::DiffSummary>,
    pub raw: Value,
}

/// A native session the agent still knows about, for attaching a threadbase
/// thread to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSession {
    pub id: String,
    pub title: String,
    pub cwd: String,
    /// Unix ms.
    pub updated_at: i64,
}

/// A native conversation in transcript shape. Transports that can read one
/// return it on attach so the thread log starts with the real history.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionHistory {
    pub turns: Vec<SessionTurn>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionTurn {
    /// The agent's own turn id when it has one (Codex UUIDv7). It is the
    /// cursor incremental import uses; timestamps are only a fallback.
    pub id: Option<String>,
    /// The turn's user message; empty for agent-initiated turns.
    pub user: String,
    pub items: Vec<SessionItem>,
    /// When the agent finished the turn, in Unix ms. `None` while it is still
    /// running or when the source does not report it.
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionItem {
    Message { role: MessageRole, text: String },
    Tool(Box<ToolCallEvent>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentEvent {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: SubagentStatus,
    pub cwd: Option<String>,
    pub can_cancel: bool,
    pub usage: Option<Usage>,
}

/// Which conversation stream a message belongs to. The agent owns message
/// identity (`messageId`); threadbase maps it onto its own transcript rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MessageRole {
    Agent,
    Thought,
}

/// Foreground activity reported by the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ForegroundState {
    Running,
    /// Foreground work is blocked on user action. Informational: the thread
    /// state machine already models pending approvals and elicitations.
    RequiresAction,
    /// The agent can no longer report foreground activity.
    Unknown,
    /// Foreground work ended. `stop_reason` is the agent's idle stop reason;
    /// `usage` carries end-of-turn usage when the adapter reported it.
    Idle {
        stop_reason: Option<String>,
        usage: Option<Usage>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RawAgentEvent {
    /// Append one chunk to a message identified by the agent's stable id.
    /// Chunks of different messages interleave freely; the id places them.
    MessageChunk {
        role: MessageRole,
        message_id: String,
        text: String,
    },
    /// Replace a message's text (whole-message upsert). `None` patches
    /// only metadata and changes nothing the transcript shows.
    MessageUpsert {
        role: MessageRole,
        message_id: String,
        text: Option<String>,
    },
    /// Full tool-call snapshot after the harness applied the protocol's
    /// upsert. The first upsert for an id creates the tool call.
    ToolUpsert {
        call: ToolCallEvent,
    },
    /// Foreground state change; `Idle` is the completion signal. A prompt
    /// response only acknowledges insertion.
    Foreground {
        state: ForegroundState,
    },
    Plan {
        entries: Vec<PlanEntry>,
    },
    PermissionRequest {
        request: PermissionRequest,
    },
    /// Another client owns the native session's turn. Only transports with a
    /// shared session (Codex) report it.
    PeerActivity {
        active: bool,
    },
    /// The agent finished the turn with this id. A stable cursor for
    /// incremental import, unlike second-precision timestamps.
    NativeTurn {
        id: String,
    },
    /// The agent cancelled a pending permission request itself. The stored
    /// approval must not stay pending.
    PermissionCancelled {
        request_id: String,
    },
    ElicitationRequest {
        request: ElicitationRequest,
    },
    Subagent {
        event: SubagentEvent,
    },
    AgentMessage {
        direction: String,
        from: String,
        to: String,
        text: String,
    },
    Usage {
        usage: Usage,
    },
    Error {
        message: String,
    },
    ProcessExited {
        code: Option<i32>,
        signal: Option<String>,
    },
    Notice {
        /// `info` or `warning`; rendered as a system notice in the transcript.
        level: String,
        message: String,
    },
    /// Adapter stderr: diagnostics for the agent log, not conversation output.
    AgentLog {
        level: String,
        text: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error("agent is not running")]
    NotRunning,
    #[error("agent adapter is unavailable: {0}")]
    Unavailable(String),
    #[error("agent protocol error: {0}")]
    Protocol(String),
    #[error("agent process error: {0}")]
    Process(String),
    #[error("session not found: {0}")]
    SessionNotFound(String),
    #[error("request not found: {0}")]
    RequestNotFound(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SteeringOutcome {
    /// The input joined the running turn.
    Injected,
    /// The agent started a new turn even though the client asked it not to.
    StartedNewTurn,
    /// No running turn could accept the input; the client keeps it.
    PromptRequired,
    /// The agent could not apply the input.
    Failed { reason: Option<String> },
}

pub type HarnessResult<T> = Result<T, HarnessError>;

#[async_trait]
pub trait Harness: Send + Sync {
    fn id(&self) -> &str;
    async fn start(&mut self, ctx: LaunchCtx) -> HarnessResult<Capabilities>;
    async fn new_session(&mut self, req: NewSession) -> HarnessResult<SessionId>;
    async fn load_session(&mut self, id: &SessionId) -> HarnessResult<()>;
    async fn prompt(&mut self, parts: Vec<PromptPart>) -> HarnessResult<TurnHandle>;
    async fn cancel(&mut self, turn: &TurnHandle) -> HarnessResult<()>;
    async fn decide(
        &mut self,
        request_id: &str,
        decision: ApprovalDecision,
        option_id: Option<&str>,
    ) -> HarnessResult<()>;
    async fn respond_elicitation(
        &mut self,
        request_id: &str,
        action: ElicitationAction,
        values: Option<Value>,
    ) -> HarnessResult<()>;
    async fn cancel_subagent(&mut self, subagent_id: &str) -> HarnessResult<()>;
    fn config_options(&self) -> Vec<threadbase_shared::AgentConfigOptionDto> {
        Vec::new()
    }

    /// Ask the provider to enforce a permission mode itself, so it stops sending
    /// requests Threadbase would only answer the same way.
    ///
    /// `Unavailable` means "no provider equivalent": the caller keeps answering
    /// requests through its own policy, which is not an error.
    async fn set_permission_mode(
        &mut self,
        _mode: threadbase_shared::PermissionMode,
    ) -> HarnessResult<()> {
        Err(HarnessError::Unavailable(
            "this harness has no provider-side permission modes".to_owned(),
        ))
    }

    async fn set_config(
        &mut self,
        _model: Option<&str>,
        _effort: Option<&str>,
    ) -> HarnessResult<()> {
        Err(HarnessError::Unavailable(
            "this harness does not expose model/effort configuration".to_owned(),
        ))
    }
    async fn shutdown(&mut self) -> HarnessResult<()>;
    fn events(&self) -> broadcast::Receiver<RawAgentEvent>;

    /// Native sessions this agent can still open. Transports without a session
    /// store return an empty list.
    async fn list_sessions(
        &mut self,
        _cwd: Option<&std::path::Path>,
    ) -> HarnessResult<Vec<ExternalSession>> {
        Ok(Vec::new())
    }

    /// Read one native conversation so a new thread can start with it.
    async fn read_history(&mut self, _id: &SessionId) -> HarnessResult<SessionHistory> {
        Err(HarnessError::Unavailable(
            "this transport cannot read native sessions".to_owned(),
        ))
    }

    /// Deliver input into the running turn without starting a new one.
    ///
    /// Harnesses that do not implement the steering extension return
    /// [`HarnessError::Unavailable`].
    async fn steer(&mut self, _parts: Vec<PromptPart>) -> HarnessResult<SteeringOutcome> {
        Err(HarnessError::Unavailable(
            "this harness does not support steering".to_owned(),
        ))
    }
}
