use threadbase_shared::ActionKind;
use threadbase_shared::QueueStatus;

use super::{illegal, TransitionError};

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueAction {
    Enqueue,
    Dispatch,
    PromptSucceeded,
    PromptFailed,
    /// Hand a claimed item back to the queue: steering it failed, so it waits
    /// for the next turn instead of being dropped.
    Release,
    Supersede,
    Delete,
    Restore,
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
            QueueAction::Release => match current {
                QueueStatus::Sending => QueueStatus::Queued,
                _ => return Err(illegal(Self::ENTITY, current, "release")),
            },
            QueueAction::Supersede => match current {
                QueueStatus::Queued => QueueStatus::Superseded,
                _ => return Err(illegal(Self::ENTITY, current, "supersede")),
            },
            QueueAction::Delete => match current {
                QueueStatus::Queued => QueueStatus::Dropped,
                _ => return Err(illegal(Self::ENTITY, current, "delete")),
            },
            QueueAction::Restore => match current {
                QueueStatus::Dropped => QueueStatus::Queued,
                _ => return Err(illegal(Self::ENTITY, current, "restore")),
            },
        };
        Ok(next)
    }

    pub fn is_terminal(status: QueueStatus) -> bool {
        matches!(status, QueueStatus::Sent | QueueStatus::Dropped)
    }
}

impl QueueAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::Enqueue => "enqueue",
            Self::Dispatch => "dispatch",
            Self::PromptSucceeded => "prompt_succeeded",
            Self::PromptFailed => "prompt_failed",
            Self::Release => "release",
            Self::Supersede => "supersede",
            Self::Delete => "delete",
            Self::Restore => "restore",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Enqueue => "Queued",
            Self::Dispatch => "Dispatching",
            Self::PromptSucceeded => "Sent",
            Self::PromptFailed => "Send failed",
            Self::Release => "Returned to queue",
            Self::Supersede => "Superseded",
            Self::Delete => "Remove from queue",
            Self::Restore => "Restore to queue",
        }
    }

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Delete | Self::Restore => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }

    #[must_use]
    pub fn commands(state: QueueStatus) -> &'static [Self] {
        match state {
            QueueStatus::Queued => &[Self::Delete],
            QueueStatus::Dropped => &[Self::Restore],
            QueueStatus::Sending | QueueStatus::Sent | QueueStatus::Superseded => &[],
        }
    }
}
