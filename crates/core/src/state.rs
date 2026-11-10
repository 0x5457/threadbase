//! Explicit lifecycle state machines for Threadbase.
//!
//! Pure transition tables with guards. No IO: callers persist the returned
//! state and emit events / side effects. Machines are idempotent where an
//! action targets the current state.
//!
//! The authoritative persistence stays in SQLite; these tables are the single
//! source of truth for *which* transitions are legal. See `docs/08`.

use crate::delivery::{resolve_delivery, DeliveryContext, SendAction};
use threadbase_shared::{
    ApprovalDecision, ApprovalStatus, CommentStatus, Delivery, DeliveryMode, ElicitationStatus,
    QueueStatus, SubagentStatus, ThreadStatus,
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransitionError {
    #[error("{entity}: illegal action {action} from {from}")]
    Illegal {
        entity: &'static str,
        from: String,
        action: String,
    },
    #[error("{entity}: guard failed: {reason}")]
    Guard {
        entity: &'static str,
        reason: String,
    },
}

fn illegal(entity: &'static str, from: impl std::fmt::Debug, action: &str) -> TransitionError {
    TransitionError::Illegal {
        entity,
        from: format!("{from:?}"),
        action: action.to_owned(),
    }
}

fn guard(entity: &'static str, reason: impl Into<String>) -> TransitionError {
    TransitionError::Guard {
        entity,
        reason: reason.into(),
    }
}

/// Resolve a `requires_action` thread after a pending item is resolved.
fn resolve_wait(turn_active: bool, remaining_pending: u32) -> ThreadStatus {
    if remaining_pending > 0 {
        ThreadStatus::RequiresAction
    } else if turn_active {
        ThreadStatus::Running
    } else {
        ThreadStatus::Idle
    }
}

// ---------------------------------------------------------------------------
// Thread / turn
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadAction {
    Create,
    Restart,
    StartSucceeded,
    StartFailed,
    Prompt,
    ApprovalPending,
    ElicitationPending,
    ApprovalResolved {
        turn_active: bool,
        remaining_pending: u32,
    },
    ElicitationResolved {
        turn_active: bool,
        remaining_pending: u32,
    },
    TurnEnded,
    TurnError,
    ProcessExited,
    AgentFailed,
    Cancel,
    Archive,
    Unarchive,
}

pub struct ThreadMachine;

impl ThreadMachine {
    pub const ENTITY: &'static str = "thread";

    pub fn transition(
        current: ThreadStatus,
        action: &ThreadAction,
    ) -> Result<ThreadStatus, TransitionError> {
        let next = match action {
            ThreadAction::Create => ThreadStatus::Preparing,
            ThreadAction::Restart => match current {
                ThreadStatus::Archived => return Err(illegal(Self::ENTITY, current, "restart")),
                _ => ThreadStatus::Preparing,
            },
            ThreadAction::StartSucceeded => match current {
                ThreadStatus::Preparing => ThreadStatus::Idle,
                _ => return Err(illegal(Self::ENTITY, current, "start_succeeded")),
            },
            ThreadAction::StartFailed => match current {
                ThreadStatus::Preparing => ThreadStatus::Error,
                _ => return Err(illegal(Self::ENTITY, current, "start_failed")),
            },
            ThreadAction::Prompt => match current {
                ThreadStatus::Preparing | ThreadStatus::Archived => {
                    return Err(illegal(Self::ENTITY, current, "prompt"));
                }
                _ => ThreadStatus::Running,
            },
            ThreadAction::ApprovalPending | ThreadAction::ElicitationPending => match current {
                ThreadStatus::Preparing | ThreadStatus::Archived => {
                    return Err(illegal(Self::ENTITY, current, "pending"));
                }
                _ => ThreadStatus::RequiresAction,
            },
            ThreadAction::ApprovalResolved {
                turn_active,
                remaining_pending,
            }
            | ThreadAction::ElicitationResolved {
                turn_active,
                remaining_pending,
            } => match current {
                ThreadStatus::RequiresAction => resolve_wait(*turn_active, *remaining_pending),
                ThreadStatus::Running => ThreadStatus::Running,
                ThreadStatus::Idle => ThreadStatus::Idle,
                _ => return Err(illegal(Self::ENTITY, current, "resolved")),
            },
            ThreadAction::TurnEnded => match current {
                ThreadStatus::Running | ThreadStatus::RequiresAction | ThreadStatus::Idle => {
                    ThreadStatus::Idle
                }
                _ => return Err(illegal(Self::ENTITY, current, "turn_ended")),
            },
            ThreadAction::TurnError => match current {
                ThreadStatus::Preparing | ThreadStatus::Archived => {
                    return Err(illegal(Self::ENTITY, current, "turn_error"));
                }
                _ => ThreadStatus::Error,
            },
            ThreadAction::ProcessExited => match current {
                ThreadStatus::Preparing | ThreadStatus::Archived => {
                    return Err(illegal(Self::ENTITY, current, "process_exited"));
                }
                _ => ThreadStatus::Interrupted,
            },
            ThreadAction::AgentFailed => match current {
                ThreadStatus::Archived => {
                    return Err(illegal(Self::ENTITY, current, "agent_failed"));
                }
                _ => ThreadStatus::Error,
            },
            ThreadAction::Cancel => match current {
                ThreadStatus::Running | ThreadStatus::RequiresAction | ThreadStatus::Idle => {
                    ThreadStatus::Idle
                }
                _ => return Err(illegal(Self::ENTITY, current, "cancel")),
            },
            ThreadAction::Archive => ThreadStatus::Archived,
            ThreadAction::Unarchive => match current {
                ThreadStatus::Archived => ThreadStatus::Idle,
                _ => return Err(illegal(Self::ENTITY, current, "unarchive")),
            },
        };
        Ok(next)
    }

    /// Guard for side effects that must run before a transition is persisted.
    pub fn check_cancel_allowed(current: ThreadStatus) -> Result<(), TransitionError> {
        match current {
            ThreadStatus::Running | ThreadStatus::RequiresAction | ThreadStatus::Idle => Ok(()),
            _ => Err(guard(Self::ENTITY, "cannot cancel from this state")),
        }
    }
}

// ---------------------------------------------------------------------------
// Approval
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalAction {
    Create,
    Allow,
    Deny,
    Cancel,
}

pub struct ApprovalMachine;

impl ApprovalMachine {
    pub const ENTITY: &'static str = "approval";

    pub fn transition(
        current: ApprovalStatus,
        action: &ApprovalAction,
    ) -> Result<ApprovalStatus, TransitionError> {
        let next = match action {
            ApprovalAction::Create => ApprovalStatus::Pending,
            ApprovalAction::Allow => match current {
                ApprovalStatus::Pending => ApprovalStatus::Allowed,
                _ => return Err(illegal(Self::ENTITY, current, "allow")),
            },
            ApprovalAction::Deny => match current {
                ApprovalStatus::Pending => ApprovalStatus::Denied,
                _ => return Err(illegal(Self::ENTITY, current, "deny")),
            },
            ApprovalAction::Cancel => match current {
                ApprovalStatus::Pending => ApprovalStatus::Cancelled,
                _ => return Err(illegal(Self::ENTITY, current, "cancel")),
            },
        };
        Ok(next)
    }
}

// ---------------------------------------------------------------------------
// Elicitation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElicitationAction {
    Create,
    Accept,
    Decline,
    Cancel,
}

pub struct ElicitationMachine;

impl ElicitationMachine {
    pub const ENTITY: &'static str = "elicitation";

    pub fn transition(
        current: ElicitationStatus,
        action: &ElicitationAction,
    ) -> Result<ElicitationStatus, TransitionError> {
        let next = match action {
            ElicitationAction::Create => ElicitationStatus::Pending,
            ElicitationAction::Accept => match current {
                ElicitationStatus::Pending => ElicitationStatus::Accepted,
                _ => return Err(illegal(Self::ENTITY, current, "accept")),
            },
            ElicitationAction::Decline => match current {
                ElicitationStatus::Pending => ElicitationStatus::Declined,
                _ => return Err(illegal(Self::ENTITY, current, "decline")),
            },
            ElicitationAction::Cancel => match current {
                ElicitationStatus::Pending => ElicitationStatus::Cancelled,
                _ => return Err(illegal(Self::ENTITY, current, "cancel")),
            },
        };
        Ok(next)
    }
}

// ---------------------------------------------------------------------------
// Subagent
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentAction {
    Create,
    Run,
    Idle,
    RequireAction,
    Resolve,
    Fail,
    Cancel,
}

pub struct SubagentMachine;

impl SubagentMachine {
    pub const ENTITY: &'static str = "subagent";

    pub fn transition(
        current: SubagentStatus,
        action: &SubagentAction,
    ) -> Result<SubagentStatus, TransitionError> {
        let next = match action {
            SubagentAction::Create => SubagentStatus::Idle,
            SubagentAction::Run => match current {
                SubagentStatus::Idle | SubagentStatus::Running | SubagentStatus::RequiresAction => {
                    SubagentStatus::Running
                }
                _ => return Err(illegal(Self::ENTITY, current, "run")),
            },
            SubagentAction::Idle => match current {
                SubagentStatus::Running | SubagentStatus::RequiresAction => SubagentStatus::Idle,
                SubagentStatus::Idle => SubagentStatus::Idle,
                _ => return Err(illegal(Self::ENTITY, current, "idle")),
            },
            SubagentAction::RequireAction => match current {
                SubagentStatus::Idle | SubagentStatus::Running => SubagentStatus::RequiresAction,
                _ => return Err(illegal(Self::ENTITY, current, "require_action")),
            },
            SubagentAction::Resolve => match current {
                SubagentStatus::RequiresAction => SubagentStatus::Running,
                _ => return Err(illegal(Self::ENTITY, current, "resolve")),
            },
            SubagentAction::Fail => match current {
                SubagentStatus::Idle | SubagentStatus::Running | SubagentStatus::RequiresAction => {
                    SubagentStatus::Error
                }
                _ => return Err(illegal(Self::ENTITY, current, "fail")),
            },
            SubagentAction::Cancel => match current {
                SubagentStatus::Idle | SubagentStatus::Running | SubagentStatus::RequiresAction => {
                    SubagentStatus::Cancelled
                }
                _ => return Err(illegal(Self::ENTITY, current, "cancel")),
            },
        };
        Ok(next)
    }

    pub fn is_terminal(status: SubagentStatus) -> bool {
        matches!(status, SubagentStatus::Error | SubagentStatus::Cancelled)
    }

    pub fn can_cancel(status: SubagentStatus) -> bool {
        matches!(
            status,
            SubagentStatus::Idle | SubagentStatus::Running | SubagentStatus::RequiresAction
        )
    }
}

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueAction {
    Enqueue,
    Dispatch,
    PromptSucceeded,
    PromptFailed,
    Supersede,
    Delete,
    Retry,
}

pub struct QueueMachine;

impl QueueMachine {
    pub const ENTITY: &'static str = "queue";

    pub fn transition(
        current: QueueStatus,
        action: &QueueAction,
    ) -> Result<QueueStatus, TransitionError> {
        let next = match action {
            QueueAction::Enqueue => QueueStatus::Queued,
            QueueAction::Dispatch => match current {
                QueueStatus::Queued => QueueStatus::Sending,
                _ => return Err(illegal(Self::ENTITY, current, "dispatch")),
            },
            QueueAction::PromptSucceeded => match current {
                QueueStatus::Sending => QueueStatus::Sent,
                _ => return Err(illegal(Self::ENTITY, current, "prompt_succeeded")),
            },
            QueueAction::PromptFailed => match current {
                QueueStatus::Sending => QueueStatus::Dropped,
                _ => return Err(illegal(Self::ENTITY, current, "prompt_failed")),
            },
            QueueAction::Supersede => match current {
                QueueStatus::Queued => QueueStatus::Superseded,
                _ => return Err(illegal(Self::ENTITY, current, "supersede")),
            },
            QueueAction::Delete => match current {
                QueueStatus::Queued => QueueStatus::Dropped,
                _ => return Err(illegal(Self::ENTITY, current, "delete")),
            },
            QueueAction::Retry => match current {
                QueueStatus::Queued | QueueStatus::Superseded => QueueStatus::Queued,
                _ => return Err(illegal(Self::ENTITY, current, "retry")),
            },
        };
        Ok(next)
    }

    pub fn is_terminal(status: QueueStatus) -> bool {
        matches!(status, QueueStatus::Sent | QueueStatus::Dropped)
    }
}

// ---------------------------------------------------------------------------
// Agent session
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    NotStarted,
    Preparing,
    Ready,
    Running,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionAction {
    Start,
    StartSucceeded,
    StartFailed,
    Prompt,
    TurnEnded,
    TurnError,
    ProcessExited,
    Shutdown,
    Restart,
}

pub struct AgentSessionMachine;

impl AgentSessionMachine {
    pub const ENTITY: &'static str = "agent_session";

    pub fn transition(
        current: SessionState,
        action: &SessionAction,
    ) -> Result<SessionState, TransitionError> {
        let next = match action {
            SessionAction::Start => match current {
                SessionState::NotStarted | SessionState::Stopped | SessionState::Failed => {
                    SessionState::Preparing
                }
                _ => return Err(illegal(Self::ENTITY, current, "start")),
            },
            SessionAction::StartSucceeded => match current {
                SessionState::Preparing => SessionState::Ready,
                _ => return Err(illegal(Self::ENTITY, current, "start_succeeded")),
            },
            SessionAction::StartFailed => match current {
                SessionState::Preparing => SessionState::Failed,
                _ => return Err(illegal(Self::ENTITY, current, "start_failed")),
            },
            SessionAction::Prompt => match current {
                SessionState::Ready | SessionState::Running => SessionState::Running,
                _ => return Err(illegal(Self::ENTITY, current, "prompt")),
            },
            SessionAction::TurnEnded => match current {
                SessionState::Running => SessionState::Ready,
                SessionState::Ready => SessionState::Ready,
                _ => return Err(illegal(Self::ENTITY, current, "turn_ended")),
            },
            SessionAction::TurnError => match current {
                SessionState::Running | SessionState::Ready => SessionState::Failed,
                SessionState::Failed => SessionState::Failed,
                _ => return Err(illegal(Self::ENTITY, current, "turn_error")),
            },
            SessionAction::ProcessExited => match current {
                SessionState::Running | SessionState::Ready | SessionState::Preparing => {
                    SessionState::Stopped
                }
                SessionState::Stopped => SessionState::Stopped,
                _ => return Err(illegal(Self::ENTITY, current, "process_exited")),
            },
            SessionAction::Shutdown => match current {
                SessionState::NotStarted => SessionState::NotStarted,
                _ => SessionState::NotStarted,
            },
            SessionAction::Restart => match current {
                SessionState::Failed | SessionState::Stopped | SessionState::Ready => {
                    SessionState::Preparing
                }
                _ => return Err(illegal(Self::ENTITY, current, "restart")),
            },
        };
        Ok(next)
    }
}

// ---------------------------------------------------------------------------
// Delivery
// ---------------------------------------------------------------------------

pub struct DeliveryMachine;

impl DeliveryMachine {
    pub fn resolve(mode: DeliveryMode, ctx: DeliveryContext) -> SendAction {
        resolve_delivery(mode, ctx)
    }

    pub fn delivery_for(action: SendAction) -> Delivery {
        match action {
            SendAction::Prompt | SendAction::Inject => Delivery::Injected,
            SendAction::StopAndSend => Delivery::StoppedAndSent,
            SendAction::Queue => Delivery::Queued,
        }
    }
}

// ---------------------------------------------------------------------------
// Comment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentAction {
    CreatePending,
    CreateDelivered,
    Deliver,
    Resolve,
}

pub struct CommentMachine;

impl CommentMachine {
    pub const ENTITY: &'static str = "comment";

    pub fn transition_to(
        current: CommentStatus,
        target: CommentStatus,
    ) -> Result<CommentStatus, TransitionError> {
        if current == target {
            return Ok(current);
        }
        let action = match target {
            CommentStatus::Delivered => CommentAction::Deliver,
            CommentStatus::Resolved => CommentAction::Resolve,
            _ => return Err(illegal(Self::ENTITY, current, "target")),
        };
        Self::transition(current, &action)
    }

    pub fn transition(
        current: CommentStatus,
        action: &CommentAction,
    ) -> Result<CommentStatus, TransitionError> {
        let next = match action {
            CommentAction::CreatePending => CommentStatus::Pending,
            CommentAction::CreateDelivered => CommentStatus::Delivered,
            CommentAction::Deliver => match current {
                CommentStatus::Pending => CommentStatus::Delivered,
                _ => return Err(illegal(Self::ENTITY, current, "deliver")),
            },
            CommentAction::Resolve => match current {
                CommentStatus::Pending | CommentStatus::Delivered => CommentStatus::Resolved,
                _ => return Err(illegal(Self::ENTITY, current, "resolve")),
            },
        };
        Ok(next)
    }
}

// ---------------------------------------------------------------------------
// DB string mappings (single place for every status enum)
// ---------------------------------------------------------------------------

pub fn thread_status_str(status: ThreadStatus) -> &'static str {
    match status {
        ThreadStatus::Preparing => "preparing",
        ThreadStatus::Idle => "idle",
        ThreadStatus::Running => "running",
        ThreadStatus::RequiresAction => "requires_action",
        ThreadStatus::Interrupted => "interrupted",
        ThreadStatus::Error => "error",
        ThreadStatus::Archived => "archived",
    }
}

pub fn parse_thread_status(value: &str) -> Option<ThreadStatus> {
    match value {
        "preparing" => Some(ThreadStatus::Preparing),
        "idle" => Some(ThreadStatus::Idle),
        "running" => Some(ThreadStatus::Running),
        "requires_action" => Some(ThreadStatus::RequiresAction),
        "interrupted" => Some(ThreadStatus::Interrupted),
        "error" => Some(ThreadStatus::Error),
        "archived" => Some(ThreadStatus::Archived),
        _ => None,
    }
}

// Initial state for a freshly bound agent session (diagnostic, non-authoritative).
pub const INITIAL_BINDING_STATUS: &str = "idle";

pub fn subagent_status_str(status: SubagentStatus) -> &'static str {
    match status {
        SubagentStatus::Running => "running",
        SubagentStatus::RequiresAction => "requires_action",
        SubagentStatus::Idle => "idle",
        SubagentStatus::Cancelled => "cancelled",
        SubagentStatus::Error => "error",
    }
}

pub fn parse_subagent_status(value: &str) -> Option<SubagentStatus> {
    match value {
        "running" => Some(SubagentStatus::Running),
        "requires_action" => Some(SubagentStatus::RequiresAction),
        "idle" => Some(SubagentStatus::Idle),
        "cancelled" => Some(SubagentStatus::Cancelled),
        "error" => Some(SubagentStatus::Error),
        _ => None,
    }
}

pub fn queue_status_str(status: QueueStatus) -> &'static str {
    match status {
        QueueStatus::Queued => "queued",
        QueueStatus::Sending => "sending",
        QueueStatus::Sent => "sent",
        QueueStatus::Superseded => "superseded",
        QueueStatus::Dropped => "dropped",
    }
}

pub fn parse_queue_status(value: &str) -> Option<QueueStatus> {
    match value {
        "queued" => Some(QueueStatus::Queued),
        "sending" => Some(QueueStatus::Sending),
        "sent" => Some(QueueStatus::Sent),
        "superseded" => Some(QueueStatus::Superseded),
        "dropped" => Some(QueueStatus::Dropped),
        _ => None,
    }
}

pub fn approval_status_str(status: ApprovalStatus) -> &'static str {
    match status {
        ApprovalStatus::Pending => "pending",
        ApprovalStatus::Allowed => "allowed",
        ApprovalStatus::Denied => "denied",
        ApprovalStatus::Cancelled => "cancelled",
    }
}

pub fn parse_approval_status(value: &str) -> Option<ApprovalStatus> {
    match value {
        "pending" => Some(ApprovalStatus::Pending),
        "allowed" => Some(ApprovalStatus::Allowed),
        "denied" => Some(ApprovalStatus::Denied),
        "cancelled" => Some(ApprovalStatus::Cancelled),
        _ => None,
    }
}

pub fn approval_decision_str(decision: ApprovalDecision) -> &'static str {
    match decision {
        ApprovalDecision::AllowOnce => "allow_once",
        ApprovalDecision::AllowSession => "allow_session",
        ApprovalDecision::AllowThread => "allow_thread",
        ApprovalDecision::Deny => "deny",
    }
}

pub fn elicitation_status_str(status: ElicitationStatus) -> &'static str {
    match status {
        ElicitationStatus::Pending => "pending",
        ElicitationStatus::Accepted => "accepted",
        ElicitationStatus::Declined => "declined",
        ElicitationStatus::Cancelled => "cancelled",
    }
}

pub fn parse_elicitation_status(value: &str) -> Option<ElicitationStatus> {
    match value {
        "pending" => Some(ElicitationStatus::Pending),
        "accepted" => Some(ElicitationStatus::Accepted),
        "declined" => Some(ElicitationStatus::Declined),
        "cancelled" => Some(ElicitationStatus::Cancelled),
        _ => None,
    }
}

pub fn comment_status_str(status: CommentStatus) -> &'static str {
    match status {
        CommentStatus::Pending => "pending",
        CommentStatus::Delivered => "delivered",
        CommentStatus::Resolved => "resolved",
    }
}

pub fn parse_comment_status(value: &str) -> Option<CommentStatus> {
    match value {
        "pending" => Some(CommentStatus::Pending),
        "delivered" => Some(CommentStatus::Delivered),
        "resolved" => Some(CommentStatus::Resolved),
        _ => None,
    }
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
    match delivery {
        Delivery::Injected => "injected",
        Delivery::StoppedAndSent => "stopped_and_sent",
        Delivery::Queued => "queued",
        Delivery::Dropped => "dropped",
    }
}

pub fn parse_delivery(value: &str) -> Option<Delivery> {
    match value {
        "injected" => Some(Delivery::Injected),
        "stopped_and_sent" => Some(Delivery::StoppedAndSent),
        "queued" => Some(Delivery::Queued),
        "dropped" => Some(Delivery::Dropped),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Read rules (thread read / inbox / seen)
// ---------------------------------------------------------------------------

pub struct ReadRules;

impl ReadRules {
    /// A thread is unread while the newest event sequence is past the last read.
    pub fn thread_unread(last_read_seq: u64, max_seq: u64) -> bool {
        max_seq > last_read_seq
    }

    /// Inbox items are unread until `read_at` is set.
    pub fn inbox_unread(read_at: Option<i64>) -> bool {
        read_at.is_none()
    }

    /// A file is seen while its content hash matches the stored seen hash.
    pub fn seen(seen_hash: Option<&str>, content_hash: &str) -> bool {
        seen_hash == Some(content_hash)
    }
}

// ---------------------------------------------------------------------------
// Presence / connection rules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceState {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresenceAction {
    Connect,
    Handshake,
    Disconnect,
}

pub struct PresenceMachine;

impl PresenceMachine {
    pub const ENTITY: &'static str = "presence";

    pub fn transition(
        current: PresenceState,
        action: &PresenceAction,
    ) -> Result<PresenceState, TransitionError> {
        let next = match action {
            PresenceAction::Connect => match current {
                PresenceState::Disconnected => PresenceState::Connecting,
                _ => return Err(illegal(Self::ENTITY, current, "connect")),
            },
            PresenceAction::Handshake => match current {
                PresenceState::Connecting => PresenceState::Connected,
                _ => return Err(illegal(Self::ENTITY, current, "handshake")),
            },
            PresenceAction::Disconnect => PresenceState::Disconnected,
        };
        Ok(next)
    }

    /// Hidden users stay online; hiding only suppresses focus broadcast.
    pub fn is_online(connected: PresenceState, _hidden: bool) -> bool {
        connected == PresenceState::Connected
    }

    pub fn should_broadcast_focus(hidden: bool) -> bool {
        !hidden
    }

    /// A follow target is stale after 30 seconds without a heartbeat.
    pub fn follow_is_stale(updated_at: i64, now: i64) -> bool {
        now.saturating_sub(updated_at) > 30_000
    }
}

// ---------------------------------------------------------------------------
// Draft rules
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftProviderState {
    Connecting,
    Connected,
    Synced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftAction {
    Connect,
    Sync,
    Disconnect,
    Send,
    DeleteBlock,
}

pub struct DraftMachine;

impl DraftMachine {
    pub const ENTITY: &'static str = "draft";

    pub fn transition(
        current: DraftProviderState,
        action: &DraftAction,
    ) -> Result<DraftProviderState, TransitionError> {
        let next = match action {
            DraftAction::Connect => match current {
                DraftProviderState::Connecting => DraftProviderState::Connected,
                _ => return Err(illegal(Self::ENTITY, current, "connect")),
            },
            DraftAction::Sync => match current {
                DraftProviderState::Connected => DraftProviderState::Synced,
                _ => return Err(illegal(Self::ENTITY, current, "sync")),
            },
            DraftAction::Disconnect => DraftProviderState::Connecting,
            // Sending and deleting blocks do not change provider state.
            DraftAction::Send | DraftAction::DeleteBlock => current,
        };
        Ok(next)
    }

    pub fn can_send(state: DraftProviderState) -> bool {
        state == DraftProviderState::Synced
    }

    pub fn is_sent(sent_at: Option<i64>) -> bool {
        sent_at.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- thread -----

    #[test]
    fn thread_create_is_preparing() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Idle, &ThreadAction::Create).unwrap(),
            ThreadStatus::Preparing
        );
    }

    #[test]
    fn thread_prompt_from_recoverable_states() {
        for current in [
            ThreadStatus::Idle,
            ThreadStatus::Running,
            ThreadStatus::RequiresAction,
            ThreadStatus::Error,
            ThreadStatus::Interrupted,
        ] {
            assert_eq!(
                ThreadMachine::transition(current, &ThreadAction::Prompt).unwrap(),
                ThreadStatus::Running
            );
        }
    }

    #[test]
    fn thread_prompt_blocked_when_preparing_or_archived() {
        assert!(ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::Prompt).is_err());
        assert!(ThreadMachine::transition(ThreadStatus::Archived, &ThreadAction::Prompt).is_err());
    }

    #[test]
    fn thread_pending_only_from_active_states() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Running, &ThreadAction::ApprovalPending)
                .unwrap(),
            ThreadStatus::RequiresAction
        );
        assert_eq!(
            ThreadMachine::transition(
                ThreadStatus::RequiresAction,
                &ThreadAction::ElicitationPending
            )
            .unwrap(),
            ThreadStatus::RequiresAction
        );
        assert!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::ApprovalPending)
                .is_err()
        );
        assert!(ThreadMachine::transition(
            ThreadStatus::Archived,
            &ThreadAction::ElicitationPending
        )
        .is_err());
    }

    #[test]
    fn thread_resolved_restores_running_or_idle() {
        assert_eq!(
            ThreadMachine::transition(
                ThreadStatus::RequiresAction,
                &ThreadAction::ApprovalResolved {
                    turn_active: true,
                    remaining_pending: 0
                }
            )
            .unwrap(),
            ThreadStatus::Running
        );
        assert_eq!(
            ThreadMachine::transition(
                ThreadStatus::RequiresAction,
                &ThreadAction::ApprovalResolved {
                    turn_active: false,
                    remaining_pending: 0
                }
            )
            .unwrap(),
            ThreadStatus::Idle
        );
    }

    #[test]
    fn thread_resolved_keeps_waiting_when_pending_remains() {
        assert_eq!(
            ThreadMachine::transition(
                ThreadStatus::RequiresAction,
                &ThreadAction::ElicitationResolved {
                    turn_active: true,
                    remaining_pending: 2
                }
            )
            .unwrap(),
            ThreadStatus::RequiresAction
        );
    }

    #[test]
    fn thread_archived_is_frozen() {
        for action in [
            ThreadAction::Prompt,
            ThreadAction::ApprovalPending,
            ThreadAction::TurnEnded,
            ThreadAction::TurnError,
            ThreadAction::ProcessExited,
            ThreadAction::AgentFailed,
            ThreadAction::Cancel,
            ThreadAction::Restart,
        ] {
            assert!(
                ThreadMachine::transition(ThreadStatus::Archived, &action).is_err(),
                "archived must reject {action:?}"
            );
        }
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Archived, &ThreadAction::Archive).unwrap(),
            ThreadStatus::Archived
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Archived, &ThreadAction::Unarchive).unwrap(),
            ThreadStatus::Idle
        );
    }

    #[test]
    fn thread_cancel_clears_waiting() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::RequiresAction, &ThreadAction::Cancel).unwrap(),
            ThreadStatus::Idle
        );
        assert!(ThreadMachine::check_cancel_allowed(ThreadStatus::Error).is_err());
    }

    #[test]
    fn thread_restart_and_start() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Error, &ThreadAction::Restart).unwrap(),
            ThreadStatus::Preparing
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::StartSucceeded)
                .unwrap(),
            ThreadStatus::Idle
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::StartFailed).unwrap(),
            ThreadStatus::Error
        );
        assert!(
            ThreadMachine::transition(ThreadStatus::Idle, &ThreadAction::StartSucceeded).is_err()
        );
    }

    #[test]
    fn thread_turn_terminal_states() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Running, &ThreadAction::TurnEnded).unwrap(),
            ThreadStatus::Idle
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Running, &ThreadAction::TurnError).unwrap(),
            ThreadStatus::Error
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Running, &ThreadAction::ProcessExited).unwrap(),
            ThreadStatus::Interrupted
        );
        assert!(ThreadMachine::transition(ThreadStatus::Idle, &ThreadAction::TurnError).is_ok());
        assert!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::TurnEnded).is_err()
        );
    }

    #[test]
    fn thread_agent_failure_marks_error() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::AgentFailed).unwrap(),
            ThreadStatus::Error
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Idle, &ThreadAction::AgentFailed).unwrap(),
            ThreadStatus::Error
        );
        assert!(
            ThreadMachine::transition(ThreadStatus::Archived, &ThreadAction::AgentFailed).is_err()
        );
    }

    // ----- approval -----

    #[test]
    fn approval_create_and_decide() {
        assert_eq!(
            ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Create).unwrap(),
            ApprovalStatus::Pending
        );
        assert_eq!(
            ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Allow).unwrap(),
            ApprovalStatus::Allowed
        );
        assert_eq!(
            ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Deny).unwrap(),
            ApprovalStatus::Denied
        );
        assert_eq!(
            ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Cancel).unwrap(),
            ApprovalStatus::Cancelled
        );
    }

    #[test]
    fn approval_decided_is_terminal() {
        for current in [
            ApprovalStatus::Allowed,
            ApprovalStatus::Denied,
            ApprovalStatus::Cancelled,
        ] {
            for action in [
                ApprovalAction::Allow,
                ApprovalAction::Deny,
                ApprovalAction::Cancel,
            ] {
                assert!(
                    ApprovalMachine::transition(current, &action).is_err(),
                    "{current:?} must reject {action:?}"
                );
            }
        }
    }

    // ----- elicitation -----

    #[test]
    fn elicitation_create_and_respond() {
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Create)
                .unwrap(),
            ElicitationStatus::Pending
        );
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Accept)
                .unwrap(),
            ElicitationStatus::Accepted
        );
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Decline)
                .unwrap(),
            ElicitationStatus::Declined
        );
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Cancel)
                .unwrap(),
            ElicitationStatus::Cancelled
        );
    }

    #[test]
    fn elicitation_resolved_is_terminal() {
        for current in [
            ElicitationStatus::Accepted,
            ElicitationStatus::Declined,
            ElicitationStatus::Cancelled,
        ] {
            assert!(ElicitationMachine::transition(current, &ElicitationAction::Accept).is_err());
        }
    }

    // ----- subagent -----

    #[test]
    fn subagent_lifecycle() {
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Idle, &SubagentAction::Create).unwrap(),
            SubagentStatus::Idle
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Idle, &SubagentAction::Run).unwrap(),
            SubagentStatus::Running
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Running, &SubagentAction::Idle).unwrap(),
            SubagentStatus::Idle
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Running, &SubagentAction::RequireAction)
                .unwrap(),
            SubagentStatus::RequiresAction
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::RequiresAction, &SubagentAction::Resolve)
                .unwrap(),
            SubagentStatus::Running
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Running, &SubagentAction::Fail).unwrap(),
            SubagentStatus::Error
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Running, &SubagentAction::Cancel).unwrap(),
            SubagentStatus::Cancelled
        );
    }

    #[test]
    fn subagent_terminal_and_guards() {
        assert!(SubagentMachine::is_terminal(SubagentStatus::Error));
        assert!(SubagentMachine::is_terminal(SubagentStatus::Cancelled));
        assert!(SubagentMachine::transition(SubagentStatus::Error, &SubagentAction::Run).is_err());
        assert!(
            SubagentMachine::transition(SubagentStatus::Cancelled, &SubagentAction::Run).is_err()
        );
        assert!(
            SubagentMachine::transition(SubagentStatus::Idle, &SubagentAction::Resolve).is_err()
        );
        assert!(SubagentMachine::can_cancel(SubagentStatus::Running));
        assert!(!SubagentMachine::can_cancel(SubagentStatus::Error));
    }

    // ----- queue -----

    #[test]
    fn queue_lifecycle() {
        assert_eq!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::Enqueue).unwrap(),
            QueueStatus::Queued
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::Dispatch).unwrap(),
            QueueStatus::Sending
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Sending, &QueueAction::PromptSucceeded).unwrap(),
            QueueStatus::Sent
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Sending, &QueueAction::PromptFailed).unwrap(),
            QueueStatus::Dropped
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::Supersede).unwrap(),
            QueueStatus::Superseded
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::Delete).unwrap(),
            QueueStatus::Dropped
        );
        assert_eq!(
            QueueMachine::transition(QueueStatus::Superseded, &QueueAction::Retry).unwrap(),
            QueueStatus::Queued
        );
    }

    #[test]
    fn queue_terminal_and_guards() {
        assert!(QueueMachine::is_terminal(QueueStatus::Sent));
        assert!(QueueMachine::is_terminal(QueueStatus::Dropped));
        assert!(!QueueMachine::is_terminal(QueueStatus::Superseded));
        assert!(QueueMachine::transition(QueueStatus::Sent, &QueueAction::Dispatch).is_err());
        assert!(QueueMachine::transition(QueueStatus::Superseded, &QueueAction::Delete).is_err());
        assert!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::PromptSucceeded).is_err()
        );
        assert!(QueueMachine::transition(QueueStatus::Sent, &QueueAction::Retry).is_err());
    }

    // ----- agent session -----

    #[test]
    fn session_lifecycle() {
        assert_eq!(
            AgentSessionMachine::transition(SessionState::NotStarted, &SessionAction::Start)
                .unwrap(),
            SessionState::Preparing
        );
        assert_eq!(
            AgentSessionMachine::transition(
                SessionState::Preparing,
                &SessionAction::StartSucceeded
            )
            .unwrap(),
            SessionState::Ready
        );
        assert_eq!(
            AgentSessionMachine::transition(SessionState::Ready, &SessionAction::Prompt).unwrap(),
            SessionState::Running
        );
        assert_eq!(
            AgentSessionMachine::transition(SessionState::Running, &SessionAction::TurnEnded)
                .unwrap(),
            SessionState::Ready
        );
        assert_eq!(
            AgentSessionMachine::transition(SessionState::Running, &SessionAction::TurnError)
                .unwrap(),
            SessionState::Failed
        );
        assert_eq!(
            AgentSessionMachine::transition(SessionState::Running, &SessionAction::ProcessExited)
                .unwrap(),
            SessionState::Stopped
        );
    }

    #[test]
    fn session_restart_and_shutdown() {
        assert_eq!(
            AgentSessionMachine::transition(SessionState::Failed, &SessionAction::Restart).unwrap(),
            SessionState::Preparing
        );
        assert_eq!(
            AgentSessionMachine::transition(SessionState::Stopped, &SessionAction::Restart)
                .unwrap(),
            SessionState::Preparing
        );
        assert_eq!(
            AgentSessionMachine::transition(SessionState::Ready, &SessionAction::Shutdown).unwrap(),
            SessionState::NotStarted
        );
        assert_eq!(
            AgentSessionMachine::transition(SessionState::Running, &SessionAction::Shutdown)
                .unwrap(),
            SessionState::NotStarted
        );
    }

    #[test]
    fn session_guards() {
        assert!(
            AgentSessionMachine::transition(SessionState::Preparing, &SessionAction::Prompt)
                .is_err()
        );
        assert!(
            AgentSessionMachine::transition(SessionState::NotStarted, &SessionAction::Prompt)
                .is_err()
        );
        assert!(
            AgentSessionMachine::transition(SessionState::Ready, &SessionAction::StartFailed)
                .is_err()
        );
        assert!(
            AgentSessionMachine::transition(SessionState::Running, &SessionAction::Restart)
                .is_err()
        );
    }

    // ----- delivery -----

    #[test]
    fn delivery_machine_resolves_modes() {
        let idle = DeliveryContext::idle();
        assert_eq!(
            DeliveryMachine::resolve(DeliveryMode::Auto, idle),
            SendAction::Prompt
        );
        assert_eq!(
            DeliveryMachine::resolve(DeliveryMode::Queue, idle),
            SendAction::Queue
        );
        let running = DeliveryContext {
            turn_running: true,
            steering_supported: true,
        };
        assert_eq!(
            DeliveryMachine::resolve(DeliveryMode::Auto, running),
            SendAction::Inject
        );
        assert_eq!(
            DeliveryMachine::delivery_for(SendAction::Inject),
            Delivery::Injected
        );
        assert_eq!(
            DeliveryMachine::delivery_for(SendAction::StopAndSend),
            Delivery::StoppedAndSent
        );
        assert_eq!(
            DeliveryMachine::delivery_for(SendAction::Queue),
            Delivery::Queued
        );
    }

    // ----- comment -----

    #[test]
    fn comment_lifecycle() {
        assert_eq!(
            CommentMachine::transition(CommentStatus::Pending, &CommentAction::CreatePending)
                .unwrap(),
            CommentStatus::Pending
        );
        assert_eq!(
            CommentMachine::transition(CommentStatus::Pending, &CommentAction::CreateDelivered)
                .unwrap(),
            CommentStatus::Delivered
        );
        assert_eq!(
            CommentMachine::transition(CommentStatus::Pending, &CommentAction::Deliver).unwrap(),
            CommentStatus::Delivered
        );
        assert_eq!(
            CommentMachine::transition(CommentStatus::Delivered, &CommentAction::Resolve).unwrap(),
            CommentStatus::Resolved
        );
    }

    #[test]
    fn comment_guards() {
        assert!(
            CommentMachine::transition(CommentStatus::Resolved, &CommentAction::Resolve).is_err()
        );
        assert!(
            CommentMachine::transition(CommentStatus::Delivered, &CommentAction::Deliver).is_err()
        );
    }

    #[test]
    fn comment_transition_to() {
        assert_eq!(
            CommentMachine::transition_to(CommentStatus::Pending, CommentStatus::Delivered)
                .unwrap(),
            CommentStatus::Delivered
        );
        assert_eq!(
            CommentMachine::transition_to(CommentStatus::Delivered, CommentStatus::Resolved)
                .unwrap(),
            CommentStatus::Resolved
        );
        assert_eq!(
            CommentMachine::transition_to(CommentStatus::Pending, CommentStatus::Pending).unwrap(),
            CommentStatus::Pending
        );
        assert!(
            CommentMachine::transition_to(CommentStatus::Resolved, CommentStatus::Delivered)
                .is_err()
        );
    }

    // ----- read rules -----

    #[test]
    fn session_idempotent_actions() {
        assert_eq!(
            AgentSessionMachine::transition(SessionState::NotStarted, &SessionAction::Shutdown)
                .unwrap(),
            SessionState::NotStarted
        );
        assert_eq!(
            AgentSessionMachine::transition(SessionState::Ready, &SessionAction::TurnEnded)
                .unwrap(),
            SessionState::Ready
        );
        assert_eq!(
            AgentSessionMachine::transition(SessionState::Failed, &SessionAction::TurnError)
                .unwrap(),
            SessionState::Failed
        );
        assert_eq!(
            AgentSessionMachine::transition(SessionState::Stopped, &SessionAction::ProcessExited)
                .unwrap(),
            SessionState::Stopped
        );
    }

    #[test]
    fn subagent_idempotent_actions() {
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Idle, &SubagentAction::Idle).unwrap(),
            SubagentStatus::Idle
        );
        assert_eq!(
            SubagentMachine::transition(SubagentStatus::Running, &SubagentAction::Run).unwrap(),
            SubagentStatus::Running
        );
        assert!(SubagentMachine::transition(
            SubagentStatus::RequiresAction,
            &SubagentAction::RequireAction
        )
        .is_err());
    }

    #[test]
    fn queue_and_comment_idempotent_actions() {
        assert_eq!(
            QueueMachine::transition(QueueStatus::Queued, &QueueAction::Enqueue).unwrap(),
            QueueStatus::Queued
        );
        assert_eq!(
            CommentMachine::transition(CommentStatus::Pending, &CommentAction::CreatePending)
                .unwrap(),
            CommentStatus::Pending
        );
        assert_eq!(
            CommentMachine::transition(CommentStatus::Delivered, &CommentAction::CreateDelivered)
                .unwrap(),
            CommentStatus::Delivered
        );
        assert!(
            CommentMachine::transition(CommentStatus::Resolved, &CommentAction::Resolve).is_err()
        );
    }

    #[test]
    fn thread_idempotent_actions() {
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Preparing, &ThreadAction::Create).unwrap(),
            ThreadStatus::Preparing
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::Archived, &ThreadAction::Archive).unwrap(),
            ThreadStatus::Archived
        );
        assert_eq!(
            ThreadMachine::transition(ThreadStatus::RequiresAction, &ThreadAction::ApprovalPending)
                .unwrap(),
            ThreadStatus::RequiresAction
        );
    }

    #[test]
    fn approval_idempotent_actions() {
        // Create is idempotent: it always yields Pending.
        assert_eq!(
            ApprovalMachine::transition(ApprovalStatus::Pending, &ApprovalAction::Create).unwrap(),
            ApprovalStatus::Pending
        );
        // Decisions are terminal: repeating them is rejected.
        assert!(
            ApprovalMachine::transition(ApprovalStatus::Allowed, &ApprovalAction::Allow).is_err()
        );
    }

    #[test]
    fn elicitation_idempotent_actions() {
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Create)
                .unwrap(),
            ElicitationStatus::Pending
        );
        assert_eq!(
            ElicitationMachine::transition(ElicitationStatus::Pending, &ElicitationAction::Cancel)
                .unwrap(),
            ElicitationStatus::Cancelled
        );
    }

    #[test]
    fn delivery_and_read_idempotent() {
        // Delivery resolution is pure: same input -> same output.
        assert_eq!(
            DeliveryMachine::resolve(DeliveryMode::Auto, DeliveryContext::idle()),
            SendAction::Prompt
        );
        assert_eq!(
            DeliveryMachine::delivery_for(SendAction::Queue),
            Delivery::Queued
        );
        // Read rules are idempotent.
        assert!(!ReadRules::thread_unread(5, 5));
        assert!(!ReadRules::inbox_unread(Some(1)));
        assert!(ReadRules::seen(Some("a"), "a"));
    }

    #[test]
    fn presence_idempotent_actions() {
        assert_eq!(
            PresenceMachine::transition(PresenceState::Disconnected, &PresenceAction::Disconnect)
                .unwrap(),
            PresenceState::Disconnected
        );
        assert!(
            PresenceMachine::transition(PresenceState::Connecting, &PresenceAction::Connect)
                .is_err()
        );
    }

    #[test]
    fn draft_idempotent_actions() {
        assert_eq!(
            DraftMachine::transition(DraftProviderState::Synced, &DraftAction::Send).unwrap(),
            DraftProviderState::Synced
        );
        assert_eq!(
            DraftMachine::transition(DraftProviderState::Synced, &DraftAction::DeleteBlock)
                .unwrap(),
            DraftProviderState::Synced
        );
    }

    #[test]
    fn read_rules() {
        assert!(ReadRules::thread_unread(4, 5));
        assert!(!ReadRules::thread_unread(5, 5));
        assert!(ReadRules::inbox_unread(None));
        assert!(!ReadRules::inbox_unread(Some(1)));
        assert!(ReadRules::seen(Some("abc"), "abc"));
        assert!(!ReadRules::seen(Some("abc"), "def"));
        assert!(!ReadRules::seen(None, "abc"));
    }

    #[test]
    fn presence_rules() {
        assert_eq!(
            PresenceMachine::transition(PresenceState::Disconnected, &PresenceAction::Connect)
                .unwrap(),
            PresenceState::Connecting
        );
        assert_eq!(
            PresenceMachine::transition(PresenceState::Connecting, &PresenceAction::Handshake)
                .unwrap(),
            PresenceState::Connected
        );
        assert_eq!(
            PresenceMachine::transition(PresenceState::Connected, &PresenceAction::Disconnect)
                .unwrap(),
            PresenceState::Disconnected
        );
        assert!(
            PresenceMachine::transition(PresenceState::Connected, &PresenceAction::Handshake)
                .is_err()
        );
        assert!(PresenceMachine::is_online(PresenceState::Connected, true));
        assert!(!PresenceMachine::is_online(
            PresenceState::Disconnected,
            false
        ));
        assert!(PresenceMachine::should_broadcast_focus(false));
        assert!(!PresenceMachine::should_broadcast_focus(true));
        assert!(PresenceMachine::follow_is_stale(100, 30_101));
        assert!(!PresenceMachine::follow_is_stale(100, 30_000));
    }

    #[test]
    fn draft_rules() {
        assert_eq!(
            DraftMachine::transition(DraftProviderState::Connecting, &DraftAction::Connect)
                .unwrap(),
            DraftProviderState::Connected
        );
        assert_eq!(
            DraftMachine::transition(DraftProviderState::Connected, &DraftAction::Sync).unwrap(),
            DraftProviderState::Synced
        );
        assert!(DraftMachine::can_send(DraftProviderState::Synced));
        assert!(!DraftMachine::can_send(DraftProviderState::Connected));
        assert!(DraftMachine::is_sent(Some(1)));
        assert!(!DraftMachine::is_sent(None));
        assert_eq!(
            DraftMachine::transition(DraftProviderState::Synced, &DraftAction::Send).unwrap(),
            DraftProviderState::Synced
        );
    }
}
