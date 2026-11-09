import { useQuery } from "@tanstack/react-query";
import {
  fetchAuthStatus,
  fetchInbox,
  fetchMe,
  fetchProject,
  fetchProjectThreads,
  fetchProjects,
  fetchState,
  fetchThreadBundle,
  fetchThreadEvents,
  fetchWorkspaceThreads,
  searchWorkspace,
  type InboxFilter,
} from "@/api/endpoints";

export const queryKeys = {
  authStatus: ["auth", "status"] as const,
  me: ["auth", "me"] as const,
  projects: ["projects"] as const,
  project: (projectId: string) => ["projects", projectId] as const,
  projectThreads: (projectId: string) =>
    ["projects", projectId, "threads"] as const,
  threads: ["threads"] as const,
  threadBundle: (threadId: string) => ["threads", threadId, "bundle"] as const,
  threadEvents: (threadId: string) => ["threads", threadId, "events"] as const,
  inbox: (filter: InboxFilter) => ["inbox", filter] as const,
  search: (query: string) => ["search", query] as const,
  state: ["state"] as const,
};

export function useAuthStatus() {
  return useQuery({
    queryKey: queryKeys.authStatus,
    queryFn: fetchAuthStatus,
    retry: false,
    staleTime: 10_000,
  });
}

export function useMe() {
  return useQuery({
    queryKey: queryKeys.me,
    queryFn: fetchMe,
    retry: false,
    staleTime: 30_000,
  });
}

export function useProjects() {
  return useQuery({
    queryKey: queryKeys.projects,
    queryFn: fetchProjects,
  });
}

export function useProject(projectId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.project(projectId ?? ""),
    queryFn: () => fetchProject(projectId ?? ""),
    enabled: Boolean(projectId),
  });
}

export function useProjectThreads(projectId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.projectThreads(projectId ?? ""),
    queryFn: () => fetchProjectThreads(projectId ?? ""),
    enabled: Boolean(projectId),
  });
}

export function useWorkspaceThreads() {
  return useQuery({
    queryKey: queryKeys.threads,
    queryFn: fetchWorkspaceThreads,
  });
}

export function useThreadBundle(threadId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.threadBundle(threadId ?? ""),
    queryFn: () => fetchThreadBundle(threadId ?? ""),
    enabled: Boolean(threadId),
  });
}

export function useThreadEvents(threadId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.threadEvents(threadId ?? ""),
    queryFn: () => fetchThreadEvents(threadId ?? ""),
    enabled: Boolean(threadId),
  });
}

export function useInbox(filter: InboxFilter) {
  return useQuery({
    queryKey: queryKeys.inbox(filter),
    queryFn: () => fetchInbox(filter),
  });
}

export function useWorkspaceSearch(query: string) {
  const trimmed = query.trim();
  return useQuery({
    queryKey: queryKeys.search(trimmed),
    queryFn: () => searchWorkspace(trimmed),
    enabled: trimmed.length > 0,
    retry: false,
  });
}

export function useWorkspaceState() {
  return useQuery({
    queryKey: queryKeys.state,
    queryFn: fetchState,
  });
}
