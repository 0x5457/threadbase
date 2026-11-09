//! Agent harness abstraction and ACP adapter profiles.
//!
//! The server never talks to an SDK type directly: ACP updates are normalized
//! into [`RawAgentEvent`] and persisted by the thread event log.

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

pub use profile::{all_profiles, find_profile, AgentProfile};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCtx {
    pub thread_id: String,
    pub cwd: PathBuf,
    pub agent: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    pub cwd: PathBuf,
    pub mcp_servers: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub steering: bool,
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
    pub raw: Value,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RawAgentEvent {
    AgentMessageChunk {
        text: String,
    },
    AgentThoughtChunk {
        text: String,
    },
    ToolCall {
        call: ToolCallEvent,
    },
    ToolCallUpdate {
        call: ToolCallEvent,
    },
    Plan {
        entries: Vec<PlanEntry>,
    },
    PermissionRequest {
        request: PermissionRequest,
    },
    ElicitationRequest {
        request: ElicitationRequest,
    },
    Subagent {
        event: SubagentEvent,
    },
    Usage {
        usage: Usage,
    },
    PromptDone {
        stop_reason: String,
    },
    Error {
        message: String,
    },
    ProcessExited {
        code: Option<i32>,
        signal: Option<String>,
    },
    Notice {
        message: String,
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

pub type HarnessResult<T> = Result<T, HarnessError>;

#[async_trait]
pub trait Harness: Send + Sync {
    fn id(&self) -> &'static str;
    async fn start(&mut self, ctx: LaunchCtx) -> HarnessResult<Capabilities>;
    async fn new_session(&mut self, req: NewSession) -> HarnessResult<SessionId>;
    async fn load_session(&mut self, id: &SessionId) -> HarnessResult<()>;
    async fn prompt(&mut self, parts: Vec<String>) -> HarnessResult<TurnHandle>;
    async fn cancel(&mut self, turn: &TurnHandle) -> HarnessResult<()>;
    async fn decide(&mut self, request_id: &str, decision: ApprovalDecision) -> HarnessResult<()>;
    async fn respond_elicitation(
        &mut self,
        request_id: &str,
        action: ElicitationAction,
        values: Option<Value>,
    ) -> HarnessResult<()>;
    async fn cancel_subagent(&mut self, subagent_id: &str) -> HarnessResult<()>;
    async fn shutdown(&mut self) -> HarnessResult<()>;
    fn events(&self) -> broadcast::Receiver<RawAgentEvent>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_a_unique_id() {
        let mut ids = std::collections::HashSet::new();
        for profile in all_profiles() {
            assert!(ids.insert(profile.id));
        }
    }

    #[test]
    fn profiles_cover_documented_agents() {
        for id in ["codex", "claude", "opencode", "pi"] {
            assert!(find_profile(id).is_some(), "missing profile {id}");
        }
    }
}
