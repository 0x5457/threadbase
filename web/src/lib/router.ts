import { useSyncExternalStore } from "react";

/**
 * Minimal history router: the URL is the single source of truth for which
 * screen is open, so deep links, reloads and the back button all work without
 * pulling in a routing dependency for five screens.
 */

export type Route =
  | { kind: "inbox" }
  | { kind: "settings" }
  | { kind: "project"; projectId: string }
  | { kind: "thread"; projectId: string; threadId: string };

const KNOWN_PREFIXES = ["/inbox", "/settings", "/p/"];

/** Parses a pathname into a route; anything unknown falls back to the inbox. */
export function parseRoute(pathname: string): Route {
  const parts = pathname.replace(/\/+$/, "").split("/").filter(Boolean);
  if (parts.length === 0) return { kind: "inbox" };
  if (parts[0] === "settings") return { kind: "settings" };
  if (parts[0] === "inbox") return { kind: "inbox" };
  if (parts[0] === "p" && parts[1]) {
    if (parts[2] === "t" && parts[3]) {
      return { kind: "thread", projectId: parts[1], threadId: parts[3] };
    }
    return { kind: "project", projectId: parts[1] };
  }
  return { kind: "inbox" };
}

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

/** True when the pathname maps onto a screen we know about. */
export function isKnownPath(pathname: string): boolean {
  if (pathname === "/" || pathname === "") return true;
  return KNOWN_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(prefix),
  );
}

const CHANGE_EVENT = "threadbase:navigate";

export function currentPathname(): string {
  return window.location.pathname;
}

/** Pushes (or replaces) a route and notifies subscribers. */
export function navigate(path: string, options?: { replace?: boolean }): void {
  if (window.location.pathname === path) return;
  if (options?.replace) {
    window.history.replaceState(null, "", path);
  } else {
    window.history.pushState(null, "", path);
  }
  window.dispatchEvent(new Event(CHANGE_EVENT));
}

function subscribe(onChange: () => void): () => void {
  const handler = () => onChange();
  window.addEventListener("popstate", handler);
  window.addEventListener(CHANGE_EVENT, handler);
  return () => {
    window.removeEventListener("popstate", handler);
    window.removeEventListener(CHANGE_EVENT, handler);
  };
}

/** Subscribes the component tree to URL changes. */
export function useRoute(): Route {
  return parseRoute(useSyncExternalStore(subscribe, currentPathname));
}
