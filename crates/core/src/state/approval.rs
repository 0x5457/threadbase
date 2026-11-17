use threadbase_shared::ActionKind;
use threadbase_shared::{ApprovalStatus, ElicitationStatus};

use super::{illegal, Actions, States, TransitionError};

// ---------------------------------------------------------------------------
// Approval
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl States for ApprovalStatus {
    fn all() -> &'static [Self] {
        &[Self::Pending, Self::Allowed, Self::Denied, Self::Cancelled]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Allowed => "Allowed",
            Self::Denied => "Denied",
            Self::Cancelled => "Cancelled",
        }
    }

    fn terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }
}

impl States for ElicitationStatus {
    fn all() -> &'static [Self] {
        &[
            Self::Pending,
            Self::Accepted,
            Self::Declined,
            Self::Cancelled,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Accepted => "Answered",
            Self::Declined => "Declined",
            Self::Cancelled => "Cancelled",
        }
    }

    fn terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }
}

impl Actions for ApprovalAction {
    type State = ApprovalStatus;

    const ALL: &'static [Self] = &[Self::Create, Self::Allow, Self::Deny, Self::Cancel];

    fn id(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Cancel => "cancel",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Create => "Requested",
            Self::Allow => "Allow",
            Self::Deny => "Deny",
            Self::Cancel => "Cancel",
        }
    }

    fn kind(self) -> ActionKind {
        match self {
            Self::Create => ActionKind::Fact,
            _ => ActionKind::Command,
        }
    }

    fn commands(state: ApprovalStatus) -> &'static [Self] {
        match state {
            ApprovalStatus::Pending => &[Self::Allow, Self::Deny, Self::Cancel],
            ApprovalStatus::Allowed | ApprovalStatus::Denied | ApprovalStatus::Cancelled => &[],
        }
    }
}

impl Actions for ElicitationAction {
    type State = ElicitationStatus;

    const ALL: &'static [Self] = &[Self::Create, Self::Accept, Self::Decline, Self::Cancel];

    fn id(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Accept => "accept",
            Self::Decline => "decline",
            Self::Cancel => "cancel",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Create => "Asked",
            Self::Accept => "Answer",
            Self::Decline => "Decline",
            Self::Cancel => "Cancel",
        }
    }

    fn kind(self) -> ActionKind {
        match self {
            Self::Create => ActionKind::Fact,
            _ => ActionKind::Command,
        }
    }

    fn commands(state: ElicitationStatus) -> &'static [Self] {
        match state {
            ElicitationStatus::Pending => &[Self::Accept, Self::Decline, Self::Cancel],
            ElicitationStatus::Accepted
            | ElicitationStatus::Declined
            | ElicitationStatus::Cancelled => &[],
        }
    }
}
