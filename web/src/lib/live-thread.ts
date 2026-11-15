import { WebSocket as ReconnectingWebSocket } from "partysocket";
import { useCallback, useEffect, useRef, useState } from "react";
import type { ThreadEvent } from "../api";
import { getOlderEvents } from "../api";
import { emptyStore, mergeEvents, replaceEvents } from "./live/store";
import type {
  EventStore,
  LivePresence,
  PresencePayload,
} from "./live/types";
import type { ConnectionPhase } from "../api/types/ConnectionPhase";
import type { ThreadState } from "../api/types/ThreadState";
import {
  createThreadView,
  extendThreadView,
  type ThreadViewState,
} from "./thread-events";
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
  parseThreadState,
} from "./live/wire";

const EVENTS_PAGE_LIMIT = 500;
const MAX_CATCH_UP_ROUNDS = 20;
/**
 * Coalesce token deltas instead of rendering every animation frame. Streaming
 * text reads the same at 20 fps, and the main thread stays free for clicks,
 * scrolling and the changes panel between batches.
 */
const EVENT_FLUSH_MS = 50;

export function useLiveThread(
  threadId: string | null,
  options: LiveThreadOptions = {},
): LiveThreadState {
  const { projectId, hidden = false, onInboxCount, agentName = "Agent" } = options;

  const [events, setEvents] = useState<ThreadEvent[]>([]);
  const [initialView] = useState(() => createThreadView(agentName));
  const [view, setView] = useState<ThreadViewState>(initialView);
  const [phase, setPhase] = useState<ConnectionPhase>("idle");
  const [error, setError] = useState<string | null>(null);
  const [presence, setPresence] = useState<LivePresence>(EMPTY_PRESENCE);
  const [hasEarlier, setHasEarlier] = useState(false);
  const [threadState, setThreadState] = useState<ThreadState | null>(null);
  const threadStateRevision = useRef(-1);
  const oldestRef = useRef(0);
  const publishRef = useRef<((next: EventStore) => void) | null>(null);
  const loadingEarlier = useRef(false);

  const [initialStore] = useState(emptyStore);
  const storeRef = useRef<EventStore>(initialStore);
  const viewRef = useRef<ThreadViewState>(initialView);
  const agentNameRef = useRef(agentName);
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

  // The agent name arrives with the thread bundle, after the first events may
  // already have been folded. Rebuild once so fallback names catch up.
  useEffect(() => {
    if (agentNameRef.current === agentName) return;
    agentNameRef.current = agentName;
    const rebuilt = extendThreadView(
      createThreadView(agentName),
      storeRef.current.ordered,
      agentName,
    );
    viewRef.current = rebuilt;
    setView(rebuilt);
  }, [agentName]);

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
    const resetView = () => {
      const next = createThreadView(agentNameRef.current);
      viewRef.current = next;
      setView(next);
    };
    if (threadId === null) {
      storeRef.current = emptyStore();
      resetView();
      setEvents([]);
      setPhase("idle");
      setError(null);
      setThreadState(null);
      threadStateRevision.current = -1;
      return;
    }

    storeRef.current = emptyStore();
    resetView();
    setEvents([]);
    setPhase("loading");
    setError(null);
    setHasEarlier(false);
    oldestRef.current = 0;

    if (typeof WebSocket === "undefined") {
      // Server-rendered or exotic host without WebSocket: stay inert instead
      // of throwing from render.
      setPhase("failed");
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
      const nextView = extendThreadView(
        viewRef.current,
        next.ordered,
        agentNameRef.current,
      );
      if (nextView === viewRef.current) return;
      viewRef.current = nextView;
      setView(nextView);
    };
    publishRef.current = publish;

    // Token deltas can arrive hundreds of times per second. Coalescing them
    // into batches keeps the incremental fold and React render bounded.
    let pendingEvents: ThreadEvent[] = [];
    let flushTimer: number | null = null;
    let lastFlushAt = 0;
    const flush = () => {
      flushTimer = null;
      lastFlushAt = performance.now();
      if (pendingEvents.length === 0) return;
      const batch = pendingEvents;
      pendingEvents = [];
      publish(mergeEvents(storeRef.current, batch));
    };
    const queueEvent = (event: ThreadEvent) => {
      pendingEvents.push(event);
      if (flushTimer !== null) return;
      const delay = Math.max(
        0,
        EVENT_FLUSH_MS - (performance.now() - lastFlushAt),
      );
      flushTimer = window.setTimeout(flush, delay);
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
      const firstLoad =
        storeRef.current.ordered.length === 0 || page.resync;
      const next = page.resync
        ? replaceEvents(page.events)
        : mergeEvents(storeRef.current, page.events);
      publish(next);
      setPhase("live");
      if (firstLoad) {
        oldestRef.current = page.oldestSeq;
        setHasEarlier(page.oldestSeq > 1);
      }
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
        case "thread_state": {
          const snapshot = parseThreadState(message.state);
          // Revisions are monotonic; an out-of-order frame would flicker the
          // composer between enabled and disabled.
          if (snapshot === null || snapshot.threadId !== threadId) return;
          if (snapshot.revision < threadStateRevision.current) return;
          threadStateRevision.current = snapshot.revision;
          setThreadState(snapshot);
          return;
        }
        case "pong": {
          pingOutstanding = false;
          return;
        }
        default:
          return;
      }
    };

    const clearFlush = () => {
      if (flushTimer !== null) {
        window.clearTimeout(flushTimer);
        flushTimer = null;
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
      setPhase("failed");
      setError(errorMessage(cause, "Could not open the live connection."));
      return;
    }

    socket = next;
    socketRef.current = next;

    next.onopen = () => {
      catchUpRounds = 0;
      pingOutstanding = false;
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

    // Partysocket's WebSocket frame handler, not window.postMessage: the
    // server authenticated this session before it subscribed.
    // react-doctor-disable-next-line react-doctor/postmessage-origin-risk
    next.onmessage = (message: MessageEvent) => {
      if (typeof message.data === "string") handleServerMessage(message.data);
    };

    // Partysocket synthesizes a 1000 close after transport errors; keep a
    // distinct message so a dropped network does not read like a normal close.
    next.onerror = () => setError("Live connection lost; reconnecting.");

    next.onclose = (event: CloseEvent) => {
      if (disposed) return;
      flush();
      clearTimers();
      pingOutstanding = false;
      setPhase("reconnecting");
      setPresence(EMPTY_PRESENCE);
      setError(
        event.reason !== ""
          ? event.reason
          : event.code === 1000
            ? "Live connection lost; reconnecting."
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

  const loadEarlier = useCallback(async () => {
    const before = oldestRef.current;
    if (!threadId || before <= 1 || loadingEarlier.current) return false;
    loadingEarlier.current = true;
    try {
      const page = await getOlderEvents(threadId, before);
      if (storeThreadRef.current !== threadId) return false;
      if (page.events.length === 0) {
        setHasEarlier(false);
        return false;
      }
      publishRef.current?.(mergeEvents(storeRef.current, page.events));
      oldestRef.current = page.oldestSeq;
      setHasEarlier(page.hasMore);
      return true;
    } catch {
      return false;
    } finally {
      loadingEarlier.current = false;
    }
  }, [threadId]);

  return {
    events:
      threadId !== null && storeThreadRef.current === threadId
        ? events
        : EMPTY_EVENTS,
    threadState,
    view,
    phase,
    error,
    presence,
    hasEarlier,
    loadEarlier,
    sendPresence,
  };
}
