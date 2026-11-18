import { jsonRequest, request } from "./client";
import type { GuestLinkResponse } from "./types/GuestLinkResponse";
import type { GuestLinkStatus } from "./types/GuestLinkStatus";
import type { GuestMessageAction } from "./types/GuestMessageAction";
import type { GuestMessageDecision } from "./types/GuestMessageDecision";
import type { GuestMessageDto } from "./types/GuestMessageDto";
import type { GuestMessagesResponse } from "./types/GuestMessagesResponse";
import type { GuestSessionRequest } from "./types/GuestSessionRequest";
import type { GuestThreadResponse } from "./types/GuestThreadResponse";

/** Turns the link token in the URL into a cookie-bound guest session. */
export function startGuestSession(token: string) {
  const body: GuestSessionRequest = { token };
  return jsonRequest<GuestThreadResponse>("/api/guest/session", "POST", body);
}

export function getGuestThread(after?: number) {
  const suffix = after ? `?after=${after}` : "";
  return request<GuestThreadResponse>(`/api/guest/thread${suffix}`);
}

export function sendGuestDraft(text: string) {
  return jsonRequest<GuestMessageDto>("/api/guest/messages", "POST", { text });
}

export function getGuestLink(threadId: string) {
  return request<GuestLinkStatus>(`/api/threads/${threadId}/guest-link`);
}

export function createGuestLink(threadId: string) {
  return jsonRequest<GuestLinkResponse>(
    `/api/threads/${threadId}/guest-link`,
    "POST",
    {},
  );
}

export function revokeGuestLink(threadId: string) {
  return jsonRequest<GuestLinkResponse>(
    `/api/threads/${threadId}/guest-link`,
    "DELETE",
  );
}

export function listGuestMessages(threadId: string) {
  return request<GuestMessagesResponse>(
    `/api/threads/${threadId}/guest-messages`,
  );
}

export function decideGuestMessage(
  threadId: string,
  messageId: string,
  action: GuestMessageAction,
) {
  const body: GuestMessageDecision = { action };
  return jsonRequest<GuestMessagesResponse>(
    `/api/threads/${threadId}/guest-messages/${messageId}`,
    "POST",
    body,
  );
}
