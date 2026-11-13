import { WebsocketProvider } from "y-websocket";
import * as Y from "yjs";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

/**
 * Collaborative per-thread drafts over the y-protocols WebSocket in
 * `crates/server/src/drafts.rs`.
 *
 * One Y.Doc per open thread, shared by everyone:
 *
 *   Y.Doc
 *   ├─ XmlFragment "content"  y-prosemirror document edited by every Tiptap
 *   │                         instance (see `useComposerEditor`)
 *   └─ Map "editors"          user id → last local edit timestamp; the server
 *                             turns those ids into the prompt's authors
 *
 * The server reads the plain text of `content` when a draft is sent and clears
 * both roots in the same transaction, so every open composer empties at once.
 * Real-time text and carets are rendered by `@tiptap/extension-collaboration`
 * and `@tiptap/extension-collaboration-caret`; this module only owns the
 * document, the socket and presence state.
 */

/** Root type holding the shared composer document; mirrors `threadbase_draft`. */
export const DRAFT_FIELD = "content";
/** Root map of user ids that edited the pending draft. */
export const EDITORS_MAP = "editors";
/** Byte cap the server enforces on the shared draft. */
export const MAX_THREAD_DRAFT_BYTES = 200 * 1024;

export type DraftStatus = "idle" | "connecting" | "connected" | "disconnected";

export interface DraftAuthor {
  id: string;
  name: string;
}

/** A peer currently focused on the draft. */
export interface DraftEditor {
  id: string;
  name: string;
  color: string;
  typing: boolean;
}

export interface DraftSession {
  /** A document exists for this thread; false in the design-preview composer. */
  active: boolean;
  /** The provider finished its first sync round-trip. */
  synced: boolean;
  status: DraftStatus;
  connected: boolean;
  /** Shared CRDT document; bind the Tiptap collaboration extension to this. */
  document: Y.Doc | null;
  /** Awareness source for collaboration carets; null while connecting. */
  provider: WebsocketProvider | null;
  /** Peers currently editing the draft. */
  remoteEditors: DraftEditor[];
  /** Publish focus/typing presence for this author. */
  setEditing: (active: boolean) => void;
}

interface DraftSessionState {
  synced: boolean;
  status: DraftStatus;
  connected: boolean;
  remoteEditors: DraftEditor[];
}

const IDLE_STATE: DraftSessionState = {
  synced: false,
  status: "idle",
  connected: false,
  remoteEditors: [],
};

/** Mirrors `user_color` in crates/shared/src/lib.rs: the same eight-color
 *  palette and 64-bit FNV-1a hash, so a user has one color everywhere. */
// Zed's collaborator `players` palette (One Dark), shared with
// `user_color` in crates/shared/src/lib.rs.
const USER_COLORS = [
  "#74ade8",
  "#be5046",
  "#bf956a",
  "#b477cf",
  "#6eb4bf",
  "#d07277",
  "#dec184",
  "#a1c181",
] as const;

const textEncoder = new TextEncoder();

export function userColor(userId: string): string {
  let hash = 0xcbf29ce484222325n;
  for (const byte of textEncoder.encode(userId)) {
    hash ^= BigInt(byte);
    hash = BigInt.asUintN(64, hash * 0x0100000001b3n);
  }
  return USER_COLORS[Number(hash % BigInt(USER_COLORS.length))];
}

function draftSocketUrl(): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/ws/drafts`;
}

interface AwarenessUser {
  id?: unknown;
  displayName?: unknown;
  color?: unknown;
}

function readEditors(
  awareness: WebsocketProvider["awareness"],
  selfClientId: number,
): DraftEditor[] {
  const editors: DraftEditor[] = [];
  awareness.getStates().forEach((state, clientId) => {
    if (clientId === selfClientId) return;
    const user = state.user as AwarenessUser | undefined;
    const typing = state.typing === true;
    // Focus is announced with `typing`; peers browsing can stay silent.
    if (!typing) return;
    const id = typeof user?.id === "string" ? user.id : `client-${clientId}`;
    editors.push({
      id,
      name: typeof user?.displayName === "string" ? user.displayName : "Teammate",
      color: typeof user?.color === "string" ? user.color : userColor(id),
      typing,
    });
  });
  editors.sort((left, right) => (left.id < right.id ? -1 : 1));
  return editors;
}

export function useDraftSession(
  threadId: string | null,
  user: DraftAuthor | null,
): DraftSession {
  const document = useMemo(() => (threadId ? new Y.Doc() : null), [threadId]);
  const [provider, setProvider] = useState<WebsocketProvider | null>(null);
  const [state, setState] = useState<DraftSessionState>(IDLE_STATE);
  const editingRef = useRef<((active: boolean) => void) | null>(null);

  const userId = user?.id ?? null;
  const userName = user?.name ?? null;

  // The document outlives provider reconnects but not a thread switch.
  useEffect(
    () => () => {
      document?.destroy();
    },
    [document],
  );

  useEffect(() => {
    editingRef.current = null;
    if (!document || !threadId || !userId || !userName) {
      setProvider(null);
      setState(IDLE_STATE);
      return;
    }

    let disposed = false;
    let synced = false;
    let status: DraftStatus = "connecting";
    let lastSignature = "";

    const next = new WebsocketProvider(draftSocketUrl(), threadId, document, {
      connect: true,
    });

    const publish = () => {
      if (disposed) return;
      const snapshot: DraftSessionState = {
        synced,
        status,
        connected: status === "connected",
        remoteEditors: readEditors(next.awareness, document.clientID),
      };
      const signature = JSON.stringify(snapshot);
      if (signature === lastSignature) return;
      lastSignature = signature;
      setState(snapshot);
    };

    const onSync = (isSynced: boolean) => {
      synced = isSynced;
      publish();
    };
    const onStatus = (event: { status: DraftStatus }) => {
      status = event.status;
      publish();
    };

    next.awareness.setLocalStateField("user", {
      id: userId,
      displayName: userName,
      color: userColor(userId),
    });
    next.on("sync", onSync);
    next.on("status", onStatus);
    next.awareness.on("change", publish);
    editingRef.current = (active: boolean) => {
      const current = next.awareness.getLocalState() ?? {};
      next.awareness.setLocalState({ ...current, typing: active });
    };
    setProvider(next);
    publish();

    return () => {
      disposed = true;
      editingRef.current = null;
      next.awareness.off("change", publish);
      next.off("sync", onSync);
      next.off("status", onStatus);
      next.destroy();
      setProvider(null);
      setState(IDLE_STATE);
    };
  }, [document, threadId, userId, userName]);

  const setEditing = useCallback((active: boolean) => {
    editingRef.current?.(active);
  }, []);

  return {
    active: document !== null,
    document,
    provider,
    setEditing,
    ...state,
  };
}
