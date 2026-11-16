//! Read model for the Sessions surface: agent threads and host tmux sessions.
//!
//! A session is a *view* of work you can open, not a persisted entity. Threads
//! stay threads (project, agent, events, approvals); tmux sessions stay live
//! terminal processes. This module only composes them into one list so the UI
//! can sort, filter and link to both without inventing a polymorphic table.

use crate::ids::{ProjectId, ThreadId, UserId};
use crate::model::ThreadStatus;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The workbench shell owned by a thread, when it is alive on the private
/// socket. It is deliberately small: the Sessions row only needs to know that
/// the shell exists and who is watching it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorkbenchShellSummaryDto {
    /// Browsers attached through threadbase, including the driver.
    pub viewers: u32,
    /// The user holding the keyboard, when somebody is driving.
    pub controller: Option<UserId>,
    /// Whether the caller may type; members normally only watch workbench
    /// shells, while administrators and configured drivers may drive them.
    pub can_drive: bool,
}

/// One row in the Sessions surface.
///
/// The tag is the discriminator the client switches on. A thread row opens the
/// transcript; a tmux row opens the terminal drawer. Keeping the payloads
/// separate means each kind carries exactly the actions and permissions it
/// supports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum SessionSummaryDto {
    Thread {
        id: ThreadId,
        title: String,
        project_id: ProjectId,
        project_name: String,
        agent: String,
        status: ThreadStatus,
        /// Coalesced `last_activity_at`/`updated_at`, in milliseconds.
        last_activity_at: i64,
        pinned: bool,
        pending_approvals: u32,
        /// Present when the thread's private workbench shell is running.
        terminal: Option<WorkbenchShellSummaryDto>,
    },
    Tmux {
        /// tmux's stable `$N` session id.
        id: String,
        socket: String,
        name: String,
        cwd: Option<String>,
        command: Option<String>,
        windows: u32,
        /// tmux clients, including local terminals.
        attached: u32,
        /// Browsers attached through threadbase.
        viewers: u32,
        created_at: i64,
        activity_at: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionsResponse {
    /// Pinned first, then active work, then most recent activity.
    pub sessions: Vec<SessionSummaryDto>,
    /// Non-fatal source failures; the rest of the list still renders.
    pub warnings: Vec<String>,
}
