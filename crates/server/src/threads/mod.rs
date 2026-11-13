//! Thread aggregate: lifecycle, listing and the bundle the UI opens with.
//!
//! Each submodule owns one layer:
//! - [`lifecycle`] validates state transitions and owns worktree mutations,
//! - [`query`] builds list/detail DTOs with batched reads,
//! - [`bundle`] composes the single-thread read model and membership.

mod bundle;
mod lifecycle;
mod query;

pub use bundle::bundle;
pub use lifecycle::{create, delete, fork, reconcile_after_restart, status, transition, update};
pub use query::{get, list, list_all, load_row, ThreadRow};
