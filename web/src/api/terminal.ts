import { request } from "./client";
import { ApiError } from "./client";
import type { CreateTerminalSessionRequest } from "./types/CreateTerminalSessionRequest";
import type { HostTerminalAttachResponse } from "./types/HostTerminalAttachResponse";
import type { HostTerminalSessionDto } from "./types/HostTerminalSessionDto";
import type { TerminalSessionDto } from "./types/TerminalSessionDto";
import type { TerminalSessionResponse } from "./types/TerminalSessionResponse";
import type { TerminalSessionsResponse } from "./types/TerminalSessionsResponse";

export type TerminalSession = TerminalSessionDto;
export type HostTerminalSession = HostTerminalSessionDto;

/**
 * The drawer always shows exactly one target: a threadbase shell for a thread
 * (or the home shell), or a session on a host tmux server. Host targets carry
 * the socket because `$0` repeats on every server.
 */
export type TerminalTarget =
  | { kind: "workbench"; threadId: string | null }
  | { kind: "host"; socket: string; id: string; name?: string };

/** Live shells on this machine; `threadId` narrows it to one thread. */
export function listTerminalSessions(threadId?: string | null) {
  const query = threadId ? `?threadId=${encodeURIComponent(threadId)}` : "";
  return request<TerminalSessionsResponse>(`/api/terminal/sessions${query}`);
}

/** Create (or reuse) the shell for a thread, and mint an attach ticket. */
export function createTerminalSession(body: {
  threadId: string | null;
  cols?: number;
  rows?: number;
  watchOnly?: boolean;
}) {
  const payload: CreateTerminalSessionRequest = {
    threadId: body.threadId as CreateTerminalSessionRequest["threadId"],
    cols: body.cols ?? null,
    rows: body.rows ?? null,
    watchOnly: body.watchOnly ?? false,
  };
  return request<TerminalSessionResponse>("/api/terminal/sessions", {
    method: "POST",
    body: JSON.stringify(payload),
  });
}

/** Attach ticket for a shell that already exists. */
export function attachTerminalSession(sessionId: string, watchOnly = false) {
  return request<TerminalSessionResponse>(
    `/api/terminal/sessions/${encodeURIComponent(sessionId)}/attach`,
    { method: "POST", body: JSON.stringify({ watchOnly }) },
  );
}

export async function killTerminalSession(sessionId: string) {
  // The error body is only read on a non-2xx response; the rule cannot see
  // through the early return.
  // react-doctor-disable-next-line react-doctor/no-fetch-response-used-without-status-check
  const response = await fetch(
    `/api/terminal/sessions/${encodeURIComponent(sessionId)}`,
    {
      method: "DELETE",
      credentials: "same-origin",
      headers: { "x-requested-with": "threadbase" },
    },
  );
  if (response.ok) return;
  const body = (await response.json().catch(() => null)) as {
    error?: { message?: string; code?: string };
  } | null;
  throw new ApiError(
    body?.error?.message ?? `Could not close the terminal (${response.status})`,
    response.status,
    body?.error?.code ?? null,
  );
}

/**
 * WebSocket URLs cannot carry custom headers, so the ticket minted by the POST
 * is what authorises the upgrade.
 */
export function terminalSocketUrl(sessionId: string, ticket: string) {
  const scheme = window.location.protocol === "https:" ? "wss" : "ws";
  const query = new URLSearchParams({ ticket }).toString();
  return `${scheme}://${window.location.host}/ws/terminal/${encodeURIComponent(sessionId)}?${query}`;
}

/** Attach to a host session; the ticket names the socket and tmux `$N` id. */
export function attachHostTerminalSession(
  socket: string,
  sessionId: string,
  watchOnly = false,
) {
  return request<HostTerminalAttachResponse>(
    `/api/tmux/sessions/${encodeURIComponent(socket)}/${encodeURIComponent(sessionId)}/attach`,
    { method: "POST", body: JSON.stringify({ watchOnly }) },
  );
}

/** The host socket carries the target in the ticket instead of the path. */
export function hostTerminalSocketUrl(ticket: string) {
  const scheme = window.location.protocol === "https:" ? "wss" : "ws";
  const query = new URLSearchParams({ ticket }).toString();
  return `${scheme}://${window.location.host}/ws/tmux?${query}`;
}
