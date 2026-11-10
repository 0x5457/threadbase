//! Append-only thread event model.

use crate::ids::{ApprovalId, ElicitationId, MessageId, SubagentId, ThreadId, ToolCallId, TurnId};
use crate::model::{
    ApprovalDecision, ApprovalDto, ApprovalScope, ChangeStats, CommentDto, DiffSummary,
    ElicitationAction, ElicitationDto, MessageRole, PlanEntry, ShareLevel, SubagentDto,
    ThreadStatus, ToolKind, ToolLocation, ToolStatus, Usage, UserBrief,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ActorKind {
    User,
    Agent,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Actor {
    pub kind: ActorKind,
    pub id: Option<String>,
    pub name: Option<String>,
}

impl Actor {
    #[must_use]
    pub fn system() -> Self {
        Self {
            kind: ActorKind::System,
            id: None,
            name: Some("Threadbase".to_owned()),
        }
    }

    #[must_use]
    pub fn user(user: &UserBrief) -> Self {
        Self {
            kind: ActorKind::User,
            id: Some(user.id.0.clone()),
            name: Some(user.display_name.clone()),
        }
    }

    #[must_use]
    pub fn agent(name: &str) -> Self {
        Self {
            kind: ActorKind::Agent,
            id: None,
            name: Some(name.to_owned()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Origin {
    Local,
    Relay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EventEnvelope {
    pub seq: u64,
    pub thread_id: ThreadId,
    pub ts: i64,
    pub actor: Actor,
    pub origin: Origin,
    pub subagent_id: Option<SubagentId>,
    pub parent_subagent_id: Option<SubagentId>,
    pub tool_call_id: Option<ToolCallId>,
    pub delivery: Option<crate::model::Delivery>,
    pub payload: ThreadEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MessageDto {
    pub id: MessageId,
    pub thread_id: ThreadId,
    pub turn_id: Option<TurnId>,
    pub role: MessageRole,
    pub author: Option<UserBrief>,
    pub authors: Vec<UserBrief>,
    pub text: String,
    pub model: Option<String>,
    pub streaming: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ToolCallData {
    pub id: ToolCallId,
    pub thread_id: ThreadId,
    pub title: String,
    pub kind: ToolKind,
    pub status: ToolStatus,
    pub input: Option<String>,
    pub output: Option<String>,
    pub locations: Vec<ToolLocation>,
    pub diff: Option<DiffSummary>,
    pub terminal_id: Option<String>,
    pub subagent_id: Option<SubagentId>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentMessageDirection {
    Sent,
    Received,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ThreadEvent {
    TurnStarted {
        turn_id: TurnId,
        message_id: MessageId,
        author: UserBrief,
        prompt: String,
    },
    MessageDelta {
        message_id: MessageId,
        role: MessageRole,
        text: String,
        author: Option<UserBrief>,
    },
    MessageCompleted {
        message: MessageDto,
    },
    ThinkingDelta {
        message_id: MessageId,
        text: String,
    },
    ThinkingCompleted {
        message_id: MessageId,
    },
    ToolCall {
        call: ToolCallData,
    },
    ToolCallUpdate {
        call: ToolCallData,
    },
    PlanUpdated {
        entries: Vec<PlanEntry>,
    },
    PermissionRequested {
        approval: ApprovalDto,
    },
    PermissionResolved {
        approval_id: ApprovalId,
        decision: ApprovalDecision,
        scope: Option<ApprovalScope>,
        decided_by: Option<UserBrief>,
    },
    PermissionCancelled {
        approval_id: ApprovalId,
    },
    ElicitationRequested {
        elicitation: ElicitationDto,
    },
    ElicitationResolved {
        elicitation_id: ElicitationId,
        action: ElicitationAction,
        #[ts(type = "unknown")]
        response: Option<serde_json::Value>,
        responded_by: Option<UserBrief>,
    },
    SubagentUpdated {
        subagent: SubagentDto,
    },
    UsageUpdated {
        usage: Usage,
    },
    TurnEnded {
        turn_id: TurnId,
        stop_reason: String,
    },
    TurnError {
        turn_id: Option<TurnId>,
        message: String,
    },
    ProcessExited {
        code: Option<i32>,
        signal: Option<String>,
        message: String,
    },
    ChangesUpdated {
        stats: ChangeStats,
    },
    CommentAdded {
        comment: CommentDto,
    },
    CommentUpdated {
        comment: CommentDto,
    },
    MembersChanged {
        member: crate::model::ThreadMemberDto,
        action: String,
    },
    QueueUpdated {
        items: Vec<crate::model::QueueItemDto>,
    },
    ThreadUpdated {
        title: Option<String>,
        status: Option<ThreadStatus>,
        pinned: Option<bool>,
        share_level: Option<ShareLevel>,
    },
    AgentMessage {
        direction: AgentMessageDirection,
        from: String,
        to: String,
        text: String,
    },
    ContextCompacted {
        summary: Option<String>,
    },
    SystemNotice {
        level: String,
        message: String,
    },
}

impl ThreadEvent {
    /// Stable wire discriminator, also stored in the `events.kind` column.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::TurnStarted { .. } => "turn_started",
            Self::MessageDelta { .. } => "message_delta",
            Self::MessageCompleted { .. } => "message_completed",
            Self::ThinkingDelta { .. } => "thinking_delta",
            Self::ThinkingCompleted { .. } => "thinking_completed",
            Self::ToolCall { .. } => "tool_call",
            Self::ToolCallUpdate { .. } => "tool_call_update",
            Self::PlanUpdated { .. } => "plan_updated",
            Self::PermissionRequested { .. } => "permission_requested",
            Self::PermissionResolved { .. } => "permission_resolved",
            Self::PermissionCancelled { .. } => "permission_cancelled",
            Self::ElicitationRequested { .. } => "elicitation_requested",
            Self::ElicitationResolved { .. } => "elicitation_resolved",
            Self::SubagentUpdated { .. } => "subagent_updated",
            Self::UsageUpdated { .. } => "usage_updated",
            Self::TurnEnded { .. } => "turn_ended",
            Self::TurnError { .. } => "turn_error",
            Self::ProcessExited { .. } => "process_exited",
            Self::ChangesUpdated { .. } => "changes_updated",
            Self::CommentAdded { .. } => "comment_added",
            Self::CommentUpdated { .. } => "comment_updated",
            Self::MembersChanged { .. } => "members_changed",
            Self::QueueUpdated { .. } => "queue_updated",
            Self::ThreadUpdated { .. } => "thread_updated",
            Self::AgentMessage { .. } => "agent_message",
            Self::ContextCompacted { .. } => "context_compacted",
            Self::SystemNotice { .. } => "system_notice",
        }
    }
}
