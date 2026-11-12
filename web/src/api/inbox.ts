import { jsonRequest, request } from "./client";
import type { InboxItemDto } from "./types/InboxItemDto";
import type { InboxPageDto } from "./types/InboxPageDto";
import type { OkResponse } from "./types/OkResponse";

export type InboxItem = InboxItemDto;

export function listInbox(filter: "unread" | "all" = "unread") {
  return request<InboxPageDto>(`/api/inbox?filter=${filter}`);
}

export function markInboxRead(threadId: string) {
  return jsonRequest<OkResponse>("/api/inbox/read", "POST", {
    threadId,
    all: null,
  });
}

export function markAllInboxRead() {
  return jsonRequest<OkResponse>("/api/inbox/read", "POST", {
    threadId: null,
    all: true,
  });
}
