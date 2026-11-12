import { useCallback, useEffect, useRef, useState } from "react";
import { WebsocketProvider } from "y-websocket";
import * as Y from "yjs";

/**
 * Collaborative per-thread drafts over the y-protocols WebSocket in
 * `crates/server/src/drafts.rs`.
 *
 * Wire schema (must match `crates/draft/src/lib.rs`):
 *
 *   Y.Doc
 *   └─ Y.Array "blocks": Y.Map
 *      ├─ "id":        string  (crypto.randomUUID)
 *      ├─ "author":    string  (users.id — the Rust `briefs_by_ids` lookup)
 *      ├─ "createdAt": number  (Date.now(); yrs reads it as i64 when < 2^53)
 *      ├─ "text":      Y.Text  (`cast::<TextRef>()`; never a string)
 *      └─ "sentAt":    boolean (server-only tombstone flag)
 *
 * Rules the Rust reader imposes on this client:
 * - blocks are never removed from the array (deleting an array parent while a
 *   peer edits it leaves dangling updates); a send only empties `text` and
 *   sets `sentAt`, so pending = present and not `sentAt`.
 * - every write goes to an unsent block. Text written into a tombstoned block
 *   is invisible to `peek_blocks` and never reaches the agent.
 * - per-block and thread-wide UTF-8 byte caps are enforced here before the
 *   write: exceeding them makes the server drop the room and close sockets.
 *
 * Only the author's own block is mirrored into the composer editor; other
 * authors' pending blocks are rendered read-only. The Tiptap document is never
 * overwritten by remote text, so Tiptap's local undo history cannot touch a
 * teammate's characters.
 */

/** `MAX_BLOCK_BYTES` / `MAX_THREAD_DRAFT_BYTES` in crates/draft/src/lib.rs. */
const MAX_BLOCK_BYTES = 16 * 1024;
const MAX_THREAD_BYTES = 200 * 1024;
const BLOCKS_ARRAY = "blocks";

export type DraftStatus = "idle" | "connecting" | "connected" | "disconnected";

export interface DraftAuthor {
  id: string;
  name: string;
}

/** A non-empty, unsent block — the text `sendDraft` will deliver. */
export interface DraftPendingBlock {
  id: string;
  author: string;
  createdAt: number;
  text: string;
}

/** An awareness entry for a peer that is focused on a block. */
export interface DraftEditor {
  id: string;
  name: string;
  color: string;
  blockId: string | null;
  typing: boolean;
}

export interface DraftSession {
  /** A socket exists for this thread; false in the design-preview composer. */
  active: boolean;
  /** The initial sync round-trip finished; safe to send. */
  synced: boolean;
  status: DraftStatus;
  connected: boolean;
  /** Pending blocks written by other authors, in `(createdAt, id)` order. */
  remoteBlocks: DraftPendingBlock[];
  /** Peers currently focused on a block or typing. */
  remoteEditors: DraftEditor[];
  /** This author's pending text at the first sync, for editor restore. */
  restoredText: string | null;
  /** Mirror the editor's plain text into this author's block. */
  setMyText: (text: string) => void;
  /** Called after a successful send: start a fresh block for the next turn. */
  markSent: () => void;
  /** Publish focus/typing presence for this author. */
  setEditing: (active: boolean) => void;
}

type DraftSessionState = Omit<
  DraftSession,
  "setMyText" | "markSent" | "setEditing"
>;

interface BlockRecord {
  id: string;
  author: string;
  createdAt: number;
  text: string;
  sent: boolean;
  body: Y.Text | null;
}

const IDLE_STATE: DraftSessionState = {
  active: false,
  synced: false,
  status: "idle",
  connected: false,
  remoteBlocks: [],
  remoteEditors: [],
  restoredText: null,
};

/** Presence colors are per user id so every client renders a peer the same. */
const USER_COLORS = [
  "#f76707",
  "#1c7ed6",
  "#7048e8",
  "#2f9e44",
  "#f59f00",
  "#e64980",
];

export function userColor(userId: string): string {
  let hash = 0;
  for (let index = 0; index < userId.length; index += 1) {
    hash = (hash * 31 + userId.charCodeAt(index)) | 0;
  }
  return USER_COLORS[Math.abs(hash) % USER_COLORS.length];
}

function draftSocketUrl(): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/ws/drafts`;
}

const textEncoder = new TextEncoder();

function byteLength(value: string): number {
  return textEncoder.encode(value).length;
}

/** Trim to a UTF-8 byte budget without splitting a code point. */
function truncateToBytes(value: string, limit: number): string {
  if (byteLength(value) <= limit) return value;
  let size = 0;
  let end = 0;
  for (const char of value) {
    const charSize = byteLength(char);
    if (size + charSize > limit) break;
    size += charSize;
    end += char.length;
  }
  return value.slice(0, end);
}

function randomId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 12)}`;
}

function readBlocks(blocks: Y.Array<Y.Map<unknown>>): BlockRecord[] {
  const records: BlockRecord[] = [];
  blocks.forEach((block) => {
    const body = block.get("text");
    const id = block.get("id");
    const author = block.get("author");
    const createdAt = block.get("createdAt");
    const text = body instanceof Y.Text ? body.toString() : "";
    records.push({
      id: typeof id === "string" ? id : "",
      author: typeof author === "string" ? author : "",
      createdAt: typeof createdAt === "number" ? createdAt : 0,
      text,
      sent: block.has("sentAt"),
      body: body instanceof Y.Text ? body : null,
    });
  });
  return records;
}

/** Deterministic display order; the server sorts by `(createdAt, id)` too. */
function byOrder(left: BlockRecord, right: BlockRecord): number {
  if (left.createdAt !== right.createdAt) {
    return left.createdAt - right.createdAt;
  }
  return left.id < right.id ? -1 : left.id > right.id ? 1 : 0;
}

/** Apply `next` to `body` as one minimal diff transaction. */
function spliceText(doc: Y.Doc, body: Y.Text, next: string): void {
  const current = body.toString();
  if (current === next) return;

  let start = 0;
  const shortest = Math.min(current.length, next.length);
  while (start < shortest && current[start] === next[start]) start += 1;
  // Never cut a surrogate pair in half.
  if (start > 0 && isHighSurrogate(current.charCodeAt(start - 1))) start -= 1;

  let tailCurrent = current.length;
  let tailNext = next.length;
  while (tailCurrent > start && tailNext > start) {
    const previous = current.charCodeAt(tailCurrent - 1);
    const candidate = next.charCodeAt(tailNext - 1);
    if (previous !== candidate) break;
    if (isLowSurrogate(previous) && tailCurrent - 2 >= start && tailNext - 2 >= start) {
      if (
        isHighSurrogate(current.charCodeAt(tailCurrent - 2)) &&
        isHighSurrogate(next.charCodeAt(tailNext - 2))
      ) {
        tailCurrent -= 2;
        tailNext -= 2;
        continue;
      }
    }
    tailCurrent -= 1;
    tailNext -= 1;
  }

  const removed = tailCurrent - start;
  const inserted = next.slice(start, tailNext);
  doc.transact(() => {
    if (removed > 0) body.delete(start, removed);
    if (inserted.length > 0) body.insert(start, inserted);
  });
}

function isHighSurrogate(code: number): boolean {
  return code >= 0xd800 && code <= 0xdbff;
}

function isLowSurrogate(code: number): boolean {
  return code >= 0xdc00 && code <= 0xdfff;
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
    const blockId = typeof state.blockId === "string" ? state.blockId : null;
    const typing = state.typing === true;
    if (!blockId && !typing) return;
    const id = typeof user?.id === "string" ? user.id : `client-${clientId}`;
    editors.push({
      id,
      name: typeof user?.displayName === "string" ? user.displayName : "Teammate",
      color: typeof user?.color === "string" ? user.color : userColor(id),
      blockId,
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
  const [state, setState] = useState<DraftSessionState>(IDLE_STATE);
  const controller = useRef<{
    setMyText: (text: string) => void;
    markSent: () => void;
    setEditing: (active: boolean) => void;
  } | null>(null);

  const userId = user?.id ?? null;
  const userName = user?.name ?? null;

  useEffect(() => {
    controller.current = null;
    if (!threadId || !userId || !userName) {
      setState(IDLE_STATE);
      return;
    }

    const authorId = userId;
    setState(IDLE_STATE);
    const doc = new Y.Doc();
    const blocks = doc.getArray<Y.Map<unknown>>(BLOCKS_ARRAY);
    const provider = new WebsocketProvider(draftSocketUrl(), threadId, doc, {
      connect: true,
    });

    let disposed = false;
    let myBlockId: string | null = null;
    let restoredText: string | null = null;
    let status: DraftStatus = "connecting";
    let lastSignature = "";

    const createBlock = (): BlockRecord => {
      const map = new Y.Map<unknown>();
      const body = new Y.Text();
      const id = randomId();
      const createdAt = Date.now();
      doc.transact(() => {
        map.set("id", id);
        map.set("author", authorId);
        map.set("createdAt", createdAt);
        map.set("text", body);
        blocks.push([map]);
      });
      return { id, author: authorId, createdAt, text: "", sent: false, body };
    };

    /** The single block this tab edits. Extra empty own blocks are peers'. */
    const ensureMyBlock = (): BlockRecord | null => {
      const own = readBlocks(blocks).filter(
        (block) => block.author === authorId && !block.sent && block.body !== null,
      );
      if (myBlockId) {
        const current = own.find((block) => block.id === myBlockId);
        if (current) return current;
      }
      // After a reload the draft to continue is the author's non-empty block.
      const candidate = own.filter((block) => block.text.length > 0).sort(byOrder)[0];
      const target = candidate ?? createBlock();
      myBlockId = target.id;
      return target;
    };

    const publish = () => {
      if (disposed) return;
      const records = readBlocks(blocks);
      const pending = records
        .filter((block) => !block.sent && block.text.trim().length > 0)
        .sort(byOrder);
      const next: DraftSessionState = {
        active: true,
        synced: provider.synced,
        status,
        connected: status === "connected",
        remoteBlocks: pending
          .filter((block) => block.author !== authorId)
          .map(({ id, author, createdAt, text }) => ({
            id,
            author,
            createdAt,
            text,
          })),
        remoteEditors: readEditors(provider.awareness, doc.clientID),
        restoredText,
      };
      // Doc updates fire per keystroke; only re-render on visible changes.
      const signature = JSON.stringify(next);
      if (signature === lastSignature) return;
      lastSignature = signature;
      setState(next);
    };

    const setMyText = (text: string) => {
      const target = ensureMyBlock();
      if (!target?.body) return;
      const others = readBlocks(blocks).reduce(
        (total, block) =>
          block.id === target.id ? total : total + byteLength(block.text),
        0,
      );
      const budget = Math.max(
        0,
        Math.min(MAX_BLOCK_BYTES, MAX_THREAD_BYTES - others),
      );
      spliceText(doc, target.body, truncateToBytes(text, budget));
    };

    const markSent = () => {
      // The server tombstones every block it just sent. Allocate the next block
      // immediately instead of adopting one, so the editor's clear lands in a
      // live block even before the tombstone update arrives.
      myBlockId = createBlock().id;
      publish();
    };

    const setEditing = (active: boolean) => {
      const current = provider.awareness.getLocalState() ?? {};
      provider.awareness.setLocalState({
        ...current,
        blockId: active ? (myBlockId ?? null) : null,
        typing: active,
      });
    };

    const onSync = (synced: boolean) => {
      if (disposed) return;
      if (synced) {
        const mine = ensureMyBlock();
        if (restoredText === null && mine) {
          restoredText = mine.text;
        }
      }
      publish();
    };

    const onStatus = (event: { status: DraftStatus }) => {
      if (disposed) return;
      status = event.status;
      publish();
    };

    provider.awareness.setLocalStateField("user", {
      id: authorId,
      displayName: userName,
      color: userColor(authorId),
    });
    provider.on("sync", onSync);
    provider.on("status", onStatus);
    blocks.observeDeep(publish);
    provider.awareness.on("change", publish);
    controller.current = { setMyText, markSent, setEditing };
    publish();

    return () => {
      disposed = true;
      controller.current = null;
      blocks.unobserveDeep(publish);
      provider.awareness.off("change", publish);
      provider.off("sync", onSync);
      provider.off("status", onStatus);
      provider.destroy();
      doc.destroy();
    };
  }, [threadId, userId, userName]);

  const setMyText = useCallback((text: string) => {
    controller.current?.setMyText(text);
  }, []);
  const markSent = useCallback(() => {
    controller.current?.markSent();
  }, []);
  const setEditing = useCallback((active: boolean) => {
    controller.current?.setEditing(active);
  }, []);

  return { ...state, setMyText, markSent, setEditing };
}
