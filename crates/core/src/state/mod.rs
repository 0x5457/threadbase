//! Explicit lifecycle state machines for Threadbase.
//!
//! Pure transition tables with guards. No IO: callers persist the returned
//! state and emit events / side effects. Machines are idempotent where an
//! action targets the current state.
//!
//! The authoritative persistence stays in SQLite; these tables are the single
//! source of truth for *which* transitions are legal. One file per machine.

mod approval;
mod auth;
mod catalogue;
mod client;
mod comment;
mod db;
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
pub use push::*;
pub use queue::*;
pub use setup::*;
pub use subagent::*;
pub use thread::*;
pub use turn::*;
pub use watch::*;

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
