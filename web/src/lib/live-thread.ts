import { WebSocket as ReconnectingWebSocket } from "partysocket";
import { useCallback, useEffect, useRef, useState } from "react";
import type { ThreadEvent } from "../api";
import { emptyStore, mergeEvents, replaceEvents } from "./live/store";
import type { EventStore, LivePresence, PresencePayload } from "./live/types";
import {
  DEFAULT_PANEL,
  EMPTY_EVENTS,
  EMPTY_PRESENCE,
} from "./live/types";
import type { LiveEventsPage, LiveThreadOptions, LiveThreadState } from "./live/types";
import {
  HEARTBEAT_MS,
  RECONNECT_OPTIONS,
  errorMessage,
  sendJson,
  socketUrl,
} from "./live/transport";
import {
  asNumber,
  asRecord,
  asString,
  parseEventsPage,
  parsePresence,
  parseThreadEvent,
} from "./live/wire";

const EVENTS_PAGE_LIMIT = 500;
const MAX_CATCH_UP_ROUNDS = 20;

export type {
  LiveFocus,
  LivePanel,
  LivePresence,
  LiveThreadOptions,
  LiveThreadState,
} from "./live/types";


export function useLiveThread(
  threadId: string | null,
  options: LiveThreadOptions = {},
): LiveThreadState {
  const { projectId, hidden = false, onInboxCount } = options;

  const [events, setEvents] = useState<ThreadEvent[]>([]);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [presence, setPresence] = useState<LivePresence>(EMPTY_PRESENCE);

  const storeRef = useRef<EventStore>(emptyStore());
  /** Which thread `storeRef` belongs to; render never shows another thread's log. */
  const storeThreadRef = useRef<string | null>(null);
  const socketRef = useRef<ReconnectingWebSocket | null>(null);
  const onInboxCountRef = useRef(onInboxCount);
  const payloadRef = useRef<PresencePayload>({
    projectId: projectId ?? "",
    target: null,
    threadId,
    panel: DEFAULT_PANEL,
    typing: false,
    hidden,
  });

  useEffect(() => {
    onInboxCountRef.current = onInboxCount;
  }, [onInboxCount]);

  const sendPresenceNow = useCallback(() => {
    const payload = payloadRef.current;
    if (payload.projectId === "") return;
    sendJson(socketRef.current, { type: "presence", ...payload });
  }, []);

  // Track the base focus separately from the socket: a partial update can be
  // recorded before the socket is open and is replayed on the next open.
  useEffect(() => {
    const previous = payloadRef.current;
    payloadRef.current = {
      ...previous,
      projectId: projectId ?? previous.projectId,
      threadId,
      hidden,
    };
    sendPresenceNow();
  }, [projectId, threadId, hidden, sendPresenceNow]);

  useEffect(() => {
    storeThreadRef.current = threadId;
    if (threadId === null) {
      storeRef.current = emptyStore();
      setEvents([]);
      setConnected(false);
      setError(null);
      return;
    }

    storeRef.current = emptyStore();
    setEvents([]);
    setConnected(false);
    setError(null);

    if (typeof WebSocket === "undefined") {
      // Server-rendered or exotic host without WebSocket: stay inert instead
      // of throwing from render.
      setError("Live updates are unavailable: this browser has no WebSocket.");
      return;
    }

    let disposed = false;
    let socket: ReconnectingWebSocket | null = null;
    let heartbeatTimer: number | null = null;
    let catchUpRounds = 0;
    let pingOutstanding = false;

    const publish = (next: EventStore) => {
      if (next === storeRef.current) return;
      storeRef.current = next;
      setEvents(next.ordered);
    };

    // Token deltas arrive many times per frame. Buffering them keeps the
    // event-store merge and React render to at most once per animation frame.
    let pendingEvents: ThreadEvent[] = [];
    let flushFrame: number | null = null;
    const flush = () => {
      flushFrame = null;
      if (pendingEvents.length === 0) return;
      const batch = pendingEvents;
      pendingEvents = [];
      publish(mergeEvents(storeRef.current, batch));
    };
    const queueEvent = (event: ThreadEvent) => {
      pendingEvents.push(event);
      if (flushFrame !== null) return;
      if (typeof window.requestAnimationFrame === "function") {
        flushFrame = window.requestAnimationFrame(flush);
      } else {
        flush();
      }
    };

    const sendSubscribe = () => {
      sendJson(socket, {
        type: "subscribe",
        threadId,
        afterSeq: storeRef.current.lastSeq,
      });
    };

    const applyPage = (page: LiveEventsPage) => {
      flush();
      const next = page.resync
        ? replaceEvents(page.events)
        : mergeEvents(storeRef.current, page.events);
      publish(next);
      if (
        page.events.length >= EVENTS_PAGE_LIMIT &&
        next.lastSeq < page.lastSeq &&
        catchUpRounds < MAX_CATCH_UP_ROUNDS
      ) {
        catchUpRounds += 1;
        sendSubscribe();
      }
    };

    const handleServerMessage = (raw: string) => {
      let parsed: unknown;
      try {
        parsed = JSON.parse(raw);
      } catch {
        return;
      }
      const message = asRecord(parsed);
      if (message === null) return;
      switch (message.type) {
        case "events_page": {
          const page = parseEventsPage(message.page);
          // A page for another thread is a leftover from a previous
          // subscription; applying it would corrupt this thread's log.
          if (page === null || page.threadId !== threadId) return;
          applyPage(page);
          return;
        }
        case "event": {
          const envelope = asRecord(message.event);
          if (envelope === null) return;
          const eventThreadId = asString(envelope.threadId);
          if (eventThreadId !== null && eventThreadId !== threadId) return;
          const event = parseThreadEvent(envelope);
          if (event === null) return;
          queueEvent(event);
          return;
        }
        case "presence": {
          const snapshot = parsePresence(message);
          if (snapshot === null) return;
          // Presence snapshots are broadcast to every socket; keep the project
          // we are showing.
          const wanted = payloadRef.current.projectId;
          if (wanted !== "" && snapshot.projectId !== wanted) return;
          setPresence(snapshot);
          return;
        }
        case "inbox_count": {
          const unread = asNumber(message.unread);
          if (unread !== null) onInboxCountRef.current?.(unread);
          return;
        }
        case "notice": {
          const text = asString(message.message);
          if (text !== null && message.level === "error") setError(text);
          return;
        }
        case "pong": {
          pingOutstanding = false;
          return;
        }
        default:
          // "thread_state" and any future frame the hook does not use.
          return;
      }
    };

    const clearFlush = () => {
      if (flushFrame !== null) {
        window.cancelAnimationFrame(flushFrame);
        flushFrame = null;
      }
      pendingEvents = [];
    };

    const clearHeartbeat = () => {
      if (heartbeatTimer !== null) {
        window.clearInterval(heartbeatTimer);
        heartbeatTimer = null;
      }
    };

    const clearTimers = () => {
      clearFlush();
      clearHeartbeat();
    };

    let next: ReconnectingWebSocket;
    try {
      next = new ReconnectingWebSocket(
        socketUrl(),
        undefined,
        RECONNECT_OPTIONS,
      );
    } catch (cause) {
      setError(errorMessage(cause, "Could not open the live connection."));
      return;
    }

    socket = next;
    socketRef.current = next;

    next.onopen = () => {
      catchUpRounds = 0;
      pingOutstanding = false;
      setConnected(true);
      setError(null);
      clearHeartbeat();
      heartbeatTimer = window.setInterval(() => {
        // A ping that went unanswered for a full interval means the socket is
        // half-open (dead network, no close frame); reconnect to replace it.
        if (pingOutstanding) {
          next.reconnect(4000, "heartbeat timeout");
          return;
        }
        pingOutstanding = true;
        sendJson(next, { type: "ping" });
      }, HEARTBEAT_MS);
      sendSubscribe();
      sendPresenceNow();
    };

    next.onmessage = (message: MessageEvent) => {
      if (typeof message.data === "string") handleServerMessage(message.data);
    };

    next.onclose = (event: CloseEvent) => {
      if (disposed) return;
      flush();
      clearTimers();
      pingOutstanding = false;
      setConnected(false);
      setPresence(EMPTY_PRESENCE);
      setError(
        event.reason !== ""
          ? event.reason
          : `Live connection closed (${event.code}); reconnecting.`,
      );
    };

    return () => {
      disposed = true;
      clearTimers();
      const closing = socket;
      socket = null;
      if (socketRef.current === closing) socketRef.current = null;
      if (closing === null) return;
      // Detach first: partysocket only re-enters from these callbacks.
      closing.onopen = null;
      closing.onmessage = null;
      closing.onerror = null;
      closing.onclose = null;
      closing.close(1000, "live thread released");
    };
  }, [threadId, sendPresenceNow]);

  const sendPresence = useCallback(
    (focus: Partial<PresencePayload>) => {
      payloadRef.current = Object.assign({}, payloadRef.current, focus);
      sendPresenceNow();
    },
    [sendPresenceNow],
  );

  return {
    events:
      threadId !== null && storeThreadRef.current === threadId
        ? events
        : EMPTY_EVENTS,
    connected,
    error,
    presence,
    sendPresence,
  };
}
