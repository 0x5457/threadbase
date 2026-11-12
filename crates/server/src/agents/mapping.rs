//! Conversions between harness events and shared wire types.

use sqlx::SqlitePool;
use threadbase_core::state::SubagentAction;
use threadbase_shared::{
    now_ms, ApprovalDecision, ApprovalOption, ElicitationDto, ElicitationMode, MessageDto,
    MessageId, MessageRole, SubagentDto, SubagentStatus, ThreadId, ToolKind, TurnId, UserBrief,
};

use crate::auth::CurrentUser;
use crate::error::ServerError;

#[must_use]
pub fn user_brief(user: &CurrentUser) -> UserBrief {
    UserBrief::from_user(&user.user)
}

/// Resolve draft-block author ids to display briefs, preserving order and
/// de-duplicating by user id.
pub async fn briefs_by_ids(pool: &SqlitePool, ids: &[String]) -> Vec<UserBrief> {
    let mut seen = std::collections::HashSet::new();
    let mut briefs = Vec::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        let brief = match crate::auth::load_user_by_id(pool, id).await {
            Ok(Some(user)) => UserBrief::from_user(&user),
            _ => UserBrief {
                id: id.clone().into(),
                username: "member".into(),
                display_name: "Member".into(),
                avatar_url: None,
                color: threadbase_shared::user_color(id).to_owned(),
            },
        };
        briefs.push(brief);
    }
    briefs
}

pub fn user_message(
    thread_id: &str,
    author: &UserBrief,
    text: &str,
    turn_id: Option<TurnId>,
    authors: &[UserBrief],
) -> MessageDto {
    MessageDto {
        id: MessageId::new(),
        thread_id: ThreadId::from(thread_id.to_owned()),
        turn_id,
        role: MessageRole::User,
        author: Some(author.clone()),
        authors: authors.to_vec(),
        text: text.to_owned(),
        model: None,
        streaming: false,
        created_at: now_ms(),
    }
}

pub fn tool_data(
    thread_id: &str,
    call: threadbase_agent::ToolCallEvent,
) -> threadbase_shared::ToolCallData {
    threadbase_shared::ToolCallData {
        id: call.id.into(),
        thread_id: ThreadId::from(thread_id.to_owned()),
        title: call.title,
        kind: call.kind,
        status: call.status,
        input: call.input,
        output: call.output,
        locations: call.locations,
        diff: call.diff,
        terminal_id: call.terminal_id,
        subagent_id: None,
        created_at: now_ms(),
        updated_at: now_ms(),
    }
}

#[must_use]
pub fn approval_kind(kind: threadbase_shared::ApprovalKind) -> &'static str {
    match kind {
        threadbase_shared::ApprovalKind::Command => "command",
        threadbase_shared::ApprovalKind::FileRead => "file_read",
        threadbase_shared::ApprovalKind::FileWrite => "file_write",
        threadbase_shared::ApprovalKind::Network => "network",
        threadbase_shared::ApprovalKind::Other => "other",
    }
}

#[must_use]
pub fn tool_kind(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Read => "file_read",
        ToolKind::Edit | ToolKind::Delete | ToolKind::Move => "file_write",
        ToolKind::Execute => "command",
        _ => "other",
    }
}

#[must_use]
pub fn subagent_action(status: SubagentStatus) -> SubagentAction {
    match status {
        SubagentStatus::Running => SubagentAction::Run,
        SubagentStatus::Idle => SubagentAction::Idle,
        SubagentStatus::RequiresAction => SubagentAction::RequireAction,
        SubagentStatus::Error => SubagentAction::Fail,
        SubagentStatus::Cancelled => SubagentAction::Cancel,
    }
}

#[must_use]
pub fn agent_error(error: threadbase_agent::HarnessError) -> ServerError {
    ServerError::internal(error.to_string())
}

#[must_use]
pub fn approval_option(id: &str, kind: &str, label: &str) -> ApprovalOption {
    ApprovalOption {
        id: id.to_owned(),
        kind: match kind {
            "allow_once" => ApprovalDecision::AllowOnce,
            "allow_always" => ApprovalDecision::AllowSession,
            _ => ApprovalDecision::Deny,
        },
        label: label.to_owned(),
    }
}

pub fn elicitation_dto(
    request: threadbase_agent::ElicitationRequest,
    thread_id: &str,
    agent: &str,
) -> ElicitationDto {
    ElicitationDto {
        id: request.id.into(),
        thread_id: ThreadId::from(thread_id.to_owned()),
        subagent_id: None,
        tool_call_id: request.tool_call_id.map(Into::into),
        agent: agent.to_owned(),
        message: request.message,
        mode: if request.mode == "url" {
            ElicitationMode::Url
        } else {
            ElicitationMode::Form
        },
        requested_schema: request.requested_schema,
        blocking: request.blocking,
        status: threadbase_shared::ElicitationStatus::Pending,
        response: None,
        responded_by: None,
        responded_at: None,
        created_at: now_ms(),
    }
}

pub fn subagent_dto(
    event: &threadbase_agent::SubagentEvent,
    thread_id: &str,
    status: SubagentStatus,
) -> SubagentDto {
    let now = now_ms();
    SubagentDto {
        id: event.id.clone().into(),
        thread_id: ThreadId::from(thread_id.to_owned()),
        parent_id: event.parent_id.clone().map(Into::into),
        title: event.title.clone(),
        description: event.description.clone(),
        status,
        cwd: event.cwd.clone(),
        can_cancel: event.can_cancel,
        usage: event.usage.clone(),
        last_seq: 0,
        created_at: now,
        updated_at: now,
    }
}

pub fn message_dto(
    thread_id: &str,
    message_id: MessageId,
    turn_id: Option<TurnId>,
    text: String,
) -> MessageDto {
    MessageDto {
        id: message_id,
        thread_id: ThreadId::from(thread_id.to_owned()),
        turn_id,
        role: MessageRole::Agent,
        author: None,
        authors: Vec::new(),
        text,
        model: None,
        streaming: false,
        created_at: now_ms(),
    }
}
