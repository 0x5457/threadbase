use threadbase_shared::ActionKind;
use threadbase_shared::SetupStatus;

use super::{illegal, Actions, States, TransitionError};

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    pub fn is_busy(state: SetupStatus) -> bool {
        matches!(state, SetupStatus::Pending | SetupStatus::Running)
    }
}

impl States for SetupStatus {
    fn all() -> &'static [Self] {
        &[
            Self::None,
            Self::Pending,
            Self::Running,
            Self::Succeeded,
            Self::Failed,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::None => "Not configured",
            Self::Pending => "Queued",
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
        }
    }
}

impl Actions for SetupAction {
    type State = SetupStatus;

    const ALL: &'static [Self] = &[
        Self::Start,
        Self::Started,
        Self::Finish,
        Self::Fail,
        Self::Interrupt,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Started => "started",
            Self::Finish => "finish",
            Self::Fail => "fail",
            Self::Interrupt => "interrupt",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Start => "Run setup",
            Self::Started => "Setup started",
            Self::Finish => "Setup finished",
            Self::Fail => "Setup failed",
            Self::Interrupt => "Setup interrupted by restart",
        }
    }

    fn kind(self) -> ActionKind {
        match self {
            Self::Start => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }

    /// `none` and a finished setup can run again.
    fn commands(state: SetupStatus) -> &'static [Self] {
        match state {
            SetupStatus::Pending | SetupStatus::Running => &[],
            SetupStatus::None | SetupStatus::Succeeded | SetupStatus::Failed => &[Self::Start],
        }
    }
}
