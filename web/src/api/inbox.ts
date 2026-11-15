import { jsonRequest, request } from "./client";
import type { InboxItemDto } from "./types/InboxItemDto";
import type { InboxPageDto } from "./types/InboxPageDto";
import type { OkResponse } from "./types/OkResponse";

export type InboxItem = InboxItemDto;

export function listInbox(
  filter: "unread" | "all" = "unread",
  before?: string,
) {
  const params = new URLSearchParams({ filter });
  if (before) params.set("before", before);
  return request<InboxPageDto>(`/api/inbox?${params.toString()}`);
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
