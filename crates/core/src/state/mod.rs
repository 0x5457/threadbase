//! Explicit lifecycle state machines for Threadbase.
//!
//! Pure transition tables with guards. No IO: callers persist the returned
//! state and emit events / side effects. Machines are idempotent where an
//! action targets the current state.
//!
//! The authoritative persistence stays in SQLite; these tables are the single
//! source of truth for *which* transitions are legal. One file per machine,
//! alongside the metadata [`catalogue()`] publishes to clients.

mod approval;
mod auth;
mod catalogue;
mod client;
mod comment;
mod db;
mod placement;
mod push;
mod queue;
mod setup;
mod subagent;
mod thread;
mod turn;
mod watch;

#[cfg(test)]
mod test_suite;

pub use approval::*;
pub use auth::*;
pub use catalogue::*;
pub use client::*;
pub use comment::*;
pub use db::*;
pub use placement::*;
pub use push::*;
pub use queue::*;
pub use setup::*;
pub use subagent::*;
pub use thread::*;
pub use turn::*;
pub use watch::*;

use threadbase_shared::ActionKind;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransitionError {
    #[error("{entity}: illegal action {action} from {from}")]
    Illegal {
        entity: &'static str,
        from: String,
        action: String,
    },
    #[error("{entity}: guard failed: {reason}")]
    Guard {
        entity: &'static str,
        reason: String,
    },
}

fn illegal(entity: &'static str, from: impl std::fmt::Debug, action: &str) -> TransitionError {
    TransitionError::Illegal {
        entity,
        from: format!("{from:?}"),
        action: action.to_owned(),
    }
}

fn guard(entity: &'static str, reason: impl Into<String>) -> TransitionError {
    TransitionError::Guard {
        entity,
        reason: reason.into(),
    }
}

// ---------------------------------------------------------------------------
// Publication metadata
// ---------------------------------------------------------------------------

/// One status enum as the catalogue publishes it.
///
/// `Into<&'static str>` is the wire value (`strum::IntoStaticStr`), so a state
/// can never be published under a name the database does not store.
pub trait States: Copy + Into<&'static str> + 'static {
    /// Every state, in display order.
    fn all() -> &'static [Self];
    /// Human label for banners, menus and tooltips.
    fn label(self) -> &'static str;
    /// Finished: the entity is done, and only a revival command leaves it.
    fn terminal(self) -> bool {
        false
    }
}

/// One machine's action type: the metadata the catalogue publishes plus the
/// commands a caller may invoke.
pub trait Actions: Copy + 'static {
    type State: States;

    /// Every action, in display order. Parameterised actions carry the
    /// canonical value the catalogue publishes.
    const ALL: &'static [Self];

    fn id(self) -> &'static str;
    fn label(self) -> &'static str;
    fn kind(self) -> ActionKind;

    /// User-invocable actions from `state`, in display order. Facts are never
    /// listed: nobody can invoke them.
    fn commands(state: Self::State) -> &'static [Self];
}
