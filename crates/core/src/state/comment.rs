use threadbase_shared::ActionKind;
use threadbase_shared::CommentStatus;

use super::{illegal, Actions, States, TransitionError};

// ---------------------------------------------------------------------------
// Comment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl States for CommentStatus {
    fn all() -> &'static [Self] {
        &[Self::Pending, Self::Delivered, Self::Resolved]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Delivered => "Delivered",
            Self::Resolved => "Resolved",
        }
    }

    fn terminal(self) -> bool {
        matches!(self, Self::Resolved)
    }
}

impl Actions for CommentAction {
    type State = CommentStatus;

    const ALL: &'static [Self] = &[
        Self::CreatePending,
        Self::CreateDelivered,
        Self::Deliver,
        Self::Resolve,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::CreatePending => "create_pending",
            Self::CreateDelivered => "create_delivered",
            Self::Deliver => "deliver",
            Self::Resolve => "resolve",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::CreatePending => "Comment added",
            Self::CreateDelivered => "Comment added",
            Self::Deliver => "Send to agent",
            Self::Resolve => "Resolve",
        }
    }

    fn kind(self) -> ActionKind {
        match self {
            Self::Deliver | Self::Resolve => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }

    fn commands(state: CommentStatus) -> &'static [Self] {
        match state {
            CommentStatus::Pending => &[Self::Deliver, Self::Resolve],
            CommentStatus::Delivered => &[Self::Resolve],
            CommentStatus::Resolved => &[],
        }
    }
}
