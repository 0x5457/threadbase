use threadbase_shared::ActionKind;
use threadbase_shared::PushStatus;

use super::{illegal, TransitionError};

// ---------------------------------------------------------------------------
// Push delivery
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushAction {
    Enqueue,
    Deliver,
    /// One attempt failed; a retry is scheduled.
    Fail,
    /// Attempts are exhausted.
    GiveUp,
    /// Nothing to deliver: already read, or no subscription.
    Skip,
    Retry,
}

pub struct PushMachine;

impl PushMachine {
    pub const ENTITY: &'static str = "push";

    pub fn transition(
        current: PushStatus,
        action: &PushAction,
    ) -> Result<PushStatus, TransitionError> {
        let next = match action {
            PushAction::Enqueue => PushStatus::Pending,
            PushAction::Deliver => match current {
                PushStatus::Pending => PushStatus::Sent,
                _ => return Err(illegal(Self::ENTITY, current, "deliver")),
            },
            PushAction::Fail => match current {
                PushStatus::Pending => PushStatus::Pending,
                _ => return Err(illegal(Self::ENTITY, current, "fail")),
            },
            PushAction::GiveUp => match current {
                PushStatus::Pending => PushStatus::Failed,
                _ => return Err(illegal(Self::ENTITY, current, "give_up")),
            },
            PushAction::Skip => match current {
                PushStatus::Pending => PushStatus::Skipped,
                _ => return Err(illegal(Self::ENTITY, current, "skip")),
            },
            PushAction::Retry => match current {
                PushStatus::Failed | PushStatus::Skipped => PushStatus::Pending,
                PushStatus::Pending | PushStatus::Sent => {
                    return Err(illegal(Self::ENTITY, current, "retry"));
                }
            },
        };
        Ok(next)
    }

    pub fn commands(state: PushStatus) -> &'static [PushAction] {
        match state {
            PushStatus::Failed | PushStatus::Skipped => &[PushAction::Retry],
            _ => &[],
        }
    }

    pub fn is_terminal(state: PushStatus) -> bool {
        matches!(
            state,
            PushStatus::Sent | PushStatus::Failed | PushStatus::Skipped
        )
    }
}

impl PushAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::Enqueue => "enqueue",
            Self::Deliver => "deliver",
            Self::Fail => "fail",
            Self::GiveUp => "give_up",
            Self::Skip => "skip",
            Self::Retry => "retry",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Enqueue => "Queued",
            Self::Deliver => "Delivered",
            Self::Fail => "Attempt failed",
            Self::GiveUp => "Delivery failed",
            Self::Skip => "Skipped",
            Self::Retry => "Retry delivery",
        }
    }

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Retry => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }
}
