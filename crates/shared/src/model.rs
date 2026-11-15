//! Domain and API model shared by the server and the web client.

use crate::ids::{
    ApprovalId, CommentId, ElicitationId, ProjectId, QueueItemId, SubagentId, ThreadId, ToolCallId,
    UserId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::{Display, EnumString, IntoStaticStr};
use ts_rs::TS;

/// Inherent `as_str` for wire enums whose `strum::IntoStaticStr` string is the
/// canonical wire value.
macro_rules! impl_as_str {
    ($($ty:ident),* $(,)?) => {
        $(
            impl $ty {
                #[must_use]
                pub fn as_str(self) -> &'static str {
                    self.into()
                }
            }
        )*
    };
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum Role {
    Owner,
    Editor,
    Viewer,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum UserAccess {
    /// Signed in, but an administrator has not approved the account yet.
    Pending,
    /// Approved to read. Every write is rejected server-side.
    ReadOnly,
    /// Normal access; thread roles still decide per-thread writes.
    Member,
    /// Access revoked. Existing sessions stop working immediately.
    Disabled,
}

impl UserAccess {
    #[must_use]
    pub fn can_read(self) -> bool {
        matches!(self, Self::ReadOnly | Self::Member)
    }

    #[must_use]
    pub fn can_write(self) -> bool {
        matches!(self, Self::Member)
    }

    /// Parse a persisted value; unknown values fall back to `Pending`.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        value.parse().unwrap_or(Self::Pending)
    }
}

impl Role {
    #[must_use]
    pub fn parse(value: &str) -> Self {
        value.parse().unwrap_or(Self::Viewer)
    }

    #[must_use]
    pub fn can_write(self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }

    #[must_use]
    pub fn can_manage(self) -> bool {
        matches!(self, Self::Owner)
    }

    #[must_use]
    pub fn rank(self) -> u8 {
        match self {
            Self::Owner => 3,
            Self::Editor => 2,
            Self::Viewer => 1,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum SetupStatus {
    None,
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum ThreadStatus {
    Preparing,
    Idle,
    Running,
    RequiresAction,
    Interrupted,
    Error,
    Archived,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum SubagentStatus {
    Running,
    RequiresAction,
    Idle,
    Cancelled,
    Error,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum Delivery {
    Injected,
    StoppedAndSent,
    Queued,
    Dropped,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum QueueStatus {
    Queued,
    Sending,
    Sent,
    Superseded,
    Dropped,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum ApprovalKind {
    Command,
    FileRead,
    FileWrite,
    Network,
    Other,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum ApprovalStatus {
    Pending,
    Allowed,
    Denied,
    Cancelled,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum ApprovalScope {
    Once,
    Session,
    Thread,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum ApprovalDecision {
    AllowOnce,
    AllowSession,
    AllowThread,
    Deny,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum ElicitationAction {
    Accept,
    Decline,
    Cancel,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum ElicitationMode {
    Form,
    Url,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum ElicitationStatus {
    Pending,
    Accepted,
    Declined,
    Cancelled,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum CommentStatus {
    Pending,
    Delivered,
    Resolved,
}

/// One thread's notification relationship with the current user.
///
/// Rows live in `thread_watchers`; this enum is the only interpretation of
/// them, so "no row", "row with muted=0" and "row with muted=1" never leak
/// out as two booleans that callers have to combine themselves.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum WatchState {
    /// No row: no notifications and no implicit follow.
    NotWatching,
    /// Row with muted=0: notified about pending items, mentions and turns.
    Watching,
    /// Row with muted=1: still a watcher, but notifications are suppressed.
    Muted,
}

impl WatchState {
    /// The `muted` column value for a state, or `None` when no row should exist.
    #[must_use]
    pub fn muted_value(self) -> Option<bool> {
        match self {
            Self::NotWatching => None,
            Self::Watching => Some(false),
            Self::Muted => Some(true),
        }
    }

    #[must_use]
    pub fn from_row(muted: Option<i64>) -> Self {
        match muted {
            None => Self::NotWatching,
            Some(0) => Self::Watching,
            Some(_) => Self::Muted,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum BaselineKind {
    BaseBranch,
    LastCommit,
    LastTurn,
    Commit,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum DiffLineKind {
    Context,
    Add,
    Delete,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum MessageRole {
    User,
    Agent,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    Other,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum ToolStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum PlanPriority {
    High,
    Medium,
    Low,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum PlanStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum InboxKind {
    Approval,
    Elicitation,
    Mention,
    Completed,
    Error,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum Panel {
    Transcript,
    Changes,
    Approvals,
    Files,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UserBrief {
    pub id: UserId,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub color: String,
}

impl UserBrief {
    #[must_use]
    pub fn from_user(user: &UserDto) -> Self {
        Self {
            id: user.id.clone(),
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            avatar_url: user.avatar_url.clone(),
            color: crate::user_color(user.id.as_str()).to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UserDto {
    pub id: UserId,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub is_admin: bool,
    pub access: UserAccess,
    pub hidden_presence: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Participant {
    pub user: UserBrief,
    pub role: Role,
    pub online: bool,
    pub last_seen_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeStats {
    pub files: u32,
    pub additions: u32,
    pub deletions: u32,
    pub seen: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectDto {
    pub id: ProjectId,
    pub name: String,
    pub path: String,
    pub remote_url: Option<String>,
    pub default_branch: String,
    /// Optional command run in a thread worktree before merge-back accepts it.
    pub verify_command: Option<String>,
    /// Optional command run once when a thread worktree is created.
    pub setup_command: Option<String>,
    pub active_threads: u32,
    pub waiting_approvals: u32,
    pub last_activity_at: Option<i64>,
    pub available: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadDto {
    pub id: ThreadId,
    pub project_id: ProjectId,
    pub parent_thread_id: Option<ThreadId>,
    pub title: String,
    pub status: ThreadStatus,
    /// Commands legal from `status`, from the thread machine. Menus and
    /// buttons are rendered from this list instead of comparing status
    /// strings, so the UI cannot offer an action the server would reject.
    pub allowed_actions: Vec<String>,
    pub agent: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub branch: String,
    pub base_branch: String,
    pub base_sha: Option<String>,
    pub worktree_path: Option<String>,
    pub setup_status: SetupStatus,
    pub setup_error: Option<String>,
    pub created_by: UserId,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
    pub pinned: bool,
    pub participants: Vec<Participant>,
    pub pending_approvals: u32,
    /// Subagents this thread delegated to. Empty for adapters that do not
    /// report them and for threads that never spawned any.
    #[serde(default)]
    pub subagents: Vec<SubagentDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SubagentDto {
    pub id: SubagentId,
    pub thread_id: ThreadId,
    pub parent_id: Option<SubagentId>,
    pub title: String,
    pub description: Option<String>,
    pub status: SubagentStatus,
    pub cwd: Option<String>,
    pub can_cancel: bool,
    pub usage: Option<Usage>,
    pub last_seq: u64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub context_used: Option<u64>,
    pub context_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApprovalOption {
    pub id: String,
    pub kind: ApprovalDecision,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApprovalDto {
    pub id: ApprovalId,
    pub thread_id: ThreadId,
    pub subagent_id: Option<SubagentId>,
    pub turn_id: Option<String>,
    pub tool_call_id: Option<ToolCallId>,
    pub agent: String,
    pub title: String,
    pub kind: ApprovalKind,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub paths: Vec<String>,
    pub options: Vec<ApprovalOption>,
    pub input: Option<String>,
    pub status: ApprovalStatus,
    pub decision: Option<ApprovalDecision>,
    pub scope: Option<ApprovalScope>,
    pub decided_by: Option<UserBrief>,
    pub decided_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ElicitationDto {
    pub id: ElicitationId,
    pub thread_id: ThreadId,
    pub subagent_id: Option<SubagentId>,
    pub tool_call_id: Option<ToolCallId>,
    pub agent: String,
    pub message: String,
    pub mode: ElicitationMode,
    #[ts(type = "unknown")]
    pub requested_schema: Value,
    pub blocking: bool,
    pub status: ElicitationStatus,
    #[ts(type = "unknown")]
    pub response: Option<Value>,
    pub responded_by: Option<UserBrief>,
    pub responded_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CommentSide {
    /// The old file column; a comment on a deleted line.
    Old,
    /// The new file column; a comment on an added or context line.
    New,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CommentAnchor {
    Message {
        message_id: String,
    },
    ToolCall {
        tool_call_id: String,
    },
    Elicitation {
        elicitation_id: String,
    },
    File {
        path: String,
        line: u32,
        context_hash: Option<String>,
    },
    Diff {
        path: String,
        hunk_id: String,
        line: u32,
        /// Which column the line number belongs to. Older anchors do not have
        /// it; the reader falls back to finding the number on either side.
        #[serde(default)]
        side: Option<CommentSide>,
        context_hash: Option<String>,
    },
    Thread,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommentDto {
    pub id: CommentId,
    pub thread_id: ThreadId,
    pub parent_id: Option<CommentId>,
    pub author: UserBrief,
    pub body: String,
    pub anchor: CommentAnchor,
    pub anchor_changed: bool,
    pub status: CommentStatus,
    pub created_at: i64,
    pub updated_at: i64,
    pub delivered_seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QueueItemDto {
    pub id: QueueItemId,
    pub thread_id: ThreadId,
    pub author: UserBrief,
    pub text: String,
    pub status: QueueStatus,
    pub position: u32,
    pub created_at: i64,
}

/// A stored file a user attached to a message. Bytes live on disk; the DTO is
/// what events carry so a cold reload can still render the image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AttachmentDto {
    pub id: String,
    pub mime_type: String,
    pub byte_size: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InboxItemDto {
    pub id: String,
    pub kind: InboxKind,
    pub actor: Option<UserBrief>,
    pub thread_id: ThreadId,
    pub thread_title: String,
    pub project_id: ProjectId,
    pub summary: String,
    pub created_at: i64,
    pub read: bool,
    pub approval_id: Option<ApprovalId>,
    pub elicitation_id: Option<ElicitationId>,
    pub comment_id: Option<CommentId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileEntryDto {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileContentDto {
    pub path: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BaselineDto {
    pub kind: BaselineKind,
    pub commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileChangeDto {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
    pub seen: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiffLineDto {
    pub kind: DiffLineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiffHunkDto {
    pub id: String,
    pub header: String,
    pub context_hash: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLineDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileDiffDto {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub binary: bool,
    pub hunks: Vec<DiffHunkDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangesDto {
    pub baseline: BaselineDto,
    pub base_label: String,
    pub files: Vec<FileChangeDto>,
    pub stats: ChangeStats,
    pub commit: Option<String>,
}

/// One commit in a thread's worktree history. The subject is git's own, but
/// `checkpoint` marks the snapshots Threadbase writes before a turn and
/// `turn_prompt` names the turn whose work the snapshot captured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommitSummaryDto {
    pub sha: String,
    pub short_sha: String,
    /// Committer time in Unix milliseconds.
    pub at: i64,
    pub author: String,
    pub subject: String,
    pub additions: u32,
    pub deletions: u32,
    pub files: u32,
    pub checkpoint: bool,
    pub turn_id: Option<String>,
    pub turn_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadHistoryDto {
    pub base_branch: String,
    pub base_sha: Option<String>,
    pub head: Option<String>,
    /// Newest first. An empty list means the worktree has not committed
    /// anything since the base commit yet.
    pub entries: Vec<CommitSummaryDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlanEntry {
    pub content: String,
    pub priority: PlanPriority,
    pub status: PlanStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ToolLocation {
    pub path: String,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiffSummary {
    pub path: String,
    pub additions: u32,
    pub deletions: u32,
    pub patch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentInfoDto {
    pub id: String,
    pub label: String,
    pub installed: bool,
    pub version: Option<String>,
    pub adapter: Option<String>,
    pub adapter_version: Option<String>,
    pub supports_steering: bool,
    pub supports_subagents: bool,
    pub supports_elicitation: bool,
    pub identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DoctorReport {
    pub node: Option<String>,
    pub git: Option<String>,
    pub agents: Vec<AgentInfoDto>,
    pub data_dir: String,
    pub database: String,
    pub bind: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AuthMeDto {
    pub user: UserDto,
    pub auth_method: String,
    pub needs_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AuthStatusDto {
    /// GitHub OAuth is configured, so a sign-in redirect is available.
    pub github: bool,
    pub dev_identity: bool,
    pub needs_admin: bool,
    /// Where to send the browser to start a GitHub sign-in.
    pub login_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SettingsDto {
    pub theme: String,
    pub text_width: String,
    pub subthreads_per_thread: u32,
    pub send_with_modifier: bool,
    pub default_agent: String,
    pub default_model: Option<String>,
    pub default_baseline: BaselineKind,
    pub hidden_presence: bool,
    pub webhook_url: Option<String>,
    pub browser_notifications: bool,
}

impl Default for SettingsDto {
    fn default() -> Self {
        Self {
            theme: "dark".to_owned(),
            text_width: "default".to_owned(),
            subthreads_per_thread: 10,
            send_with_modifier: false,
            default_agent: "codex".to_owned(),
            default_model: None,
            default_baseline: BaselineKind::BaseBranch,
            hidden_presence: false,
            webhook_url: None,
            browser_notifications: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentConfigValueDto {
    pub value: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentConfigOptionDto {
    pub id: String,
    pub name: String,
    pub category: String,
    pub current_value: String,
    pub options: Vec<AgentConfigValueDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentConfigResponse {
    pub options: Vec<AgentConfigOptionDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DirectoryEntryDto {
    pub name: String,
    pub path: String,
    pub is_git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DirectoryListingDto {
    pub path: String,
    pub parent: Option<String>,
    pub is_git: bool,
    pub entries: Vec<DirectoryEntryDto>,
}

/// In-memory state of one thread's single turn slot.
///
/// The ACP connection allows exactly one turn at a time. This models the
/// claim/install/clear protocol that enforces it, including the "turn ended
/// while still starting" race the harness creates.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum TurnState {
    Empty,
    Starting,
    Running,
}

/// A server-side sign-in session. Derived from `expires_at` / `revoked_at`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum AuthSessionState {
    Active,
    Expired,
    Revoked,
}

/// Delivery state of one inbox row to Web Push.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum PushStatus {
    Pending,
    Sent,
    Failed,
    /// Read before it could be delivered, or the user has no subscription.
    Skipped,
}

/// Browser WebSocket lifecycle for one thread subscription.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum ConnectionPhase {
    Idle,
    Loading,
    Live,
    Reconnecting,
    Failed,
}

/// Shared composer draft lifecycle (y-websocket provider).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum DraftPhase {
    Idle,
    Connecting,
    /// Socket is up, first sync round-trip has not completed.
    Syncing,
    Live,
    Offline,
}

/// Browser terminal lifecycle for one tmux session.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TS,
    Display,
    EnumString,
    IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[ts(export)]
pub enum TerminalPhase {
    Connecting,
    Live,
    Exited,
    Error,
}

impl_as_str!(
    Role,
    UserAccess,
    ThreadStatus,
    SetupStatus,
    SubagentStatus,
    Delivery,
    QueueStatus,
    ApprovalStatus,
    ApprovalDecision,
    ElicitationStatus,
    CommentStatus,
    WatchState,
    TurnState,
    AuthSessionState,
    PushStatus,
    ConnectionPhase,
    DraftPhase,
    TerminalPhase,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_enums_use_snake_case_strings() {
        assert_eq!(UserAccess::ReadOnly.as_str(), "read_only");
        assert_eq!("read_only".parse::<UserAccess>(), Ok(UserAccess::ReadOnly));
        assert!("unknown".parse::<UserAccess>().is_err());
        assert_eq!(UserAccess::parse("unknown"), UserAccess::Pending);
        assert_eq!(Role::Owner.as_str(), "owner");
        assert_eq!(Role::parse("unknown"), Role::Viewer);
        assert_eq!(ThreadStatus::RequiresAction.as_str(), "requires_action");
        assert_eq!(Delivery::StoppedAndSent.as_str(), "stopped_and_sent");
        assert_eq!(
            serde_json::to_string(&UserAccess::ReadOnly).unwrap(),
            "\"read_only\""
        );
    }
}
