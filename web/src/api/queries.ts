import { useQuery } from "@tanstack/react-query";
import {
  fetchAgentConfig,
  fetchAuthStatus,
  fetchChanges,
  fetchComments,
  fetchDirectories,
  fetchFile,
  fetchFiles,
  fetchFileDiff,
  fetchInbox,
  fetchMe,
  fetchProject,
  fetchProjectOverview,
  fetchQueue,
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
  projectOverview: (projectId: string) =>
    ["projects", projectId, "overview"] as const,
  projectThreads: (projectId: string, includeArchived = false) =>
    ["projects", projectId, "threads", includeArchived] as const,
  threads: ["threads"] as const,
  threadBundle: (threadId: string) => ["threads", threadId, "bundle"] as const,
  threadEvents: (threadId: string) => ["threads", threadId, "events"] as const,
  changes: (threadId: string, kind: string) =>
    ["threads", threadId, "changes", kind] as const,
  fileDiff: (threadId: string, path: string, kind: string, commit = "") =>
    ["threads", threadId, "fileDiff", path, kind, commit] as const,
  comments: (threadId: string) => ["threads", threadId, "comments"] as const,
  queue: (threadId: string) => ["threads", threadId, "queue"] as const,
  files: (threadId: string, path: string) =>
    ["threads", threadId, "files", path] as const,
  file: (threadId: string, path: string) =>
    ["threads", threadId, "file", path] as const,
  inbox: (filter: InboxFilter) => ["inbox", filter] as const,
  search: (query: string) => ["search", query] as const,
  state: ["state"] as const,
  agentConfig: (threadId: string) => ["threads", threadId, "agentConfig"] as const,
  directories: (path: string) => ["fs", "directories", path] as const,
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

export function useProjectOverview(projectId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.projectOverview(projectId ?? ""),
    queryFn: () => fetchProjectOverview(projectId ?? ""),
    enabled: Boolean(projectId),
    refetchInterval: 10_000,
  });
}

export function useProjectThreads(
  projectId: string | undefined,
  includeArchived = false,
) {
  return useQuery({
    queryKey: queryKeys.projectThreads(projectId ?? "", includeArchived),
    queryFn: () => fetchProjectThreads(projectId ?? "", includeArchived),
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

export function useChanges(
  threadId: string | undefined,
  kind: string,
  commit = "",
) {
  return useQuery({
    queryKey: queryKeys.changes(threadId ?? "", `${kind}:${commit}`),
    queryFn: () => fetchChanges(threadId ?? "", kind, commit),
    enabled: Boolean(threadId),
    refetchInterval: 3_000,
  });
}

export function useFileDiff(
  threadId: string | undefined,
  path: string | null,
  kind: string,
  commit = "",
) {
  return useQuery({
    queryKey: queryKeys.fileDiff(threadId ?? "", path ?? "", kind, commit),
    queryFn: () => fetchFileDiff(threadId ?? "", path ?? "", kind, commit),
    enabled: Boolean(threadId && path),
  });
}

export function useQueue(threadId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.queue(threadId ?? ""),
    queryFn: () => fetchQueue(threadId ?? ""),
    enabled: Boolean(threadId),
    refetchInterval: 2_000,
  });
}

export function useAgentConfig(threadId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.agentConfig(threadId ?? ""),
    queryFn: () => fetchAgentConfig(threadId ?? ""),
    enabled: Boolean(threadId),
    staleTime: 60_000,
    refetchInterval: (query) =>
      (query.state.data?.options.length ?? 0) > 0 ? false : 2_000,
  });
}

export function useDirectoryListing(path: string) {
  return useQuery({
    queryKey: queryKeys.directories(path),
    queryFn: () => fetchDirectories(path),
    retry: false,
    staleTime: 5_000,
  });
}

export function useFiles(threadId: string | undefined, path: string) {
  return useQuery({
    queryKey: queryKeys.files(threadId ?? "", path),
    queryFn: () => fetchFiles(threadId ?? "", path),
    enabled: Boolean(threadId),
  });
}

export function useFile(threadId: string | undefined, path: string | null) {
  return useQuery({
    queryKey: queryKeys.file(threadId ?? "", path ?? ""),
    queryFn: () => fetchFile(threadId ?? "", path ?? ""),
    enabled: Boolean(threadId && path),
  });
}

export function useComments(threadId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.comments(threadId ?? ""),
    queryFn: () => fetchComments(threadId ?? ""),
    enabled: Boolean(threadId),
  });
}

export function useInbox(filter: InboxFilter) {
  return useQuery({
    queryKey: queryKeys.inbox(filter),
    queryFn: () => fetchInbox(filter),
    refetchInterval: 5_000,
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
