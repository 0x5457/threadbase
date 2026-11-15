use super::CommentAction;
use threadbase_shared::{
    ApprovalDecision, ApprovalStatus, CommentStatus, Delivery, ElicitationStatus, QueueStatus,
    SubagentStatus, ThreadStatus,
};

// ---------------------------------------------------------------------------
// DB string mappings (single place for every status enum)
// ---------------------------------------------------------------------------

pub fn thread_status_str(status: ThreadStatus) -> &'static str {
    status.as_str()
}

pub fn parse_thread_status(value: &str) -> Option<ThreadStatus> {
    value.parse().ok()
}

pub fn setup_status_str(status: threadbase_shared::SetupStatus) -> &'static str {
    status.as_str()
}

pub fn parse_setup_status(value: &str) -> Option<threadbase_shared::SetupStatus> {
    value.parse().ok()
}

pub fn subagent_status_str(status: SubagentStatus) -> &'static str {
    status.as_str()
}

pub fn parse_subagent_status(value: &str) -> Option<SubagentStatus> {
    value.parse().ok()
}

pub fn queue_status_str(status: QueueStatus) -> &'static str {
    status.as_str()
}

pub fn parse_queue_status(value: &str) -> Option<QueueStatus> {
    value.parse().ok()
}

pub fn approval_status_str(status: ApprovalStatus) -> &'static str {
    status.as_str()
}

pub fn parse_approval_status(value: &str) -> Option<ApprovalStatus> {
    value.parse().ok()
}

pub fn approval_decision_str(decision: ApprovalDecision) -> &'static str {
    decision.as_str()
}

pub fn elicitation_status_str(status: ElicitationStatus) -> &'static str {
    status.as_str()
}

pub fn parse_elicitation_status(value: &str) -> Option<ElicitationStatus> {
    value.parse().ok()
}

pub fn comment_status_str(status: CommentStatus) -> &'static str {
    status.as_str()
}

pub fn parse_comment_status(value: &str) -> Option<CommentStatus> {
    value.parse().ok()
}

/// Map a target status to the action that produces it (None when unreachable).
pub fn comment_action_for(target: CommentStatus) -> Option<CommentAction> {
    match target {
        CommentStatus::Delivered => Some(CommentAction::Deliver),
        CommentStatus::Resolved => Some(CommentAction::Resolve),
        _ => None,
    }
}

pub fn delivery_str(delivery: Delivery) -> &'static str {
    delivery.as_str()
}

pub fn parse_delivery(value: &str) -> Option<Delivery> {
    value.parse().ok()
}

// ---------------------------------------------------------------------------
// Read rules (thread read / inbox / seen)
// ---------------------------------------------------------------------------

pub struct ReadRules;

impl ReadRules {
    /// Inbox items are unread until `read_at` is set.
    pub fn inbox_unread(read_at: Option<i64>) -> bool {
        read_at.is_none()
    }

    /// A file is seen while its content hash matches the stored seen hash.
    pub fn seen(seen_hash: Option<&str>, content_hash: &str) -> bool {
        seen_hash == Some(content_hash)
    }
}
