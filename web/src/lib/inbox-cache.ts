import type { QueryClient } from "@tanstack/react-query";
import type { InboxItem } from "../api";
import type { InboxPageDto } from "../api/types/InboxPageDto";

type InfiniteInbox = {
  pages: InboxPageDto[];
  pageParams: unknown[];
};

function readPage(page: InboxPageDto, item: InboxItem): InboxPageDto {
  let removedUnread = 0;
  const items = page.items.flatMap((entry) => {
    if (entry.threadId !== item.threadId) return [entry];
    if (!entry.read) removedUnread += 1;
    return [{ ...entry, read: true }];
  });
  return {
    ...page,
    items,
    unread: Math.max(0, page.unread - removedUnread),
  };
}

function dropThread(page: InboxPageDto, item: InboxItem): InboxPageDto {
  let removedUnread = 0;
  const items = page.items.filter((entry) => {
    if (entry.threadId !== item.threadId) return true;
    if (!entry.read) removedUnread += 1;
    return false;
  });
  return {
    ...page,
    items,
    unread: Math.max(0, page.unread - removedUnread),
  };
}

/**
 * Marks one thread's notifications read everywhere they are cached. Returns a
 * rollback that restores every touched query, so a failed write leaves the
 * inbox exactly as it was.
 */
export function optimisticInboxRead(
  queryClient: QueryClient,
  item: InboxItem,
): () => void {
  const snapshots = [
    ...queryClient.getQueriesData({ queryKey: ["inbox"] }),
    ...queryClient.getQueriesData({ queryKey: ["inbox-unread"] }),
  ];

  // The filter is part of the key: "all" keeps the row and clears its dot;
  // "unread" drops the thread the same way the next fetch will.
  for (const [key] of queryClient.getQueriesData<InfiniteInbox>({
    queryKey: ["inbox"],
  })) {
    const filter = key[1];
    queryClient.setQueryData<InfiniteInbox>(key, (data) => {
      if (!data) return data;
      return {
        ...data,
        pages: data.pages.map((page) =>
          filter === "unread" ? dropThread(page, item) : readPage(page, item),
        ),
      };
    });
  }

  for (const [key, data] of queryClient.getQueriesData<InboxPageDto>({
    queryKey: ["inbox-unread"],
  })) {
    if (!data) continue;
    queryClient.setQueryData<InboxPageDto>(key, dropThread(data, item));
  }

  return () => {
    for (const [key, data] of snapshots) {
      queryClient.setQueryData(key, data);
    }
  };
}
