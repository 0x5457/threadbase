use threadbase_shared::ActionKind;
use threadbase_shared::TurnState;

use super::{illegal, TransitionError};

// ---------------------------------------------------------------------------
// Turn (in-memory, one per thread)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnAction {
    /// A sender claims the right to prepare the next turn.
    Claim,
    /// The sender gave up before installing a prompt.
    Abandon,
    /// The prompt handle is installed: the turn is now running.
    Install,
    /// The turn finished before the handle was installed.
    InstallAfterFinish,
    /// A completion event arrived while the turn was still starting.
    Complete,
    /// The turn ended normally.
    End,
    /// A caller asked to stop; while starting this is remembered.
    Cancel,
    /// Reconcile the slot with the persisted thread status.
    Sync { running: bool },
}

pub struct TurnMachine;

impl TurnMachine {
    pub const ENTITY: &'static str = "turn";

    pub fn transition(
        current: TurnState,
        action: &TurnAction,
    ) -> Result<TurnState, TransitionError> {
        use TurnState::{Empty, Running, Starting};
        let next = match action {
            TurnAction::Claim => match current {
                Empty => Starting,
                _ => return Err(illegal(Self::ENTITY, current, "claim")),
            },
            TurnAction::Abandon => match current {
                Starting => Empty,
                _ => return Err(illegal(Self::ENTITY, current, "abandon")),
            },
            TurnAction::Install => match current {
                Starting => Running,
                _ => return Err(illegal(Self::ENTITY, current, "install")),
            },
            TurnAction::InstallAfterFinish => match current {
                Starting => Empty,
                _ => return Err(illegal(Self::ENTITY, current, "install_after_finish")),
            },
            TurnAction::Complete => match current {
                // The slot stays claimed so the starter observes the race.
                Starting => Starting,
                _ => return Err(illegal(Self::ENTITY, current, "complete")),
            },
            TurnAction::End => match current {
                Running | Empty => Empty,
                Starting => Starting,
            },
            TurnAction::Cancel => match current {
                Starting => Starting,
                Running | Empty => Empty,
            },
            TurnAction::Sync { running } => {
                if *running {
                    current
                } else {
                    match current {
                        Running => Empty,
                        other => other,
                    }
                }
            }
        };
        Ok(next)
    }

    /// Commands a caller may invoke. Starting a turn is driven by the send
    /// path, so cancellation is the only user choice here.
    pub fn commands(state: TurnState) -> &'static [TurnAction] {
        match state {
            TurnState::Starting | TurnState::Running => &[TurnAction::Cancel],
            TurnState::Empty => &[],
        }
    }
}

impl TurnAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::Claim => "claim",
            Self::Abandon => "abandon",
            Self::Install => "install",
            Self::InstallAfterFinish => "install_after_finish",
            Self::Complete => "complete",
            Self::End => "end",
            Self::Cancel => "cancel",
            Self::Sync { .. } => "sync",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Claim => "Start a turn",
            Self::Abandon => "Release the start",
            Self::Install => "Prompt issued",
            Self::InstallAfterFinish => "Turn finished while starting",
            Self::Complete => "Turn finished",
            Self::End => "Turn ended",
            Self::Cancel => "Stop the turn",
            Self::Sync { .. } => "Reconcile with thread status",
        }
    }

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Cancel => ActionKind::Command,
            _ => ActionKind::Fact,
        }
    }
}
