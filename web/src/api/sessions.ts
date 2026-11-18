import { jsonRequest, request } from "./client";
import type { CreateThreadFromTmuxRequest } from "./types/CreateThreadFromTmuxRequest";
import type { SessionSummaryDto } from "./types/SessionSummaryDto";
import type { SessionsResponse } from "./types/SessionsResponse";
import type { ThreadResponse } from "./types/ThreadResponse";

export type SessionSummary = SessionSummaryDto;
export type TmuxSessionSummary = Extract<SessionSummary, { kind: "tmux" }>;

/**
 * Live work on this machine: agent threads and host tmux sessions in one
 * ordered projection. Tmux entries are omitted when the host feature is off or
 * the caller may not see them.
 */
export function listSessions() {
  return request<SessionsResponse>("/api/sessions");
}

/**
 * Turn a live host tmux session into a thread. The server infers the project
 * from the session's live cwd and records the origin link.
 */
export function createThreadFromTmux(
  socket: string,
  sessionId: string,
  input: {
    title?: string | null;
    agent?: string | null;
    model?: string | null;
    effort?: string | null;
    worktree?: boolean | null;
    initialPrompt?: string | null;
  },
) {
  const body: CreateThreadFromTmuxRequest = {
    title: input.title ?? null,
    agent: input.agent ?? null,
    model: input.model ?? null,
    effort: input.effort ?? null,
    worktree: input.worktree ?? null,
    initialPrompt: input.initialPrompt ?? null,
  };
  return jsonRequest<ThreadResponse>(
    `/api/tmux/sessions/${encodeURIComponent(socket)}/${encodeURIComponent(sessionId)}/thread`,
    "POST",
    body,
  );
}
