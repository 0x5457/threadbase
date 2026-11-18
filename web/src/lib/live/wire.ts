import type { ThreadEvent } from "../../api";
import type { ActorKind } from "../../api/types/ActorKind";
import type { Delivery } from "../../api/types/Delivery";
import type { ThreadState } from "../../api/types/ThreadState";
import type { ThreadStatus } from "../../api/types/ThreadStatus";
import type {
  LiveAnchor,
  LiveEventsPage,
  LiveFocus,
  LivePanel,
  LivePresence,
} from "./types";
import { DEFAULT_PANEL } from "./types";
import { isState } from "../machine";


export function asRecord(value: unknown): Record<string, unknown> | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

export function asString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

export function asNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function asStringArray(value: unknown): string[] | null {
  if (!Array.isArray(value)) return null;
  return value.filter((item): item is string => typeof item === "string");
}

/**
 * Parse one `thread_state` frame. Unknown statuses are rejected against the
 * published `thread` machine, so a stale client and a newer server fail closed
 * instead of rendering a state neither understands.
 */
export function parseThreadState(value: unknown): ThreadState | null {
  const record = asRecord(value);
  if (record === null) return null;
  const threadId = asString(record.threadId);
  const status = asString(record.status);
  const revision = asNumber(record.revision);
  const allowedActions = asStringArray(record.allowedActions);
  if (
    threadId === null ||
    status === null ||
    revision === null ||
    allowedActions === null ||
    !isState("thread", status)
  ) {
    return null;
  }
  const placement = asString(record.placement);
  const section = asString(record.section);
  return {
    threadId,
    revision,
    status: status as ThreadStatus,
    allowedActions,
    currentTurn: asString(record.currentTurn),
    pendingApprovals: asNumber(record.pendingApprovals) ?? 0,
    pendingElicitations: asNumber(record.pendingElicitations) ?? 0,
    queuedInputs: asNumber(record.queuedInputs) ?? 0,
    activeSubagents: asNumber(record.activeSubagents) ?? 0,
    placement: (placement ?? "active") as ThreadState["placement"],
    section: (section ?? "active") as ThreadState["section"],
    snoozeUntil: asNumber(record.snoozeUntil),
  };
}

function isPanel(value: string | null): value is LivePanel {
  return (
    value === "transcript" ||
    value === "changes" ||
    value === "approvals" ||
    value === "files" ||
    value === "terminal"
  );
}

function parseAnchor(value: unknown): LiveAnchor | null {
  const record = asRecord(value);
  if (record === null || typeof record.kind !== "string") return null;
  return record as LiveAnchor;
}

function parseFocus(value: unknown): LiveFocus | null {
  const record = asRecord(value);
  if (record === null) return null;
  const userId = asString(record.userId);
  const projectId = asString(record.projectId);
  if (userId === null || projectId === null) return null;
  const panel = asString(record.panel);
  return {
    userId,
    projectId,
    threadId: asString(record.threadId),
    panel: isPanel(panel) ? panel : DEFAULT_PANEL,
    target: parseAnchor(record.target),
    typing: record.typing === true,
    hidden: record.hidden === true,
    updatedAt: asNumber(record.updatedAt) ?? 0,
  };
}

export function parseThreadEvent(value: unknown): ThreadEvent | null {
  const record = asRecord(value);
  if (record === null) return null;
  const seq = asNumber(record.seq);
  const ts = asNumber(record.ts);
  const payload = asRecord(record.payload);
  if (seq === null || ts === null) return null;
  if (payload === null || typeof payload.type !== "string") return null;
  const actor = asRecord(record.actor);
  const kind = actor === null ? null : asString(actor.kind);
  const delivery = asString(record.delivery);
  return {
    seq,
    ts,
    threadId: asString(record.threadId) ?? "",
    actor: {
      kind: (kind === "user" || kind === "agent" ? kind : "system") as ActorKind,
      id: actor === null ? null : asString(actor.id),
      name: actor === null ? null : asString(actor.name),
    },
    origin: record.origin === "relay" ? "relay" : "local",
    subagentId: asString(record.subagentId),
    parentSubagentId: asString(record.parentSubagentId),
    toolCallId: asString(record.toolCallId),
    delivery: (delivery as Delivery | null) ?? null,
    payload: payload as ThreadEvent["payload"],
  };
}

export function parseEventsPage(value: unknown): LiveEventsPage | null {
  const record = asRecord(value);
  if (record === null) return null;
  const threadId = asString(record.threadId);
  const lastSeq = asNumber(record.lastSeq);
  if (threadId === null || lastSeq === null) return null;
  if (!Array.isArray(record.events)) return null;
  const events: ThreadEvent[] = [];
  for (const raw of record.events) {
    const event = parseThreadEvent(raw);
    if (event !== null) events.push(event);
  }
  return {
    threadId,
    events,
    lastSeq,
    // The server decides whether anything exists before this page: a rewound
    // thread's surviving log may start at a high sequence with no earlier
    // branch, so the first event's seq alone cannot answer it.
    oldestSeq: asNumber(record.oldestSeq) ?? events[0]?.seq ?? 0,
    hasEarlier: record.hasEarlier === true,
    resync: record.resync === true,
  };
}

export function parsePresence(message: Record<string, unknown>): LivePresence | null {
  const projectId = asString(message.projectId);
  if (projectId === null) return null;
  const focuses: LiveFocus[] = [];
  if (Array.isArray(message.focuses)) {
    for (const raw of message.focuses) {
      const focus = parseFocus(raw);
      if (focus !== null) focuses.push(focus);
    }
  }
  const onlineUserIds: string[] = [];
  if (Array.isArray(message.onlineUserIds)) {
    for (const raw of message.onlineUserIds) {
      const userId = asString(raw);
      if (userId !== null) onlineUserIds.push(userId);
    }
  }
  return { onlineUserIds, focuses, projectId };
}

