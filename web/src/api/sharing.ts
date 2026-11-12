import { jsonRequest } from "./client";
import type { CreateInviteRequest } from "./types/CreateInviteRequest";
import type { InviteDto } from "./types/InviteDto";
import type { InviteResponse } from "./types/InviteResponse";
import type { OkResponse } from "./types/OkResponse";
import type { Role } from "./types/Role";
import type { ShareLevel } from "./types/ShareLevel";
import type { ShareStateDto } from "./types/ShareStateDto";
import type { ThreadMemberDto } from "./types/ThreadMemberDto";

export type ShareMember = ThreadMemberDto;
export type ShareInvite = InviteDto;
export type ShareState = ShareStateDto;

export function setShareLevel(threadId: string, shareLevel: ShareLevel) {
  return jsonRequest<OkResponse>(`/api/threads/${threadId}/share`, "PATCH", {
    shareLevel,
  });
}

export function addMember(threadId: string, login: string, role: Role) {
  return jsonRequest<ThreadMemberDto>(
    `/api/threads/${threadId}/members`,
    "POST",
    { login, role },
  );
}

export function updateMember(threadId: string, userId: string, role: Role) {
  return jsonRequest<ThreadMemberDto>(
    `/api/threads/${threadId}/members/${userId}`,
    "PATCH",
    { role },
  );
}

export function removeMember(threadId: string, userId: string) {
  return jsonRequest<OkResponse>(
    `/api/threads/${threadId}/members/${userId}`,
    "DELETE",
  );
}

export function createInvite(
  threadId: string,
  input: { email?: string; username?: string; role: Role },
) {
  const body: CreateInviteRequest = {
    email: input.email ?? null,
    username: input.username ?? null,
    role: input.role,
  };
  return jsonRequest<InviteResponse>(
    `/api/threads/${threadId}/invites`,
    "POST",
    body,
  );
}

export function revokeInvite(threadId: string, inviteId: string) {
  return jsonRequest<OkResponse>(
    `/api/threads/${threadId}/invites/${inviteId}`,
    "DELETE",
  );
}
