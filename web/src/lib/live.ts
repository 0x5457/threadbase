import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { toast } from "sonner";
import { create } from "zustand";
import { queryKeys, useMe } from "@/api/queries";
import type { EventEnvelope } from "@/api/types/EventEnvelope";
import type { EventsPageDto } from "@/api/types/EventsPageDto";
import type { ServerWsMessage } from "@/api/types/ServerWsMessage";
import type { ThreadBundleResponse } from "@/api/types/ThreadBundleResponse";
import type { ThreadStatus } from "@/api/types/ThreadStatus";

export const useLiveStore = create<{ connected: boolean }>(() => ({
  connected: false,
}));

function websocketUrl(path: string): string {
  const protocol = window.location.protocol === "https:" ? "wss" : "ws";
  return `${protocol}://${window.location.host}${path}`;
}

function browserNotify(title: string, body: string) {
  if (
    document.hidden &&
    typeof Notification !== "undefined" &&
    Notification.permission === "granted"
  ) {
    const notification = new Notification(title, {
      body,
      tag: "threadbase-approval",
    });
    notification.onclick = () => {
      window.focus();
      notification.close();
    };
  }
}

function applyEvent(
  queryClient: ReturnType<typeof useQueryClient>,
  threadId: string,
  event: EventEnvelope,
) {
  const key = queryKeys.threadEvents(threadId);
  queryClient.setQueryData<EventsPageDto>(key, (current) => {
    if (!current) {
      return {
        threadId,
        events: [event],
        lastSeq: event.seq,
        oldestSeq: event.seq,
        resync: false,
      };
    }
    const seq = Number(event.seq);
    if (seq <= Number(current.lastSeq)) {
      return current;
    }
    return {
      ...current,
      events: [...current.events, event],
      lastSeq: event.seq,
    };
  });
}

/**
 * Subscribe to a thread's event stream. Reconnects with backoff and replays
 * from the last sequence the client already has.
 */
export function useThreadLiveEvents(threadId: string | undefined) {
  const queryClient = useQueryClient();
  const me = useMe();
  const meRef = useRef(me.data);
  useEffect(() => {
    meRef.current = me.data;
  }, [me.data]);

  useEffect(() => {
    if (!threadId) {
      return;
    }
    let socket: WebSocket | null = null;
    let retryTimer: number | undefined;
    let closed = false;
    let attempts = 0;

    const connect = () => {
      if (closed) {
        return;
      }
      socket = new WebSocket(websocketUrl("/ws"));
      socket.onopen = () => {
        attempts = 0;
        useLiveStore.setState({ connected: true });
        const page = queryClient.getQueryData<EventsPageDto>(
          queryKeys.threadEvents(threadId),
        );
        socket?.send(
          JSON.stringify({
            type: "subscribe",
            threadId,
            afterSeq: page ? Number(page.lastSeq) : 0,
          }),
        );
      };
      socket.onmessage = (message) => {
        let parsed: ServerWsMessage;
        try {
          parsed = JSON.parse(String(message.data)) as ServerWsMessage;
        } catch {
          return;
        }
        switch (parsed.type) {
          case "event": {
            applyEvent(queryClient, threadId, parsed.event);
            if (parsed.event.payload.type === "thread_updated") {
              void queryClient.invalidateQueries({ queryKey: queryKeys.threads });
              void queryClient.invalidateQueries({
                queryKey: queryKeys.threadBundle(threadId),
              });
            }
            if (
              parsed.event.payload.type === "comment_added" ||
              parsed.event.payload.type === "comment_updated"
            ) {
              const comment = parsed.event.payload.comment;
              if (
                parsed.event.payload.type === "comment_added" &&
                meRef.current &&
                comment.body.includes(`@${meRef.current.user.username}`)
              ) {
                browserNotify("Mentioned in a thread", comment.body);
              }
              void queryClient.invalidateQueries({
                queryKey: queryKeys.comments(threadId),
              });
              void queryClient.invalidateQueries({
                queryKey: queryKeys.inbox("unread"),
              });
            }
            if (
              parsed.event.payload.type === "permission_requested" ||
              parsed.event.payload.type === "elicitation_requested"
            ) {
              const payload = parsed.event.payload;
              browserNotify(
                payload.type === "permission_requested"
                  ? "Approval requested"
                  : "Agent needs input",
                payload.type === "permission_requested"
                  ? payload.approval.title
                  : payload.elicitation.message,
              );
              void queryClient.invalidateQueries({
                queryKey: queryKeys.inbox("unread"),
              });
              void queryClient.invalidateQueries({
                queryKey: queryKeys.threadBundle(threadId),
              });
            }
            break;
          }
          case "events_page": {
            queryClient.setQueryData(
              queryKeys.threadEvents(threadId),
              parsed.page,
            );
            break;
          }
          case "thread_state": {
            queryClient.setQueryData<ThreadBundleResponse>(
              queryKeys.threadBundle(threadId),
              (current) =>
                current
                  ? {
                      ...current,
                      bundle: {
                        ...current.bundle,
                        thread: {
                          ...current.bundle.thread,
                          status: parsed.status as ThreadStatus,
                          pendingApprovals: parsed.pendingApprovals,
                          queuedInputs: parsed.queuedInputs,
                        },
                      },
                    }
                  : current,
            );
            if (parsed.pendingApprovals > 0) {
              void queryClient.invalidateQueries({
                queryKey: queryKeys.inbox("unread"),
              });
            }
            break;
          }
          case "inbox_count": {
            if (parsed.unread > 0) {
              void queryClient.invalidateQueries({
                queryKey: queryKeys.inbox("unread"),
              });
            }
            break;
          }
          case "notice": {
            if (parsed.level === "error") {
              toast.error(parsed.message);
            }
            break;
          }
          case "presence":
          case "pong":
            break;
        }
      };
      socket.onclose = () => {
        useLiveStore.setState({ connected: false });
        if (closed) {
          return;
        }
        attempts += 1;
        const delay = Math.min(8_000, 500 * 2 ** Math.min(attempts, 4));
        retryTimer = window.setTimeout(connect, delay);
      };
      socket.onerror = () => {
        socket?.close();
      };
    };

    connect();
    return () => {
      closed = true;
      if (retryTimer !== undefined) {
        window.clearTimeout(retryTimer);
      }
      socket?.close();
      useLiveStore.setState({ connected: false });
    };
  }, [threadId, queryClient]);
}
