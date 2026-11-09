import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { ApprovalResolvedResponse } from "@/api/types/ApprovalResolvedResponse";
import type { AuthMeDto } from "@/api/types/AuthMeDto";
import type { AuthStatusResponse } from "@/api/types/AuthStatusResponse";
import type { CreateProjectRequest } from "@/api/types/CreateProjectRequest";
import type { CreateThreadRequest } from "@/api/types/CreateThreadRequest";
import type { DecisionRequest } from "@/api/types/DecisionRequest";
import type { EventsPageDto } from "@/api/types/EventsPageDto";
import type { InboxPageDto } from "@/api/types/InboxPageDto";
import type { LoginRequest } from "@/api/types/LoginRequest";
import type { LoginResponse } from "@/api/types/LoginResponse";
import type { OkResponse } from "@/api/types/OkResponse";
import type { ProjectId } from "@/api/types/ProjectId";
import type { ProjectResponse } from "@/api/types/ProjectResponse";
import type { ProjectsResponse } from "@/api/types/ProjectsResponse";
import type { SearchResponseDto } from "@/api/types/SearchResponseDto";
import type { SettingsPatch } from "@/api/types/SettingsPatch";
import type { SetupRequest } from "@/api/types/SetupRequest";
import type { StateResponse } from "@/api/types/StateResponse";
import type { ThreadBundleResponse } from "@/api/types/ThreadBundleResponse";
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
  projectThreads: (projectId: ProjectId) =>
    `/api/projects/${segment(projectId)}/threads`,
  threads: "/api/threads",
  threadBundle: (threadId: ThreadId) =>
    `/api/threads/${segment(threadId)}/bundle`,
  threadEvents: (threadId: ThreadId, afterSeq = 0, limit = 200) =>
    `/api/threads/${segment(threadId)}/events?after_seq=${afterSeq}&limit=${limit}`,
  approvalDecision: (threadId: ThreadId, approvalId: string) =>
    `/api/threads/${segment(threadId)}/approvals/${segment(approvalId)}/decision`,
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

export function fetchProjectThreads(
  projectId: ProjectId,
): Promise<ThreadsResponse> {
  return api<ThreadsResponse>(endpoints.projectThreads(projectId));
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

export function fetchThreadBundle(
  threadId: ThreadId,
): Promise<ThreadBundleResponse> {
  return api<ThreadBundleResponse>(endpoints.threadBundle(threadId));
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
