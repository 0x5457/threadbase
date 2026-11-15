import { useLocation, useParams } from "react-router";

/**
 * The URL is the single source of truth for which screen is open, so deep
 * links, reloads and the back button all work without a custom history store.
 */

type Route =
  | { kind: "inbox" }
  | { kind: "settings" }
  | { kind: "project"; projectId: string }
  | { kind: "thread"; projectId: string; threadId: string }
  | {
      kind: "subagent";
      projectId: string;
      threadId: string;
      subagentId: string;
    };

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
    case "subagent":
      // Workflow children are stored as `<parent>/<child>`; the slash must be
      // encoded or the URL would read as another route segment.
      return `/p/${route.projectId}/t/${route.threadId}/s/${encodeURIComponent(route.subagentId)}`;
  }
}

/** Current route from the matched URL params. */
export function useRoute(): Route {
  const { pathname } = useLocation();
  const { projectId, threadId, subagentId } = useParams<{
    projectId?: string;
    threadId?: string;
    subagentId?: string;
  }>();

  if (projectId && threadId && subagentId) {
    return { kind: "subagent", projectId, threadId, subagentId };
  }
  if (projectId && threadId) return { kind: "thread", projectId, threadId };
  if (projectId) return { kind: "project", projectId };
  if (pathname.replace(/\/+$/, "") === "/settings") {
    return { kind: "settings" };
  }
  return { kind: "inbox" };
}
