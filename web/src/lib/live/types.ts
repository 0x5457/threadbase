import type { ThreadEvent } from "../../api";

export type LivePanel = "transcript" | "changes" | "approvals" | "files";

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
  onInboxCount?: (unread: number) => void;
}

export interface LiveThreadState {
  events: ThreadEvent[];
  connected: boolean;
  error: string | null;
  presence: LivePresence;
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

export const EMPTY_EVENTS: ThreadEvent[] = [];
