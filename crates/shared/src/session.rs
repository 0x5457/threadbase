//! Read model for the Sessions surface: agent threads and host tmux sessions.
//!
//! A session is a *view* of work you can open, not a persisted entity. Threads
//! stay threads (project, agent, events, approvals); tmux sessions stay live
//! terminal processes. This module only composes them into one list so the UI
//! can sort, filter and link to both without inventing a polymorphic table.

use crate::ids::{ProjectId, ThreadId};
use crate::model::{ThreadStatus, UserBrief};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The workbench shell owned by a thread, when it is alive on the private
/// socket. It is deliberately small: the Sessions row only needs to know that
/// the shell exists and who is watching it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorkbenchShellSummaryDto {
    /// Visible browsers attached through threadbase, including the driver.
    pub viewers: Vec<UserBrief>,
    /// The visible user holding the keyboard, when somebody is driving.
    pub controller: Option<UserBrief>,
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
        /// A non-empty collaborative composer draft exists for this thread.
        has_draft: bool,
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
        /// The registered project that contains the live cwd, when there is one.
        project_id: Option<ProjectId>,
        project_name: Option<String>,
        /// The thread this tmux session was turned into, when linked.
        link: Option<TmuxLinkDto>,
        windows: u32,
        /// tmux clients, including local terminals.
        attached: u32,
        /// Visible browsers attached through threadbase.
        viewers: Vec<UserBrief>,
        /// The visible browser holding the keyboard, when somebody is driving.
        controller: Option<UserBrief>,
        created_at: i64,
        activity_at: i64,
    },
}

/// The thread a host tmux session was turned into. The stored tmux
/// `created_at` is a fingerprint: a later session reusing the same `$N` does
/// not inherit this link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TmuxLinkDto {
    pub project_id: ProjectId,
    pub project_name: String,
    pub thread_id: Option<ThreadId>,
    pub thread_title: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateThreadFromTmuxRequest {
    pub title: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub worktree: Option<bool>,
    pub initial_prompt: Option<String>,
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
