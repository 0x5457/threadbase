use threadbase_shared::ActionKind;
use threadbase_shared::SetupStatus;

use super::{illegal, TransitionError};

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupAction {
    /// Run the project's setup command in this thread's worktree.
    Start,
    /// The command actually began executing.
    Started,
    Finish,
    Fail,
    /// The server stopped while setup was pending or running.
    Interrupt,
}

pub struct SetupMachine;

impl SetupMachine {
    pub const ENTITY: &'static str = "setup";

    pub fn transition(
        current: SetupStatus,
        action: &SetupAction,
    ) -> Result<SetupStatus, TransitionError> {
        let next = match action {
            SetupAction::Start => match current {
                SetupStatus::Pending | SetupStatus::Running => {
                    return Err(illegal(Self::ENTITY, current, "start"));
                }
                _ => SetupStatus::Pending,
            },
            SetupAction::Started => match current {
                SetupStatus::Pending => SetupStatus::Running,
                _ => return Err(illegal(Self::ENTITY, current, "started")),
            },
            SetupAction::Finish => match current {
                SetupStatus::Running => SetupStatus::Succeeded,
                _ => return Err(illegal(Self::ENTITY, current, "finish")),
            },
            SetupAction::Fail => match current {
                SetupStatus::Running => SetupStatus::Failed,
                _ => return Err(illegal(Self::ENTITY, current, "fail")),
            },
            SetupAction::Interrupt => match current {
                SetupStatus::Pending | SetupStatus::Running => SetupStatus::Failed,
                _ => return Err(illegal(Self::ENTITY, current, "interrupt")),
            },
        };
        Ok(next)
    }

    /// Actions a caller may choose. `none` and a finished setup can run again.
    pub fn commands(state: SetupStatus) -> &'static [SetupAction] {
        match state {
            SetupStatus::Pending | SetupStatus::Running => &[],
            SetupStatus::None | SetupStatus::Succeeded | SetupStatus::Failed => {
                &[SetupAction::Start]
            }
        }
    }

    pub fn is_busy(state: SetupStatus) -> bool {
        matches!(state, SetupStatus::Pending | SetupStatus::Running)
    }
}

impl SetupAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Started => "started",
            Self::Finish => "finish",
            Self::Fail => "fail",
            Self::Interrupt => "interrupt",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Start => "Run setup",
            Self::Started => "Setup started",
            Self::Finish => "Setup finished",
            Self::Fail => "Setup failed",
            Self::Interrupt => "Setup interrupted by restart",
        }
    }

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Start => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }
}
