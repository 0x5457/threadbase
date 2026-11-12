import { useCallback, useEffect, useRef, useState } from "react";
import type { ThreadEvent } from "../api";
import { emptyStore, mergeEvents, replaceEvents } from "./live/store";
import type { EventStore, LiveFocus, LivePresence, PresencePayload } from "./live/types";
import {
  DEFAULT_PANEL,
  EMPTY_EVENTS,
  EMPTY_PRESENCE,
} from "./live/types";
import type { LiveEventsPage, LiveThreadOptions, LiveThreadState } from "./live/types";
import {
  HEARTBEAT_MS,
  RECONNECT_STABLE_MS,
  SOCKET_CONNECTING,
  SOCKET_OPEN,
  backoffDelay,
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
  const socketRef = useRef<WebSocket | null>(null);
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
    let socket: WebSocket | null = null;
    let reconnectTimer: number | null = null;
    let heartbeatTimer: number | null = null;
    let attempt = 0;
    let catchUpRounds = 0;
    let openedAt = 0;
    let pingOutstanding = false;

    const publish = (next: EventStore) => {
      if (next === storeRef.current) return;
      storeRef.current = next;
      setEvents(next.ordered);
    };

    const sendSubscribe = () => {
      sendJson(socket, {
        type: "subscribe",
        threadId,
        afterSeq: storeRef.current.lastSeq,
      });
    };

    const applyPage = (page: LiveEventsPage) => {
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
          publish(mergeEvents(storeRef.current, [event]));
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

    const clearTimers = () => {
      if (reconnectTimer !== null) {
        window.clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      if (heartbeatTimer !== null) {
        window.clearInterval(heartbeatTimer);
        heartbeatTimer = null;
      }
    };

    const scheduleReconnect = () => {
      if (disposed || reconnectTimer !== null) return;
      const delay = backoffDelay(attempt);
      attempt += 1;
      reconnectTimer = window.setTimeout(() => {
        reconnectTimer = null;
        connect();
      }, delay);
    };

    const connect = () => {
      if (disposed) return;
      let next: WebSocket;
      try {
        next = new WebSocket(socketUrl());
      } catch (cause) {
        setError(
          errorMessage(cause, "Could not open the live connection."),
        );
        scheduleReconnect();
        return;
      }

      socket = next;
      socketRef.current = next;

      next.onopen = () => {
        openedAt = Date.now();
        catchUpRounds = 0;
        pingOutstanding = false;
        setConnected(true);
        setError(null);
        heartbeatTimer = window.setInterval(() => {
          // A ping that went unanswered for a full interval means the socket is
          // half-open (dead network, no close frame); drop it to reconnect.
          if (pingOutstanding) {
            next.close(4000, "heartbeat timeout");
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
        if (socket === next) socket = null;
        if (socketRef.current === next) socketRef.current = null;
        if (disposed) return;
        clearTimers();
        // Only a connection that stayed up resets the ladder, so a server that
        // accepts and immediately closes cannot turn this into a hot loop.
        if (openedAt !== 0 && Date.now() - openedAt >= RECONNECT_STABLE_MS) {
          attempt = 0;
        }
        openedAt = 0;
        pingOutstanding = false;
        setConnected(false);
        setPresence(EMPTY_PRESENCE);
        setError(
          event.reason !== ""
            ? event.reason
            : `Live connection closed (${event.code}); reconnecting.`,
        );
        scheduleReconnect();
      };

      // `onerror` always precedes `onclose`; reconnection is driven there.
    };

    connect();

    return () => {
      disposed = true;
      clearTimers();
      const closing = socket;
      socket = null;
      if (socketRef.current === closing) socketRef.current = null;
      if (closing === null) return;
      // Detach first: `connect` only re-enters from these callbacks.
      closing.onopen = null;
      closing.onmessage = null;
      closing.onerror = null;
      closing.onclose = null;
      // A handshake still in flight must be aborted, not just ignored.
      if (
        closing.readyState === SOCKET_CONNECTING ||
        closing.readyState === SOCKET_OPEN
      ) {
        closing.close(1000, "live thread released");
      }
    };
  }, [threadId, sendPresenceNow]);

  const sendPresence = useCallback(
    (focus: Partial<LiveFocus>) => {
      payloadRef.current = withFocusPatch(payloadRef.current, focus);
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


function withFocusPatch(
  payload: PresencePayload,
  patch: Partial<LiveFocus>,
): PresencePayload {
  const next: PresencePayload = { ...payload };
  if (patch.projectId !== undefined) next.projectId = patch.projectId;
  if (patch.threadId !== undefined) next.threadId = patch.threadId;
  if (patch.target !== undefined) next.target = patch.target;
  if (patch.panel !== undefined) next.panel = patch.panel;
  if (patch.typing !== undefined) next.typing = patch.typing;
  if (patch.hidden !== undefined) next.hidden = patch.hidden;
  return next;
}
