import { request } from "./client";
import type { SessionSummaryDto } from "./types/SessionSummaryDto";
import type { SessionsResponse } from "./types/SessionsResponse";

export type SessionSummary = SessionSummaryDto;
export type ThreadSessionSummary = Extract<SessionSummary, { kind: "thread" }>;
export type TmuxSessionSummary = Extract<SessionSummary, { kind: "tmux" }>;

/**
 * Live work on this machine: agent threads and host tmux sessions in one
 * ordered projection. Tmux entries are omitted when the host feature is off or
 * the caller may not see them.
 */
export function listSessions() {
  return request<SessionsResponse>("/api/sessions");
}
