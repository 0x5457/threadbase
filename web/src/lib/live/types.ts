import type { ThreadEvent } from "../../api";
import type { ConnectionPhase } from "../../api/types/ConnectionPhase";
import type { ThreadState } from "../../api/types/ThreadState";
import type { ThreadViewState } from "../thread-events";

export type LivePanel =
  | "transcript"
  | "changes"
  | "approvals"
  | "files"
  | "terminal";

export const DEFAULT_PANEL: LivePanel = "transcript";

/** `Anchor` in crates/shared/src/focus.rs, tagged by `kind`. */
export type LiveAnchor = { kind: string } & Record<string, unknown>;

/** `Focus` in crates/shared/src/focus.rs. */
export interface LiveFocus {
  userId: string;
  projectId: string;
  threadId: string | null;
  panel: LivePanel;
  target: LiveAnchor | null;
  typing: boolean;
  hidden: boolean;
  updatedAt: number;
}

export interface LivePresence {
  onlineUserIds: string[];
  focuses: LiveFocus[];
  projectId: string | null;
}

export interface LiveThreadOptions {
  /** Presence is project-scoped on the wire; without it no presence is sent. */
  projectId?: string;
  /** Hide this user's focus from other members. */
  hidden?: boolean;
  /** Fallback name used while the thread's agent metadata is still loading. */
  agentName?: string;
  onInboxCount?: (unread: number) => void;
}

export interface LiveThreadState {
  /** True once any event for this thread has arrived. */
  hasEvents: boolean;
  /**
   * Latest server projection for this thread (status, allowed actions, pending
   * counts). Null until the first frame lands.
   */
  threadState: ThreadState | null;
  /** Incrementally folded transcript; recomputed once per event batch. */
  view: ThreadViewState;
  /** Single lifecycle value for the socket: see the `connection` machine. */
  phase: ConnectionPhase;
  error: string | null;
  presence: LivePresence;
  /** The cold page did not reach the start of the log. */
  hasEarlier: boolean;
  /** Fetch the next older window; true when something was appended. */
  loadEarlier: () => Promise<boolean>;
  retry: () => void;
  sendPresence: (focus: Partial<PresencePayload>) => void;
}

/** Outgoing presence fields; `userId` and `updatedAt` are assigned by the server. */
export interface PresencePayload {
  projectId: string;
  target: LiveAnchor | null;
  threadId: string | null;
  panel: LivePanel;
  typing: boolean;
  hidden: boolean;
}

export type ClientMessage =
  | { type: "subscribe"; threadId: string; afterSeq: number }
  | ({ type: "presence" } & PresencePayload)
  | { type: "ping" };

export interface LiveEventsPage {
  threadId: string;
  events: ThreadEvent[];
  lastSeq: number;
  /** seq of the page's first event, for walking further back. */
  oldestSeq: number;
  /** The stored log has events before `oldestSeq`. */
  hasEarlier: boolean;
  resync: boolean;
}

export interface EventStore {
  bySeq: Map<number, ThreadEvent>;
  ordered: ThreadEvent[];
  /** Highest seq held; the cursor for `subscribe`. */
  lastSeq: number;
}

export const EMPTY_PRESENCE: LivePresence = {
  onlineUserIds: [],
  focuses: [],
  projectId: null,
};
