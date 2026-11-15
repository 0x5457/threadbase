use threadbase_shared::ActionKind;
use threadbase_shared::WatchState;

use super::{guard, illegal, TransitionError};

// ---------------------------------------------------------------------------
// Watch
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchAction {
    /// Follow the thread and receive notifications.
    Watch,
    /// Keep following but suppress notifications.
    Mute,
    /// Resume notifications without changing who watches.
    Unmute,
    /// Stop following entirely.
    Unwatch,
}

pub struct WatchMachine;

impl WatchMachine {
    pub const ENTITY: &'static str = "watch";

    /// Apply a command. Watches are a single value, not two flags: every
    /// command lands on exactly one [`WatchState`], so callers never have to
    /// reason about the order of `watching` and `muted` updates.
    pub fn transition(
        current: WatchState,
        action: WatchAction,
    ) -> Result<WatchState, TransitionError> {
        let next = match action {
            WatchAction::Watch => WatchState::Watching,
            WatchAction::Mute => match current {
                WatchState::NotWatching => {
                    return Err(illegal(Self::ENTITY, current, "mute"));
                }
                _ => WatchState::Muted,
            },
            WatchAction::Unmute => match current {
                WatchState::Muted => WatchState::Watching,
                _ => return Err(illegal(Self::ENTITY, current, "unmute")),
            },
            WatchAction::Unwatch => WatchState::NotWatching,
        };
        Ok(next)
    }

    /// Resolve the command that produces `target` from `current`.
    pub fn action_for(
        current: WatchState,
        target: WatchState,
    ) -> Result<WatchAction, TransitionError> {
        let action = match target {
            WatchState::NotWatching => WatchAction::Unwatch,
            WatchState::Watching if current == WatchState::Muted => WatchAction::Unmute,
            WatchState::Watching => WatchAction::Watch,
            WatchState::Muted if current == WatchState::NotWatching => {
                return Err(guard(Self::ENTITY, "mute needs a watch first"));
            }
            WatchState::Muted => WatchAction::Mute,
        };
        Ok(action)
    }

    pub fn commands(state: WatchState) -> &'static [WatchAction] {
        match state {
            WatchState::NotWatching => &[WatchAction::Watch],
            WatchState::Watching => &[WatchAction::Mute, WatchAction::Unwatch],
            WatchState::Muted => &[WatchAction::Unmute, WatchAction::Unwatch],
        }
    }
}

impl WatchAction {
    #[must_use]
    pub fn id(&self) -> &'static str {
        match self {
            Self::Watch => "watch",
            Self::Mute => "mute",
            Self::Unmute => "unmute",
            Self::Unwatch => "unwatch",
        }
    }

    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Watch => "Watch thread",
            Self::Mute => "Mute notifications",
            Self::Unmute => "Unmute notifications",
            Self::Unwatch => "Stop watching",
        }
    }

    #[must_use]
    pub fn kind(&self) -> ActionKind {
        ActionKind::Command
    }
}
