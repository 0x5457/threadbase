import type { ThreadEvent, ToolCallData } from "../api";
import type { Usage } from "../api/types/Usage";
import type { AttachmentDto } from "../api/types/AttachmentDto";
import type { UserBrief } from "../api/types/UserBrief";

/**
 * Wire shape added after the generated bindings: the diagnostics channel for
 * adapter stderr. Kept local because generated `ThreadEvent` bindings are
 * regenerated separately.
 */
type AgentLogPayload = {
  type: "agent_log";
  level: string;
  text: string;
};

export type DiagnosticLine = {
  /** Event sequence, unique within the thread. */
  id: number;
  level: "info" | "error";
  text: string;
  ts: number;
};

/** Keep the log bounded; the oldest diagnostics fall off first. */
const DIAGNOSTIC_LIMIT = 500;

export type ViewItem =
  | {
      kind: "message";
      id: string;
      role: "user" | "agent";
      name: string;
      avatarUrl: string | null;
      color: string;
      text: string;
      ts: number;
      streaming: boolean;
      attachments: AttachmentDto[];
    }
  | {
      kind: "thinking";
      id: string;
      text: string;
      ts: number;
      count: number;
      tools: number;
    }
  | { kind: "tool"; call: ToolCallData; ts: number; startedAt: number }
  | { kind: "notice"; level: "info" | "error"; text: string; ts: number }
  | { kind: "turn"; label: string; ts: number };

function displayName(
  user: UserBrief | null | undefined,
  fallback: string,
) {
  return user?.displayName || user?.username || fallback;
}

/**
 * Folding state for one thread's event log.
 *
 * The log is append-only, so the view is kept as an accumulator and each new
 * batch only folds the events after `consumed`. Rebuilding from event zero on
 * every token chunk is what made long, actively streaming threads peg the
 * main thread: an 80k-event thought log was re-string-concatenated dozens of
 * times per second.
 */
export interface ThreadViewState {
  items: ViewItem[];
  diagnostics: DiagnosticLine[];
  usage: Usage | null;
  /** Fallback display name for agent messages; part of the fold identity. */
  agentName: string;
  /** Number of events folded so far. */
  consumed: number;
  /** Last folded event, used to verify the array is still a prefix. */
  lastEvent: ThreadEvent | null;
  messageRows: Map<string, number>;
  toolRows: Map<string, number>;
  thinkingRows: Map<string, number>;
  /** Message ids currently streaming, so turn_ended avoids a full scan. */
  streamingRows: Set<string>;
}

export function createThreadView(agentName = "Agent"): ThreadViewState {
  return {
    items: [],
    diagnostics: [],
    usage: null,
    agentName,
    consumed: 0,
    lastEvent: null,
    messageRows: new Map(),
    toolRows: new Map(),
    thinkingRows: new Map(),
    streamingRows: new Set(),
  };
}

/**
 * Folds only the events after the previous state's cursor.
 *
 * The previous `items` array is copied once per batch (the block count is
 * small even when the event count is huge), while unchanged block objects keep
 * their identity so memoized rows can skip re-rendering. When the incoming
 * array is not an extension of the folded prefix (thread switch, resync) the
 * view is rebuilt from scratch.
 */
export function extendThreadView(
  state: ThreadViewState,
  events: ThreadEvent[],
  agentName = state.agentName,
): ThreadViewState {
  const sameAgent = state.agentName === agentName;
  const last = events[events.length - 1] ?? null;

  if (sameAgent && state.consumed === events.length && state.lastEvent === last) {
    return state;
  }

  const canExtend =
    sameAgent &&
    state.consumed > 0 &&
    events.length >= state.consumed &&
    state.lastEvent !== null &&
    events[state.consumed - 1] === state.lastEvent;

  const start = canExtend ? state.consumed : 0;
  // Copy the block list lazily: a batch made only of subagent events advances
  // the fold cursor without touching a single rendered row, so it should not
  // allocate a new array (or invalidate every memoized row).
  let items = start === 0 ? [] : state.items;
  let itemsOwned = start === 0;
  const ownItems = () => {
    if (itemsOwned) return;
    items = items.slice();
    itemsOwned = true;
  };
  let diagnostics = start === 0 ? [] : state.diagnostics;
  let usage = start === 0 ? null : state.usage;
  const messageRows = start === 0 ? new Map<string, number>() : state.messageRows;
  const toolRows = start === 0 ? new Map<string, number>() : state.toolRows;
  const thinkingRows = start === 0 ? new Map<string, number>() : state.thinkingRows;
  const streamingRows = start === 0 ? new Set<string>() : state.streamingRows;

  const upsertMessage = (
    id: string,
    role: "user" | "agent",
    name: string,
    avatarUrl: string | null,
    color: string,
    chunk: string,
    ts: number,
    streaming: boolean,
    attachments: AttachmentDto[] = [],
  ) => {
    const existing = messageRows.get(id);
    if (existing === undefined) {
      ownItems();
      messageRows.set(id, items.length);
      items.push({
        kind: "message",
        id,
        role,
        name,
        avatarUrl,
        color,
        text: chunk,
        ts,
        streaming,
        attachments,
      });
      if (streaming) streamingRows.add(id);
      return;
    }
    const row = items[existing];
    if (row.kind !== "message") return;
    ownItems();
    items[existing] = {
      ...row,
      text:
        chunk.startsWith(row.text) || row.text === ""
          ? chunk
          : row.text + chunk,
      streaming,
      ts: row.ts,
      attachments: attachments.length ? attachments : row.attachments,
    };
    if (streaming) streamingRows.add(id);
    else streamingRows.delete(id);
  };

  for (let index = start; index < events.length; index += 1) {
    const event = events[index];
    // Subagent activity is rendered next to the thread, not inside its
    // transcript; skipping it here also means no filtered copy of the log.
    if (event.subagentId) continue;
    const payload = event.payload as ThreadEvent["payload"] | AgentLogPayload;
    if (payload.type === "agent_log") {
      diagnostics = [
        ...diagnostics,
        {
          id: event.seq,
          level: payload.level === "error" ? "error" : "info",
          text: String(payload.text ?? ""),
          ts: event.ts,
        },
      ];
      if (diagnostics.length > DIAGNOSTIC_LIMIT) {
        diagnostics = diagnostics.slice(diagnostics.length - DIAGNOSTIC_LIMIT);
      }
      continue;
    }
    switch (payload.type) {
      case "turn_started": {
        const author = payload.author;
        const authors = payload.authors;
        const name =
          authors.length > 1
            ? authors
                .map((item) => displayName(item, "Teammate"))
                .join(", ")
            : displayName(author, "You");
        const prompt = String(payload.prompt ?? "");
        const reviewMarker = "\n\nReview comments to address:\n";
        const visiblePrompt = prompt.includes(reviewMarker)
          ? prompt.slice(0, prompt.indexOf(reviewMarker))
          : prompt;
        upsertMessage(
          String(payload.messageId),
          "user",
          name,
          author.avatarUrl ?? null,
          author.color,
          visiblePrompt,
          event.ts,
          false,
          payload.attachments ?? [],
        );
        break;
      }
      case "message_delta": {
        const author = payload.author;
        upsertMessage(
          String(payload.messageId),
          payload.role === "user" ? "user" : "agent",
          displayName(author, payload.role === "user" ? "You" : agentName),
          author?.avatarUrl ?? null,
          author?.color ?? "#878a98",
          String(payload.text ?? ""),
          event.ts,
          true,
        );
        break;
      }
      case "message_completed": {
        const message = payload.message;
        upsertMessage(
          message.id,
          message.role === "user" ? "user" : "agent",
          displayName(
            message.author,
            message.role === "user" ? "You" : agentName,
          ),
          message.author?.avatarUrl ?? null,
          message.author?.color ?? "#878a98",
          message.text,
          message.createdAt,
          false,
          message.attachments ?? [],
        );
        break;
      }
      case "thinking_delta": {
        const id = String(payload.messageId);
        const text = String(payload.text ?? "");
        const existing = thinkingRows.get(id);
        if (existing === undefined) {
          // Consecutive thoughts read as one line ("Thought 3 times") instead
          // of repeating the same row for every internal step.
          const previous = items[items.length - 1];
          if (previous?.kind === "thinking") {
            ownItems();
            thinkingRows.set(id, items.length - 1);
            items[items.length - 1] = {
              ...previous,
              id,
              text,
              ts: event.ts,
              count: previous.count + 1,
              tools: previous.tools,
            };
            break;
          }
          ownItems();
          thinkingRows.set(id, items.length);
          items.push({ kind: "thinking", id, text, ts: event.ts, count: 1, tools: 0 });
        } else {
          const row = items[existing];
          if (row.kind === "thinking") {
            ownItems();
            items[existing] = {
              ...row,
              text:
                text.startsWith(row.text) || row.text === ""
                  ? text
                  : row.text + text,
            };
          }
        }
        break;
      }
      case "tool_call":
      case "tool_call_update": {
        const call = payload.call as ToolCallData;
        const existing = toolRows.get(call.id);
        if (existing === undefined) {
          const lastItem = items[items.length - 1];
          if (lastItem?.kind === "thinking") {
            ownItems();
            items[items.length - 1] = {
              ...lastItem,
              tools: lastItem.tools + 1,
            };
          }
          ownItems();
          toolRows.set(call.id, items.length);
          items.push({ kind: "tool", call, ts: event.ts, startedAt: event.ts });
        } else {
          const row = items[existing];
          if (row.kind === "tool") {
            ownItems();
            items[existing] = { ...row, call, ts: event.ts };
          }
        }
        break;
      }
      case "turn_error": {
        ownItems();
        items.push({
          kind: "notice",
          level: "error",
          text: String(payload.message ?? "The turn failed."),
          ts: event.ts,
        });
        break;
      }
      case "process_exited": {
        ownItems();
        items.push({
          kind: "notice",
          level: "error",
          text: String(payload.message ?? "The agent process exited."),
          ts: event.ts,
        });
        break;
      }
      case "system_notice": {
        ownItems();
        items.push({
          kind: "notice",
          level: payload.level === "error" ? "error" : "info",
          text: String(payload.message ?? ""),
          ts: event.ts,
        });
        break;
      }
      case "agent_message": {
        const direction = payload.direction === "sent" ? "to" : "from";
        ownItems();
        items.push({
          kind: "notice",
          level: "info",
          text: `Agent message ${direction} ${String(payload[direction] ?? "agent")}: ${String(payload.text ?? "")}`,
          ts: event.ts,
        });
        break;
      }
      case "usage_updated": {
        usage = payload.usage;
        break;
      }
      case "turn_ended": {
        if (streamingRows.size > 0) ownItems();
        for (const id of streamingRows) {
          const streamingIndex = messageRows.get(id);
          if (streamingIndex === undefined) continue;
          const row = items[streamingIndex];
          if (row?.kind === "message" && row.streaming) {
            items[streamingIndex] = { ...row, streaming: false };
          }
        }
        streamingRows.clear();
        const reason = String(payload.stopReason ?? "");
        if (reason && reason !== "end_turn") {
          ownItems();
          items.push({
            kind: "turn",
            label: reason.replaceAll("_", " "),
            ts: event.ts,
          });
        }
        break;
      }
      default:
        break;
    }
  }

  return {
    items,
    diagnostics,
    usage,
    agentName,
    consumed: events.length,
    lastEvent: last,
    messageRows,
    toolRows,
    thinkingRows,
    streamingRows,
  };
}
