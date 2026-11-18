import { jsonRequest, request } from "./client";
import type { AuthMeDto } from "./types/AuthMeDto";
import type { AuthStatusResponse } from "./types/AuthStatusResponse";
import type { UpdateUserRequest } from "./types/UpdateUserRequest";
import type { UserDto } from "./types/UserDto";
import type { UserListResponse } from "./types/UserListResponse";

type Me = AuthMeDto;
export type User = UserDto;
export type UserAccess = UserDto["access"];

export function listUsers() {
  return request<UserListResponse>("/api/auth/users");
}

export function updateUser(userId: string, input: UpdateUserRequest) {
  return jsonRequest<UserListResponse>(
    `/api/auth/users/${encodeURIComponent(userId)}`,
    "PATCH",
    input,
  );
}

export function getMe() {
  return request<Me>("/api/auth/me");
}

export function getAuthStatus() {
  return request<AuthStatusResponse>("/api/auth/status");
}

/**
 * Trades the tailnet identity `tailscale serve` proved for a session cookie.
 * There is nothing to type: if the request did not come through the proxy, the
 * server refuses it and the shell says so.
 */
export function signInWithTailscale() {
  return jsonRequest<Me>("/api/auth/tailscale", "POST", {});
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
