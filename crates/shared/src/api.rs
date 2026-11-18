//! API level request and response envelopes.

use crate::event::EventEnvelope;
use crate::ids::{CommentId, ProjectId, ThreadId, UserId};
use crate::model::{
    ApprovalDecision, ApprovalDto, AuthStatusDto, AutoSettle, BaselineDto, ChangesDto,
    CommentAnchor, CommentDto, CommentStatus, Delivery, ElicitationAction, ElicitationDto,
    FileContentDto, FileEntryDto, GoalDto, GuestMessageStatus, InboxItemDto, McpScope,
    PermissionMode, ProjectDto, QueueItemDto, Role, SettingsDto, SubagentDto, ThreadDto,
    ThreadHistoryDto, UserAccess, UserBrief, UserDto,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateUserRequest {
    pub is_admin: bool,
    pub access: UserAccess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InboxReadRequest {
    pub thread_id: Option<ThreadId>,
    pub all: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AuthStatusResponse {
    pub status: AuthStatusDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub path: String,
    pub name: Option<String>,
    /// Let the server create the directory and initialize a repository when it
    /// is missing, empty, or a repository without commits.
    #[serde(default)]
    pub create: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SubagentLogEntryDto {
    /// "task", "parent", "agent", "tool", "notice" or "error".
    pub role: String,
    pub text: String,
    /// Milliseconds since the epoch; 0 when the adapter left no timestamp.
    pub ts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SubagentLogResponse {
    pub entries: Vec<SubagentLogEntryDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadGoalResponse {
    /// The goal the adapter keeps in this thread's workspace, if any.
    pub goal: Option<GoalDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub remote_url: Option<String>,
    pub path: Option<String>,
    /// Empty string clears the configured command.
    pub verify_command: Option<String>,
    /// Empty string clears the configured command.
    pub setup_command: Option<String>,
    /// Default mode for new threads in this project. Empty string clears it back
    /// to inheriting the instance default, matching the two fields above.
    pub permission_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadWatchRequest {
    /// Target relationship. Declarative, so retries and double-clicks are
    /// idempotent and callers never have to order two booleans correctly.
    pub state: crate::model::WatchState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadWatchResponse {
    pub state: crate::model::WatchState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchHitDto {
    pub thread_id: ThreadId,
    pub thread_title: String,
    pub project_id: ProjectId,
    pub project_name: String,
    /// `thread`, `message` or `comment`.
    pub kind: String,
    pub ref_id: String,
    pub snippet: String,
    pub ts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchResponse {
    pub hits: Vec<SearchHitDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PushConfigResponse {
    pub enabled: bool,
    pub public_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PushSubscriptionKeys {
    pub p256dh: String,
    pub auth: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PushSubscriptionRequest {
    pub endpoint: String,
    pub keys: PushSubscriptionKeys,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PushUnsubscribeRequest {
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateThreadRequest {
    pub title: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub base_branch: Option<String>,
    pub initial_prompt: Option<String>,
    /// Give the thread its own git worktree and branch (default true). When
    /// false the agent works directly in the project directory.
    pub worktree: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateThreadRequest {
    pub title: Option<String>,
    pub pinned: Option<bool>,
    pub archived: Option<bool>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub auto_settle: Option<AutoSettle>,
    /// This thread's own choice. Empty string goes back to inheriting the
    /// project (and then the instance) default; `None` leaves it alone.
    pub permission_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForkThreadRequest {
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuestMessageDto {
    pub id: String,
    pub text: String,
    pub status: GuestMessageStatus,
    pub created_at: i64,
}

/// What a guest may see and do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuestThreadResponse {
    pub title: String,
    pub agent: String,
    pub model: Option<String>,
    /// When this link stops working, in Unix ms.
    pub expires_at: i64,
    /// Transcript window, oldest first. Poll with `after` to continue.
    pub page: EventsPageDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuestLinkResponse {
    pub url: String,
    pub expires_at: i64,
}

/// Whether a thread currently has a link, so the menu can offer to stop sharing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuestLinkStatus {
    pub active: bool,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct McpTokenDto {
    pub id: String,
    pub label: String,
    pub scopes: Vec<McpScope>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct McpTokensResponse {
    pub tokens: Vec<McpTokenDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateMcpTokenRequest {
    pub label: String,
    pub scopes: Vec<McpScope>,
}

/// Returned once: the token itself is never readable again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreatedMcpToken {
    pub token: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuestSessionRequest {
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuestMessageRequest {
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GuestMessageAction {
    Send,
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuestMessageDecision {
    pub action: GuestMessageAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GuestMessagesResponse {
    pub messages: Vec<GuestMessageDto>,
}

/// Discard one turn and everything after it, files included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RewindRequest {
    pub message_id: String,
}

/// Move a thread between shelves.
///
/// `action` is a command id from the published `placement` machine
/// (`settle`, `unsettle`, `snooze`, `wake`). Keeping it a string means the
/// machine stays the only list of legal commands; a second enum here would be
/// a second rule set to forget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlacementRequest {
    pub action: String,
    /// Unix ms deadline for `snooze`. Ignored by other commands; omitted means a
    /// day from now, resolved by the server so clients never disagree on it.
    pub until: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DeliveryMode {
    Auto,
    Inject,
    StopAndSend,
    Queue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub text: String,
    pub delivery: Option<DeliveryMode>,
    /// Consume all trailing CRDT draft blocks instead of using `text`.
    pub from_draft: Option<bool>,
    /// Uploaded images to send with this message, in display order.
    pub attachment_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SendMessageResponse {
    pub delivery: Delivery,
    pub queue_position: Option<u32>,
    pub message_id: Option<String>,
}

/// Replace a user prompt and rewind the thread to it. Everything after the
/// edited message is discarded and the agent starts a fresh ACP session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EditMessageRequest {
    pub text: String,
}

/// Rewrite a message that is still waiting in the thread's input queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateQueuedInputRequest {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRequest {
    pub decision: ApprovalDecision,
    pub note: Option<String>,
    /// Exact adapter option the user picked. Two `allow_once` options can be
    /// different answers (for example "enabled" vs "disabled"), so the
    /// decision kind alone cannot identify the choice.
    #[serde(default)]
    pub option_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ElicitationResponseRequest {
    pub action: ElicitationAction,
    #[ts(type = "unknown")]
    pub values: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommentRequest {
    pub body: String,
    pub anchor: CommentAnchor,
    pub parent_id: Option<CommentId>,
    pub send: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCommentRequest {
    pub body: Option<String>,
    pub status: Option<CommentStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SeenRequest {
    pub paths: Vec<String>,
    pub seen: bool,
    /// Baseline the Seen marks apply to. Defaults to the base branch.
    pub baseline: Option<BaselineDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MergeBackRequest {
    pub commit_message: Option<String>,
    pub commit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MergeBackResponse {
    pub committed: bool,
    pub commit: Option<String>,
    pub files: Vec<String>,
    /// Branch the patch was applied to.
    pub branch: String,
    /// The base branch advanced since this thread started.
    pub base_drift: bool,
    /// A project verify command ran successfully before the patch was applied.
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SettingsPatch {
    pub theme: Option<String>,
    pub text_width: Option<String>,
    pub subthreads_per_thread: Option<u32>,
    pub send_with_modifier: Option<bool>,
    pub default_agent: Option<String>,
    pub default_model: Option<String>,
    pub default_baseline: Option<crate::model::BaselineKind>,
    pub hidden_presence: Option<bool>,
    pub webhook_url: Option<String>,
    pub browser_notifications: Option<bool>,
}

/// One earlier slice of the thread log, used when the transcript scrolls past
/// the window a cold load returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OlderEventsDto {
    pub events: Vec<EventEnvelope>,
    pub oldest_seq: u64,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EventsPageDto {
    pub thread_id: ThreadId,
    pub events: Vec<EventEnvelope>,
    pub last_seq: u64,
    pub oldest_seq: u64,
    /// Whether the stored log has events older than `oldest_seq`. A rewound
    /// thread starts mid-sequence with no earlier branch, so a non-zero
    /// `oldest_seq` is not enough to decide whether "load earlier" applies.
    pub has_earlier: bool,
    pub resync: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadBundleDto {
    pub thread: ThreadDto,
    pub project: ProjectDto,
    /// Effective role of the caller in this thread; drives read-only UI.
    pub role: Role,
    pub subagents: Vec<SubagentDto>,
    pub approvals: Vec<ApprovalDto>,
    pub elicitations: Vec<ElicitationDto>,
    pub queue: Vec<QueueItemDto>,
    pub comments: Vec<CommentDto>,
    pub changes: ChangesDto,
}

/// Single read model shared by the web UI and the agent summary.
///
/// Derived from the authoritative state store; see `docs/08` §3.3.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadState {
    pub thread_id: ThreadId,
    /// Monotonic revision for this thread; consumers must discard older values.
    pub revision: u64,
    pub status: crate::model::ThreadStatus,
    /// Commands legal from this state, from `ThreadMachine`. The client renders
    /// controls from this list instead of re-deriving rules from `status`, so a
    /// button and the endpoint it calls can never disagree.
    pub allowed_actions: Vec<String>,
    pub current_turn: Option<String>,
    pub pending_approvals: u32,
    pub pending_elicitations: u32,
    pub queued_inputs: u32,
    pub active_subagents: u32,
    /// Where the thread sits between turns, and the row it renders in. Both are
    /// server-owned: the sidebar never derives them from status strings.
    pub placement: crate::model::ThreadPlacement,
    pub section: crate::model::ThreadSection,
    pub snooze_until: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InboxPageDto {
    pub items: Vec<InboxItemDto>,
    /// Total unread items for this user, not just the ones on this page.
    pub unread: u32,
    /// Another page exists before the last item.
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FilesResponseDto {
    pub path: String,
    pub entries: Vec<FileEntryDto>,
    /// The directory held more entries than the server returns in one page.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileContentResponseDto {
    pub file: FileContentDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OkResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ClientWsMessage {
    Subscribe {
        thread_id: ThreadId,
        after_seq: u64,
    },
    Unsubscribe {
        thread_id: ThreadId,
    },
    Presence {
        project_id: ProjectId,
        target: Option<crate::focus::Anchor>,
        thread_id: Option<ThreadId>,
        panel: crate::model::Panel,
        typing: bool,
        hidden: bool,
    },
    Ping,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ServerWsMessage {
    Event {
        event: EventEnvelope,
    },
    EventsPage {
        page: EventsPageDto,
    },
    Presence {
        project_id: ProjectId,
        focuses: Vec<crate::focus::Focus>,
        online_user_ids: Vec<crate::ids::UserId>,
    },
    ThreadState {
        state: ThreadState,
    },
    /// The stored event tail was discarded and replaced. Clients drop their
    /// local log and re-subscribe from the beginning instead of leaving rows
    /// from the removed branch on screen.
    ThreadRewound {
        thread_id: ThreadId,
    },
    InboxCount {
        user_id: crate::ids::UserId,
        unread: u32,
    },
    Notice {
        level: String,
        message: String,
    },
    Pong,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[ts(type = "unknown")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UserListResponse {
    pub users: Vec<UserDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApprovalResolvedResponse {
    pub approval: ApprovalDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StateResponse {
    pub settings: SettingsDto,
    pub user: UserDto,
    /// The instance's permission defaults and ceiling. The picker renders every
    /// mode above `cap` as managed, instead of offering a choice the server
    /// would clamp away.
    pub permissions: PermissionPolicyDto,
    /// The browser terminal is opt-in per server; the UI hides its entry points
    /// when this is false.
    pub terminal: bool,
    /// Host tmux sessions are a second opt-in. When true, every member may
    /// list and attach to the configured tmux servers outside the private
    /// socket.
    pub terminal_host: bool,
    /// Guest links are opt-in; with them off the sharing entry points are hidden
    /// and their endpoints do not exist.
    pub guest_links: bool,
    /// The MCP endpoint is opt-in; with it off outside agents cannot drive
    /// threads and the settings section is hidden.
    pub mcp: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PermissionPolicyDto {
    pub default: PermissionMode,
    pub cap: PermissionMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRuleDto {
    pub id: String,
    pub project_id: ProjectId,
    pub thread_id: Option<ThreadId>,
    pub agent: String,
    pub tool: String,
    pub command_prefix: Option<String>,
    pub scope: crate::model::ApprovalScope,
    pub created_by: UserBrief,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRulesResponse {
    pub rules: Vec<ApprovalRuleDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub ok: bool,
    pub version: String,
    pub needs_admin: bool,
    pub bind: String,
    /// A built binary is staged and will be installed at the next idle window.
    #[serde(default)]
    pub deploy_staged: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ElicitationResolvedResponse {
    pub elicitation: ElicitationDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CommentResponse {
    pub comment: CommentDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommentsResponse {
    pub comments: Vec<CommentDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectResponse {
    pub project: ProjectDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectsResponse {
    pub projects: Vec<ProjectDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadResponse {
    pub thread: ThreadDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadBundleResponse {
    pub bundle: ThreadBundleDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadHistoryResponse {
    pub history: ThreadHistoryDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadsResponse {
    pub threads: Vec<ThreadDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub error: crate::api::ApiErrorBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileSearchResponse {
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillDto {
    pub name: String,
    pub description: String,
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillsResponse {
    pub skills: Vec<SkillDto>,
}

/// A shared shell. `id` is the tmux session name, which is also the URL segment
/// used by the terminal socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerminalSessionDto {
    pub id: String,
    pub thread_id: Option<ThreadId>,
    /// Live working directory as tmux reports it, not the spawn directory.
    pub cwd: String,
    pub alive: bool,
    /// The user currently holding the keyboard, when somebody is attached.
    pub controller: Option<UserId>,
    pub viewers: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerminalSessionResponse {
    pub session: TerminalSessionDto,
    /// Single-use ticket for the WebSocket upgrade; the browser cannot send
    /// custom headers on a WebSocket, so the POST mints the credential.
    pub ticket: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerminalSessionsResponse {
    pub sessions: Vec<TerminalSessionDto>,
}

/// One session on a tmux server outside the private workbench socket.
/// `id` is tmux's stable `$N` session id, which is also the only safe target
/// for `attach`: names are free-form and can be parsed as target syntax.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HostTerminalSessionDto {
    pub id: String,
    pub name: String,
    pub windows: u32,
    /// Clients attached to the tmux session, including local terminals.
    pub attached: u32,
    pub created_at: i64,
    pub activity_at: i64,
    pub cwd: Option<String>,
    pub command: Option<String>,
    /// Browsers currently attached through threadbase.
    pub viewers: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HostTerminalServerDto {
    /// The `-L` socket name; the default server is `default`.
    pub socket: String,
    pub sessions: Vec<HostTerminalSessionDto>,
    /// A broken or unreachable socket does not hide the others.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HostTerminalSessionsResponse {
    pub servers: Vec<HostTerminalServerDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HostTerminalAttachResponse {
    pub session: HostTerminalSessionDto,
    /// Single-use ticket for `/ws/tmux`; it carries the socket and `$N` target.
    pub ticket: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateTerminalSessionRequest {
    /// `None` opens a shell in the home directory instead of a thread worktree.
    pub thread_id: Option<ThreadId>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    /// Screens that cannot type (phones) ask for a watcher-only ticket.
    pub watch_only: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AttachTerminalSessionRequest {
    pub watch_only: Option<bool>,
}
