import { useEffect, useMemo, useState } from "react";
import { notifications } from "@mantine/notifications";
import { useQuery } from "@tanstack/react-query";
import { ApiError, getThread, listAllThreads, type ThreadSummary } from "../api";
import { pathFor, type Route } from "./router";

type Go = (path: string, options?: { replace?: boolean }) => void;

/**
 * The open thread and the polled list every surface reads.
 *
 * The URL is authoritative, so the open thread resolves during render from the
 * freshest cached copy; a deep link that matches no list entry fetches the one
 * thread it names.
 */
export function useSelectedThread(route: Route, go: Go) {
  const routeThreadId =
    route.kind === "thread" || route.kind === "subagent"
      ? route.threadId
      : null;
  const [selectedThreadState, setSelectedThread] =
    useState<ThreadSummary | null>(null);
  const allThreads = useQuery({
    queryKey: ["threads-all"],
    queryFn: listAllThreads,
    staleTime: 10_000,
    // The sidebar is the only consumer that needs this without a live socket.
    // Ten seconds keeps other threads' status marks fresh without rebuilding
    // the tree on every 5 s tick.
    refetchInterval: 10_000,
    refetchIntervalInBackground: false,
  });
  // Waiting for an effect made every back gesture paint the placeholder first
  // (and, when the route and state disagreed, briefly mount the previous
  // thread's transcript).
  const selectedThread = useMemo(() => {
    if (routeThreadId === null) return null;
    const known =
      selectedThreadState?.id === routeThreadId ? selectedThreadState : null;
    const listed =
      allThreads.data?.threads.find((thread) => thread.id === routeThreadId) ??
      null;
    if (known === null) return listed;
    // Local patches (pin, rename, archive) carry a newer timestamp than the
    // polled list; once the list catches up it wins again.
    if (listed === null) return known;
    return known.updatedAt >= listed.updatedAt ? known : listed;
  }, [allThreads.data, routeThreadId, selectedThreadState]);

  // A thread URL is authoritative: fetch the bundle directly when neither the
  // list nor a previous open already resolved it, so a deep link works before
  // any list has loaded.
  useEffect(() => {
    if (routeThreadId === null) {
      if (selectedThreadState !== null) setSelectedThread(null);
      return;
    }
    if (selectedThreadState?.id === routeThreadId) return;
    if (
      allThreads.data?.threads.some((thread) => thread.id === routeThreadId)
    ) {
      // Render already resolves it from the list; no state copy needed.
      return;
    }
    let cancelled = false;
    void getThread(routeThreadId)
      .then(({ thread }) => {
        if (!cancelled) setSelectedThread(thread);
      })
      .catch((cause: unknown) => {
        if (cancelled) return;
        setSelectedThread(null);
        if (!(cause instanceof ApiError) || cause.status !== 404) return;
        notifications.show({
          message: "Thread not found — showing its project",
          color: "red",
        });
        if (route.kind === "thread") {
          go(pathFor({ kind: "project", projectId: route.projectId }), {
            replace: true,
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    allThreads.data,
    go,
    route.kind,
    route.kind === "thread" ? route.projectId : null,
    routeThreadId,
    selectedThreadState,
  ]);

  return { routeThreadId, selectedThread, setSelectedThread, allThreads };
}
