use threadbase_shared::ActionKind;
use threadbase_shared::ThreadStatus;

use super::{guard, illegal, TransitionError};

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

impl ThreadAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Restart => "restart",
            Self::StartSucceeded => "start_succeeded",
            Self::StartFailed => "start_failed",
            Self::Prompt => "prompt",
            Self::ApprovalPending => "approval_pending",
            Self::ElicitationPending => "elicitation_pending",
            Self::ApprovalResolved { .. } => "approval_resolved",
            Self::ElicitationResolved { .. } => "elicitation_resolved",
            Self::TurnEnded => "turn_ended",
            Self::TurnError => "turn_error",
            Self::ProcessExited => "process_exited",
            Self::AgentFailed => "agent_failed",
            Self::Cancel => "cancel",
            Self::Archive => "archive",
            Self::Unarchive => "unarchive",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Create => "Create",
            Self::Restart => "Restart agent",
            Self::StartSucceeded => "Agent started",
            Self::StartFailed => "Agent failed to start",
            Self::Prompt => "Send a prompt",
            Self::ApprovalPending => "Approval requested",
            Self::ElicitationPending => "Question asked",
            Self::ApprovalResolved { .. } => "Approval resolved",
            Self::ElicitationResolved { .. } => "Question answered",
            Self::TurnEnded => "Turn ended",
            Self::TurnError => "Turn failed",
            Self::ProcessExited => "Agent process exited",
            Self::AgentFailed => "Agent failed",
            Self::Cancel => "Stop the turn",
            Self::Archive => "Archive",
            Self::Unarchive => "Unarchive",
        }
    }

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Restart | Self::Prompt | Self::Cancel | Self::Archive | Self::Unarchive => {
                ActionKind::Command
            }
            _ => ActionKind::Fact,
        }
    }

    /// Commands a caller may invoke from `state`, in display order.
    #[must_use]
    pub fn commands(state: ThreadStatus) -> &'static [Self] {
        match state {
            ThreadStatus::Preparing => &[Self::Restart, Self::Archive],
            ThreadStatus::Idle => &[Self::Prompt, Self::Restart, Self::Archive],
            ThreadStatus::Running => &[Self::Prompt, Self::Cancel, Self::Restart, Self::Archive],
            ThreadStatus::RequiresAction => {
                &[Self::Prompt, Self::Cancel, Self::Restart, Self::Archive]
            }
            ThreadStatus::Interrupted => &[Self::Prompt, Self::Restart, Self::Archive],
            ThreadStatus::Error => &[Self::Prompt, Self::Restart, Self::Archive],
            ThreadStatus::Archived => &[Self::Unarchive],
        }
    }
}
