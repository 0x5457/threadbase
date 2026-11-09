//! API level request and response envelopes.

use crate::event::EventEnvelope;
use crate::ids::{ApprovalId, CommentId, ElicitationId, ProjectId, QueueItemId, ThreadId};
use crate::model::{
    ApprovalDecision, ApprovalDto, AuthStatusDto, BaselineDto, ChangeStats, ChangesDto,
    CommentAnchor, CommentDto, CommentStatus, Delivery, DoctorReport, ElicitationAction,
    ElicitationDto, FileChangeDto, FileContentDto, FileEntryDto, InboxItemDto, InviteDto,
    ProjectDto, ProjectOverviewDto, QueueItemDto, Role, SearchResultDto, SettingsDto, ShareLevel,
    ShareStateDto, SubagentDto, ThreadDto, ThreadMemberDto, UserBrief, UserDto,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateUserRequest {
    pub is_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub display_name: String,
    pub is_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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
#[ts(export)]
pub struct LoginResponse {
    pub user: UserDto,
    pub needs_setup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateProjectRequest {
    pub path: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub remote_url: Option<String>,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateThreadRequest {
    pub title: Option<String>,
    pub pinned: Option<bool>,
    pub archived: Option<bool>,
    pub share_level: Option<ShareLevel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ForkThreadRequest {
    pub title: Option<String>,
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
#[ts(export)]
pub struct SendMessageRequest {
    pub text: String,
    pub delivery: Option<DeliveryMode>,
    /// Consume all trailing CRDT draft blocks instead of using `text`.
    pub from_draft: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SendMessageResponse {
    pub delivery: Delivery,
    pub queue_position: Option<u32>,
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DecisionRequest {
    pub decision: ApprovalDecision,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ElicitationResponseRequest {
    pub action: ElicitationAction,
    #[ts(type = "unknown")]
    pub values: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateCommentRequest {
    pub body: String,
    pub anchor: CommentAnchor,
    pub parent_id: Option<CommentId>,
    pub send: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateCommentRequest {
    pub body: Option<String>,
    pub status: Option<CommentStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AddMemberRequest {
    pub username: String,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateShareRequest {
    pub share_level: ShareLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateMemberRequest {
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CreateInviteRequest {
    pub email: Option<String>,
    pub username: Option<String>,
    pub role: Option<Role>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InviteResponse {
    pub invite: InviteDto,
    /// Returned only once, when the invite is created.
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SeenRequest {
    pub paths: Vec<String>,
    pub seen: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QueueMutationRequest {
    pub item_id: QueueItemId,
    pub before_id: Option<QueueItemId>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EventsPageDto {
    pub thread_id: ThreadId,
    pub events: Vec<EventEnvelope>,
    pub last_seq: u64,
    pub oldest_seq: u64,
    pub resync: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadBundleDto {
    pub thread: ThreadDto,
    pub project: ProjectDto,
    pub subagents: Vec<SubagentDto>,
    pub approvals: Vec<ApprovalDto>,
    pub elicitations: Vec<ElicitationDto>,
    pub queue: Vec<QueueItemDto>,
    pub comments: Vec<CommentDto>,
    pub changes: ChangesDto,
    pub members: Vec<ThreadMemberDto>,
    pub share: ShareStateDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApprovalsPageDto {
    pub approvals: Vec<ApprovalDto>,
    pub elicitations: Vec<ElicitationDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InboxPageDto {
    pub items: Vec<InboxItemDto>,
    pub unread: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchResponseDto {
    pub results: Vec<SearchResultDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FilesResponseDto {
    pub path: String,
    pub entries: Vec<FileEntryDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangesListResponseDto {
    pub changes: ChangesDto,
    pub files: Vec<FileChangeDto>,
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
pub struct BaselineQuery {
    pub kind: Option<String>,
    pub commit: Option<String>,
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
#[ts(export)]
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
#[ts(export)]
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
        thread_id: ThreadId,
        status: crate::model::ThreadStatus,
        pending_approvals: u32,
        queued_inputs: u32,
    },
    InboxCount {
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
#[ts(export)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[ts(type = "unknown")]
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DoctorResponse {
    pub report: DoctorReport,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BaselineResponse {
    pub baseline: BaselineDto,
    pub label: String,
    pub stats: ChangeStats,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
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
#[ts(export)]
pub struct ApprovalRulesResponse {
    pub rules: Vec<ApprovalRuleDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HealthResponse {
    pub ok: bool,
    pub version: String,
    pub needs_setup: bool,
    pub bind: String,
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
#[ts(export)]
pub struct CommentsResponse {
    pub comments: Vec<CommentDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MembersResponse {
    pub members: Vec<ThreadMemberDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SubagentsResponse {
    pub subagents: Vec<SubagentDto>,
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
pub struct ProjectOverviewResponse {
    pub overview: ProjectOverviewDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadResponse {
    pub thread: ThreadDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadBundleResponse {
    pub bundle: ThreadBundleDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ThreadsResponse {
    pub threads: Vec<ThreadDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileDiffResponse {
    pub diff: crate::model::FileDiffDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QueueResponse {
    pub items: Vec<QueueItemDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ErrorResponse {
    pub error: crate::api::ApiErrorBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApprovalIdPath {
    pub approval_id: ApprovalId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ElicitationIdPath {
    pub elicitation_id: ElicitationId,
}
