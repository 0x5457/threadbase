//! Thread aggregate: lifecycle, listing and the bundle the UI opens with.
//!
//! Each submodule owns one layer:
//! - [`lifecycle`] validates state transitions and owns worktree mutations,
//! - [`query`] builds list/detail DTOs with batched reads,
//! - [`bundle`] composes the single-thread read model and membership,
//! - [`sweep`] parks idle threads and wakes due snoozes without a client.

mod bundle;
mod lifecycle;
mod query;
mod sweep;
mod title;

pub use bundle::bundle;
pub use lifecycle::{
    create, delete, fork, place, reconcile_after_restart, status, transition, update, wake,
};
pub use query::{get, ids_for_project, list, list_all, load_row, permission_mode, ThreadRow};
pub use sweep::{spawn_sweeper, sweep, SweepReport};
pub use title::{backfill_titles, title_from_prompt};
