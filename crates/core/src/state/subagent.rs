use threadbase_shared::ActionKind;
use threadbase_shared::SubagentStatus;

use super::{illegal, TransitionError};

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

impl SubagentAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Run => "run",
            Self::Idle => "idle",
            Self::RequireAction => "require_action",
            Self::Resolve => "resolve",
            Self::Fail => "fail",
            Self::Cancel => "cancel",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Create => "Created",
            Self::Run => "Running",
            Self::Idle => "Idle",
            Self::RequireAction => "Needs input",
            Self::Resolve => "Input received",
            Self::Fail => "Failed",
            Self::Cancel => "Cancel subagent",
        }
    }

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Cancel => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }

    #[must_use]
    pub fn commands(state: SubagentStatus) -> &'static [Self] {
        match state {
            SubagentStatus::Idle | SubagentStatus::Running | SubagentStatus::RequiresAction => {
                &[Self::Cancel]
            }
            SubagentStatus::Cancelled | SubagentStatus::Error => &[],
        }
    }
}
