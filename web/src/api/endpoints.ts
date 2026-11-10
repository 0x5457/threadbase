import type { AgentConfigResponse } from "@/api/types/AgentConfigResponse";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { ApprovalResolvedResponse } from "@/api/types/ApprovalResolvedResponse";
import type { ApprovalRulesResponse } from "@/api/types/ApprovalRulesResponse";
import type { AuthMeDto } from "@/api/types/AuthMeDto";
import type { AuthStatusResponse } from "@/api/types/AuthStatusResponse";
import type { AddMemberRequest } from "@/api/types/AddMemberRequest";
import type { ChangesDto } from "@/api/types/ChangesDto";
import type { DirectoryListingDto } from "@/api/types/DirectoryListingDto";
import type { CommentResponse } from "@/api/types/CommentResponse";
import type { CommentsResponse } from "@/api/types/CommentsResponse";
import type { CreateCommentRequest } from "@/api/types/CreateCommentRequest";
import type { CreateInviteRequest } from "@/api/types/CreateInviteRequest";
import type { CreateProjectRequest } from "@/api/types/CreateProjectRequest";
import type { ElicitationAction } from "@/api/types/ElicitationAction";
import type { ElicitationResponseRequest } from "@/api/types/ElicitationResponseRequest";
import type { ElicitationResolvedResponse } from "@/api/types/ElicitationResolvedResponse";
import type { FileContentResponseDto } from "@/api/types/FileContentResponseDto";
import type { FilesResponseDto } from "@/api/types/FilesResponseDto";
import type { FileDiffDto } from "@/api/types/FileDiffDto";
import type { InviteResponse } from "@/api/types/InviteResponse";
import type { MergeBackRequest } from "@/api/types/MergeBackRequest";
import type { MergeBackResponse } from "@/api/types/MergeBackResponse";
import type { QueueResponse } from "@/api/types/QueueResponse";
import type { Role } from "@/api/types/Role";
import type { SeenRequest } from "@/api/types/SeenRequest";
import type { UpdateCommentRequest } from "@/api/types/UpdateCommentRequest";
import type { UpdateThreadRequest } from "@/api/types/UpdateThreadRequest";
import type { UpdateProjectRequest } from "@/api/types/UpdateProjectRequest";
import type { CreateThreadRequest } from "@/api/types/CreateThreadRequest";
import type { DecisionRequest } from "@/api/types/DecisionRequest";
import type { EventsPageDto } from "@/api/types/EventsPageDto";
import type { InboxPageDto } from "@/api/types/InboxPageDto";
import type { LoginRequest } from "@/api/types/LoginRequest";
import type { LoginResponse } from "@/api/types/LoginResponse";
import type { OkResponse } from "@/api/types/OkResponse";
import type { ProjectId } from "@/api/types/ProjectId";
import type { ProjectResponse } from "@/api/types/ProjectResponse";
import type { ProjectOverviewResponse } from "@/api/types/ProjectOverviewResponse";
import type { ProjectsResponse } from "@/api/types/ProjectsResponse";
import type { SearchResponseDto } from "@/api/types/SearchResponseDto";
import type { SettingsPatch } from "@/api/types/SettingsPatch";
import type { SendMessageRequest } from "@/api/types/SendMessageRequest";
import type { SendMessageResponse } from "@/api/types/SendMessageResponse";
import type { SetupRequest } from "@/api/types/SetupRequest";
import type { StateResponse } from "@/api/types/StateResponse";
import type { ThreadBundleResponse } from "@/api/types/ThreadBundleResponse";
import type { ThreadState } from "@/api/types/ThreadState";
import type { ThreadId } from "@/api/types/ThreadId";
import type { ThreadResponse } from "@/api/types/ThreadResponse";
import type { ThreadsResponse } from "@/api/types/ThreadsResponse";
import { api } from "@/lib/api";

export type InboxFilter = "unread" | "all";

function segment(value: string): string {
  return encodeURIComponent(value);
}

export const endpoints = {
  authStatus: "/api/auth/status",
  authSetup: "/api/auth/setup",
  authLogin: "/api/auth/login",
  authLogout: "/api/auth/logout",
  authMe: "/api/auth/me",
  projects: "/api/projects",
  project: (projectId: ProjectId) => `/api/projects/${segment(projectId)}`,
  projectOverview: (projectId: ProjectId) =>
    `/api/projects/${segment(projectId)}/overview`,
  projectThreads: (projectId: ProjectId, includeArchived = false) =>
    `/api/projects/${segment(projectId)}/threads${includeArchived ? "?include_archived=true" : ""}`,
  threads: "/api/threads",
  threadBundle: (threadId: ThreadId) =>
    `/api/threads/${segment(threadId)}/bundle`,
  threadState: (threadId: ThreadId) =>
    `/api/threads/${segment(threadId)}/state`,
  sendMessage: (threadId: ThreadId) =>
    `/api/threads/${segment(threadId)}/messages`,
  markRead: (threadId: ThreadId) => `/api/threads/${segment(threadId)}/read`,
  restartAgent: (threadId: ThreadId) =>
    `/api/threads/${segment(threadId)}/restart`,
  cancelTurn: (threadId: ThreadId) => `/api/threads/${segment(threadId)}/cancel`,
  forkThread: (threadId: ThreadId) =>
    `/api/threads/${segment(threadId)}/fork`,
  agentConfig: (threadId: ThreadId) =>
    `/api/threads/${segment(threadId)}/agent-config`,
  directories: (path: string) =>
    `/api/fs/directories?path=${encodeURIComponent(path)}`,
  cancelSubagent: (threadId: ThreadId, subagentId: string) =>
    `/api/threads/${segment(threadId)}/subagents/${segment(subagentId)}/cancel`,
  threadChanges: (threadId: ThreadId, kind = "base_branch", commit = "") =>
    `/api/threads/${segment(threadId)}/changes?kind=${encodeURIComponent(kind)}${commit ? `&commit=${encodeURIComponent(commit)}` : ""}`,
  threadFileDiff: (
    threadId: ThreadId,
    path: string,
    kind = "base_branch",
    commit = "",
  ) =>
    `/api/threads/${segment(threadId)}/changes/file?kind=${encodeURIComponent(kind)}&path=${encodeURIComponent(path)}${commit ? `&commit=${encodeURIComponent(commit)}` : ""}`,
  markSeen: (threadId: ThreadId) => `/api/threads/${segment(threadId)}/seen`,
  mergeBack: (threadId: ThreadId) =>
    `/api/threads/${segment(threadId)}/merge-back`,
  comments: (threadId: ThreadId) => `/api/threads/${segment(threadId)}/comments`,
  files: (threadId: ThreadId, path: string) =>
    `/api/threads/${segment(threadId)}/files?path=${encodeURIComponent(path)}`,
  file: (threadId: ThreadId, path: string) =>
    `/api/threads/${segment(threadId)}/file?path=${encodeURIComponent(path)}`,
  comment: (commentId: string) => `/api/comments/${segment(commentId)}`,
  commentSend: (commentId: string) =>
    `/api/comments/${segment(commentId)}/send`,
  queue: (threadId: ThreadId) => `/api/threads/${segment(threadId)}/queue`,
  queueItem: (threadId: ThreadId, itemId: string) =>
    `/api/threads/${segment(threadId)}/queue/${segment(itemId)}`,
  queueRetry: (threadId: ThreadId, itemId: string) =>
    `/api/threads/${segment(threadId)}/queue/${segment(itemId)}/retry`,
  members: (threadId: ThreadId) =>
    `/api/threads/${segment(threadId)}/members`,
  member: (threadId: ThreadId, userId: string) =>
    `/api/threads/${segment(threadId)}/members/${segment(userId)}`,
  invites: (threadId: ThreadId) =>
    `/api/threads/${segment(threadId)}/invites`,
  invite: (threadId: ThreadId, inviteId: string) =>
    `/api/threads/${segment(threadId)}/invites/${segment(inviteId)}`,
  share: (threadId: ThreadId) => `/api/threads/${segment(threadId)}/share`,
  threadEvents: (threadId: ThreadId, afterSeq = 0, limit = 200) =>
    `/api/threads/${segment(threadId)}/events?after_seq=${afterSeq}&limit=${limit}`,
  approvalDecision: (threadId: ThreadId, approvalId: string) =>
    `/api/threads/${segment(threadId)}/approvals/${segment(approvalId)}/decision`,
  approvalRules: "/api/approval-rules",
  approvalRule: (ruleId: string) =>
    `/api/approval-rules/${segment(ruleId)}`,
  elicitation: (elicitationId: string) =>
    `/api/elicitations/${segment(elicitationId)}/respond`,
  inbox: (filter: InboxFilter) => `/api/inbox?filter=${filter}`,
  search: (query: string) => `/api/search?q=${segment(query)}`,
  state: "/api/state",
  settings: "/api/settings",
} as const;

export function fetchAuthStatus(): Promise<AuthStatusResponse> {
  return api<AuthStatusResponse>(endpoints.authStatus);
}

export function setupAdmin(request: SetupRequest): Promise<LoginResponse> {
  return api<LoginResponse>(endpoints.authSetup, {
    method: "POST",
    body: request,
  });
}

export function login(request: LoginRequest): Promise<LoginResponse> {
  return api<LoginResponse>(endpoints.authLogin, {
    method: "POST",
    body: request,
  });
}

export function logout(): Promise<OkResponse | void> {
  return api<OkResponse | void>(endpoints.authLogout, { method: "POST" });
}

export function fetchMe(): Promise<AuthMeDto> {
  return api<AuthMeDto>(endpoints.authMe);
}

export function fetchProjects(): Promise<ProjectsResponse> {
  return api<ProjectsResponse>(endpoints.projects);
}

export function createProject(
  request: CreateProjectRequest,
): Promise<ProjectResponse> {
  return api<ProjectResponse>(endpoints.projects, {
    method: "POST",
    body: request,
  });
}

export function fetchProject(projectId: ProjectId): Promise<ProjectResponse> {
  return api<ProjectResponse>(endpoints.project(projectId));
}

export function fetchProjectOverview(
  projectId: ProjectId,
): Promise<ProjectOverviewResponse> {
  return api<ProjectOverviewResponse>(endpoints.projectOverview(projectId));
}

export function fetchProjectThreads(
  projectId: ProjectId,
  includeArchived = false,
): Promise<ThreadsResponse> {
  return api<ThreadsResponse>(
    endpoints.projectThreads(projectId, includeArchived),
  );
}

export function updateThread(
  threadId: ThreadId,
  request: UpdateThreadRequest,
): Promise<ThreadResponse> {
  return api<ThreadResponse>(`/api/threads/${segment(threadId)}`, {
    method: "PATCH",
    body: request,
  });
}

export function createThread(
  projectId: ProjectId,
  request: CreateThreadRequest,
): Promise<ThreadResponse> {
  return api<ThreadResponse>(endpoints.projectThreads(projectId), {
    method: "POST",
    body: request,
  });
}

export function fetchWorkspaceThreads(): Promise<ThreadsResponse> {
  return api<ThreadsResponse>(endpoints.threads);
}

export function fetchAgentConfig(
  threadId: ThreadId,
): Promise<AgentConfigResponse> {
  return api<AgentConfigResponse>(endpoints.agentConfig(threadId));
}

export function fetchDirectories(path: string): Promise<DirectoryListingDto> {
  return api<DirectoryListingDto>(endpoints.directories(path));
}

export function forkThread(
  threadId: ThreadId,
  title: string | null = null,
): Promise<ThreadResponse> {
  return api<ThreadResponse>(endpoints.forkThread(threadId), {
    method: "POST",
    body: { title },
  });
}

export function requestAccess(threadId: ThreadId): Promise<OkResponse> {
  return api<OkResponse>(`/api/threads/${segment(threadId)}/request-access`, {
    method: "POST",
  });
}

export function deleteThread(
  threadId: ThreadId,
  confirm: boolean,
): Promise<OkResponse> {
  return api<OkResponse>(
    `/api/threads/${segment(threadId)}?confirm=${confirm ? "true" : "false"}`,
    { method: "DELETE" },
  );
}

export function cancelTurn(threadId: ThreadId): Promise<OkResponse> {
  return api<OkResponse>(endpoints.cancelTurn(threadId), { method: "POST" });
}

export function restartAgent(threadId: ThreadId): Promise<OkResponse> {
  return api<OkResponse>(endpoints.restartAgent(threadId), { method: "POST" });
}

export function markThreadRead(threadId: ThreadId): Promise<OkResponse> {
  return api<OkResponse>(endpoints.markRead(threadId), { method: "POST" });
}

export function cancelSubagent(
  threadId: ThreadId,
  subagentId: string,
): Promise<OkResponse> {
  return api<OkResponse>(endpoints.cancelSubagent(threadId, subagentId), {
    method: "POST",
  });
}

export function sendMessage(
  threadId: ThreadId,
  request: SendMessageRequest,
): Promise<SendMessageResponse> {
  return api<SendMessageResponse>(endpoints.sendMessage(threadId), {
    method: "POST",
    body: request,
  });
}

export function fetchThreadBundle(
  threadId: ThreadId,
): Promise<ThreadBundleResponse> {
  return api<ThreadBundleResponse>(endpoints.threadBundle(threadId));
}

export function fetchThreadState(threadId: ThreadId): Promise<ThreadState> {
  return api<ThreadState>(endpoints.threadState(threadId));
}

export function fetchApprovalRules(): Promise<ApprovalRulesResponse> {
  return api<ApprovalRulesResponse>(endpoints.approvalRules);
}

export function deleteApprovalRule(ruleId: string): Promise<OkResponse> {
  return api<OkResponse>(endpoints.approvalRule(ruleId), { method: "DELETE" });
}

export function respondElicitation(
  elicitationId: string,
  action: ElicitationAction,
  values: unknown,
): Promise<ElicitationResolvedResponse> {
  const body: ElicitationResponseRequest = { action, values };
  return api<ElicitationResolvedResponse>(endpoints.elicitation(elicitationId), {
    method: "POST",
    body,
  });
}

export function fetchChanges(
  threadId: ThreadId,
  kind = "base_branch",
  commit = "",
): Promise<ChangesDto> {
  return api<ChangesDto>(endpoints.threadChanges(threadId, kind, commit));
}

export function fetchFileDiff(
  threadId: ThreadId,
  path: string,
  kind = "base_branch",
  commit = "",
): Promise<FileDiffDto> {
  return api<FileDiffDto>(
    endpoints.threadFileDiff(threadId, path, kind, commit),
  );
}

export function setSeen(
  threadId: ThreadId,
  paths: string[],
  seen: boolean,
): Promise<OkResponse> {
  const body: SeenRequest = { paths, seen };
  return api<OkResponse>(endpoints.markSeen(threadId), {
    method: "POST",
    body,
  });
}

export function revertThread(threadId: ThreadId): Promise<OkResponse> {
  return api<OkResponse>(`/api/threads/${segment(threadId)}/revert`, {
    method: "POST",
  });
}

export function mergeBackChanges(
  threadId: ThreadId,
  commit: boolean,
  commitMessage: string | null,
): Promise<MergeBackResponse> {
  const body: MergeBackRequest = { commit, commitMessage };
  return api<MergeBackResponse>(endpoints.mergeBack(threadId), {
    method: "POST",
    body,
  });
}

export function fetchFiles(
  threadId: ThreadId,
  path = "",
): Promise<FilesResponseDto> {
  return api<FilesResponseDto>(endpoints.files(threadId, path));
}

export function fetchFile(
  threadId: ThreadId,
  path: string,
): Promise<FileContentResponseDto> {
  return api<FileContentResponseDto>(endpoints.file(threadId, path));
}

export function fetchComments(threadId: ThreadId): Promise<CommentsResponse> {
  return api<CommentsResponse>(endpoints.comments(threadId));
}

export function createComment(
  threadId: ThreadId,
  request: CreateCommentRequest,
): Promise<CommentResponse> {
  return api<CommentResponse>(endpoints.comments(threadId), {
    method: "POST",
    body: request,
  });
}

export function updateComment(
  commentId: string,
  request: UpdateCommentRequest,
): Promise<CommentResponse> {
  return api<CommentResponse>(endpoints.comment(commentId), {
    method: "PATCH",
    body: request,
  });
}

export function deleteComment(commentId: string): Promise<OkResponse> {
  return api<OkResponse>(endpoints.comment(commentId), { method: "DELETE" });
}

export function sendComment(commentId: string): Promise<CommentResponse> {
  return api<CommentResponse>(endpoints.commentSend(commentId), {
    method: "POST",
  });
}

export function fetchQueue(threadId: ThreadId): Promise<QueueResponse> {
  return api<QueueResponse>(endpoints.queue(threadId));
}

export function retryQueueItem(
  threadId: ThreadId,
  itemId: string,
): Promise<OkResponse> {
  return api<OkResponse>(endpoints.queueRetry(threadId, itemId), {
    method: "POST",
  });
}

export function dropQueueItem(
  threadId: ThreadId,
  itemId: string,
): Promise<OkResponse> {
  return api<OkResponse>(endpoints.queueItem(threadId, itemId), {
    method: "DELETE",
  });
}

export function addMember(
  threadId: ThreadId,
  request: AddMemberRequest,
): Promise<unknown> {
  return api(endpoints.members(threadId), { method: "POST", body: request });
}

export function removeMember(
  threadId: ThreadId,
  userId: string,
): Promise<OkResponse> {
  return api<OkResponse>(endpoints.member(threadId, userId), {
    method: "DELETE",
  });
}

export function updateMemberRole(
  threadId: ThreadId,
  userId: string,
  role: Role,
): Promise<unknown> {
  return api(endpoints.member(threadId, userId), {
    method: "PATCH",
    body: { role },
  });
}

export function createInvite(
  threadId: ThreadId,
  request: CreateInviteRequest,
): Promise<InviteResponse> {
  return api<InviteResponse>(endpoints.invites(threadId), {
    method: "POST",
    body: request,
  });
}

export function revokeInvite(
  threadId: ThreadId,
  inviteId: string,
): Promise<OkResponse> {
  return api<OkResponse>(endpoints.invite(threadId, inviteId), {
    method: "DELETE",
  });
}

export function setShareLevel(
  threadId: ThreadId,
  shareLevel: "invite_only" | "workspace",
): Promise<OkResponse> {
  return api<OkResponse>(endpoints.share(threadId), {
    method: "PATCH",
    body: { shareLevel },
  });
}

export function fetchThreadEvents(
  threadId: ThreadId,
): Promise<EventsPageDto> {
  return api<EventsPageDto>(endpoints.threadEvents(threadId));
}

export function fetchInbox(filter: InboxFilter): Promise<InboxPageDto> {
  return api<InboxPageDto>(endpoints.inbox(filter));
}

export function searchWorkspace(query: string): Promise<SearchResponseDto> {
  return api<SearchResponseDto>(endpoints.search(query));
}

export function fetchState(): Promise<StateResponse> {
  return api<StateResponse>(endpoints.state);
}

export function updateSettings(patch: SettingsPatch): Promise<StateResponse> {
  return api<StateResponse>(endpoints.settings, {
    method: "PATCH",
    body: patch,
  });
}

export function decideApproval(
  threadId: ThreadId,
  approvalId: string,
  decision: ApprovalDecision,
  note: string | null = null,
): Promise<ApprovalResolvedResponse> {
  const request: DecisionRequest = { decision, note };
  return api<ApprovalResolvedResponse>(
    endpoints.approvalDecision(threadId, approvalId),
    { method: "POST", body: request },
  );
}

export function updateProject(
  projectId: string,
  request: UpdateProjectRequest,
): Promise<ProjectResponse> {
  return api<ProjectResponse>(`/api/projects/${segment(projectId)}`, {
    method: "PATCH",
    body: request,
  });
}
