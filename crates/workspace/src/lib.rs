//! Workspace primitives: path containment, git worktrees and file watching.

pub mod files;
pub mod git;
pub mod path;

pub use git::{create_worktree, inspect, remove_worktree, GitError, GitInfo};
pub use path::{ensure_within, ensure_within_any, PathError};
