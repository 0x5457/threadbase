import type { QueryClient } from "@tanstack/react-query";
import type { ThreadSummary } from "../api";
import type { ThreadSection } from "../api/types/ThreadSection";

type ThreadsData = { threads: ThreadSummary[] };
type ThreadBundleData = {
  bundle: {
    thread: ThreadSummary;
    [key: string]: unknown;
  };
};
type SessionsData = {
  sessions: Array<{
    kind: string;
    id: string;
    pinned?: boolean;
    status?: string;
  }>;
};

function sectionFor(
  thread: ThreadSummary,
  patch: Partial<ThreadSummary>,
): ThreadSection {
  const status = patch.status ?? thread.status;
  const placement = patch.placement ?? thread.placement;
  const pinned = patch.pinned ?? thread.pinned;
  if (status === "archived") return "archived";
  if (pinned) return "pinned";
  if (placement === "snoozed") return "snoozed";
  if (placement === "settled") return "settled";
  return status === "running" ||
    status === "preparing" ||
    status === "requires_action"
    ? "working"
    : "active";
}

function mergeThread(
  thread: ThreadSummary,
  patch: Partial<ThreadSummary>,
): ThreadSummary {
  const next = { ...thread, ...patch, updatedAt: Date.now() };
  return { ...next, section: sectionFor(next, patch) };
}

/**
 * Applies a thread patch to every list/detail cache that renders it. The
 * returned rollback restores the exact snapshots, so an optimistic action can
 * put every surface back when the server refuses it.
 */
export function optimisticThreadPatch(
  queryClient: QueryClient,
  threadId: string,
  patch: Partial<ThreadSummary>,
  fallback?: ThreadSummary,
): () => void {
  const snapshots = [
    ...queryClient.getQueriesData({ queryKey: ["threads-all"] }),
    ...queryClient.getQueriesData({ queryKey: ["thread", threadId] }),
    ...queryClient.getQueriesData({ queryKey: ["sessions"] }),
  ];

  queryClient.setQueryData<ThreadsData>(["threads-all"], (data) => {
    if (!data) return data;
    if (patch.archivedAt) {
      return {
        ...data,
        threads: data.threads.filter((thread) => thread.id !== threadId),
      };
    }
    let found = false;
    const threads = data.threads.map((thread) => {
      if (thread.id !== threadId) return thread;
      found = true;
      return mergeThread(thread, patch);
    });
    if (!found && fallback) {
      threads.push(mergeThread(fallback, patch));
    }
    return { ...data, threads };
  });

  for (const [key] of queryClient.getQueriesData<ThreadBundleData>({
    queryKey: ["thread", threadId],
  })) {
    // ["thread", id, "goal"] is not a bundle; only list/meta carry the thread.
    if (key[2] !== undefined && key[2] !== "meta") continue;
    queryClient.setQueryData<ThreadBundleData>(key, (data) => {
      if (!data) return data;
      return {
        ...data,
        bundle: {
          ...data.bundle,
          thread: mergeThread(data.bundle.thread, patch),
        },
      };
    });
  }

  queryClient.setQueryData<SessionsData>(["sessions"], (data) => {
    if (!data) return data;
    return {
      ...data,
      sessions: data.sessions.map((session) =>
        session.kind === "thread" && session.id === threadId
          ? {
              ...session,
              ...(patch.pinned === undefined
                ? {}
                : { pinned: patch.pinned }),
              ...(patch.status === undefined ? {} : { status: patch.status }),
            }
          : session,
      ),
    };
  });

  return () => {
    for (const [key, data] of snapshots) {
      queryClient.setQueryData(key, data);
    }
  };
}
