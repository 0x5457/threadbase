import { request } from "./client";
import { ApiError } from "./client";
import type { CreateTerminalSessionRequest } from "./types/CreateTerminalSessionRequest";
import type { TerminalSessionDto } from "./types/TerminalSessionDto";
import type { TerminalSessionResponse } from "./types/TerminalSessionResponse";
import type { TerminalSessionsResponse } from "./types/TerminalSessionsResponse";

export type TerminalSession = TerminalSessionDto;

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
