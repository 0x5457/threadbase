import type { SessionSummary } from "../api";

interface TmuxSessionRef {
  socket: string;
  id: string;
}

const TMUX_PREFIX = "tmux:";

/**
 * The active tmux session lives in the query string, not the path: it is an
 * overlay on top of whichever surface is open, so opening it from a thread
 * must not throw the thread away. `?session=tmux:<socket>:<$N>` is the whole
 * contract, and it is shareable as-is.
 */
export function parseTmuxSession(search: string): TmuxSessionRef | null {
  const raw = new URLSearchParams(search).get("session");
  if (!raw?.startsWith(TMUX_PREFIX)) return null;
  const rest = raw.slice(TMUX_PREFIX.length);
  const separator = rest.indexOf(":");
  if (separator <= 0) return null;
  const socket = rest.slice(0, separator);
  const id = rest.slice(separator + 1);
  // Socket names are validated server-side to a safe alphabet; `$N` is the
  // only id shape the host route accepts.
  if (!/^[A-Za-z0-9._-]+$/.test(socket) || !/^\$\d+$/.test(id)) return null;
  return { socket, id };
}

function tmuxSessionParam(socket: string, id: string): string {
  return `${TMUX_PREFIX}${socket}:${id}`;
}

/** Preserve unrelated query parameters while setting or clearing the session. */
export function withSessionParam(
  search: string,
  ref: TmuxSessionRef | null,
): string {
  const params = new URLSearchParams(search);
  if (ref) {
    params.set("session", tmuxSessionParam(ref.socket, ref.id));
  } else {
    params.delete("session");
  }
  return params.toString();
}

export function sessionKey(session: { kind: string; id: string; socket?: string }) {
  return session.kind === "tmux"
    ? `tmux:${session.socket}:${session.id}`
    : `thread:${session.id}`;
}

export function sessionMatches(session: SessionSummary, needle: string): boolean {
  if (!needle) return true;
  const fields =
    session.kind === "thread"
      ? [
          session.title,
          session.projectName,
          session.agent,
          session.status,
          session.terminal?.controller?.displayName,
          ...(session.terminal?.viewers.map((user) => user.displayName) ?? []),
        ]
      : [
          session.name,
          session.socket,
          session.cwd,
          session.command,
          session.controller?.displayName,
          ...session.viewers.map((user) => user.displayName),
        ];
  return fields.some((field) => field?.toLowerCase().includes(needle));
}

const dateTime = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});
const monthDay = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
});

export function fullTime(ts: number): string {
  return dateTime.format(new Date(ts));
}

/** Compact, stable activity label; hover shows the exact timestamp. */
export function timeAgo(ts: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.round((now - ts) / 1000));
  if (seconds < 45) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days}d ago`;
  return monthDay.format(new Date(ts));
}
