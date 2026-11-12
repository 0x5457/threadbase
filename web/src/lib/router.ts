import { useLocation, useParams } from "react-router";

/**
 * The URL is the single source of truth for which screen is open, so deep
 * links, reloads and the back button all work without a custom history store.
 */

export type Route =
  | { kind: "inbox" }
  | { kind: "settings" }
  | { kind: "project"; projectId: string }
  | { kind: "thread"; projectId: string; threadId: string };

/** Canonical pathname for a route. */
export function pathFor(route: Route): string {
  switch (route.kind) {
    case "inbox":
      return "/inbox";
    case "settings":
      return "/settings";
    case "project":
      return `/p/${route.projectId}`;
    case "thread":
      return `/p/${route.projectId}/t/${route.threadId}`;
  }
}

/** Current route from the matched URL params. */
export function useRoute(): Route {
  const { pathname } = useLocation();
  const { projectId, threadId } = useParams<{
    projectId?: string;
    threadId?: string;
  }>();

  if (projectId && threadId) return { kind: "thread", projectId, threadId };
  if (projectId) return { kind: "project", projectId };
  if (pathname.replace(/\/+$/, "") === "/settings") {
    return { kind: "settings" };
  }
  return { kind: "inbox" };
}
