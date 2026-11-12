import { jsonRequest, request } from "./client";
import type { AuthMeDto } from "./types/AuthMeDto";
import type { AuthStatusResponse } from "./types/AuthStatusResponse";

export type Me = AuthMeDto;

export function getMe() {
  return request<Me>("/api/auth/me");
}

export function getAuthStatus() {
  return request<AuthStatusResponse>("/api/auth/status");
}

/**
 * Revokes the server-side session. Uses a bare fetch because the endpoint
 * answers 204 with no body, which the JSON helper cannot parse.
 */
export async function logout(): Promise<void> {
  await fetch("/api/auth/logout", {
    method: "POST",
    credentials: "same-origin",
    headers: {
      "x-requested-with": "threadbase",
      "content-type": "application/json",
    },
  });
}

export function acceptInvite(token: string) {
  return jsonRequest<import("./types/ThreadMemberDto").ThreadMemberDto>(
    `/api/invites/${encodeURIComponent(token)}/accept`,
    "POST",
  );
}
