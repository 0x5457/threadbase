use threadbase_shared::ActionKind;
use threadbase_shared::CommentStatus;

use super::{illegal, TransitionError};

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

impl CommentAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::CreatePending => "create_pending",
            Self::CreateDelivered => "create_delivered",
            Self::Deliver => "deliver",
            Self::Resolve => "resolve",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::CreatePending => "Comment added",
            Self::CreateDelivered => "Comment added",
            Self::Deliver => "Send to agent",
            Self::Resolve => "Resolve",
        }
    }

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Deliver | Self::Resolve => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }

    #[must_use]
    pub fn commands(state: CommentStatus) -> &'static [Self] {
        match state {
            CommentStatus::Pending => &[Self::Deliver, Self::Resolve],
            CommentStatus::Delivered => &[Self::Resolve],
            CommentStatus::Resolved => &[],
        }
    }
}
