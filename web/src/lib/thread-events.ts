import type { ThreadEvent, ToolCallData } from "../api";
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
      color: string;
      text: string;
      ts: number;
      streaming: boolean;
    }
  | { kind: "thinking"; id: string; text: string; ts: number; count: number }
  | { kind: "tool"; call: ToolCallData; ts: number }
  | { kind: "notice"; level: "info" | "error"; text: string; ts: number }
  | { kind: "turn"; label: string; ts: number };

function displayName(
  user: UserBrief | null | undefined,
  fallback: string,
) {
  return user?.displayName || user?.username || fallback;
}

/**
 * Folds the append-only event log into the thread's document blocks. Deltas
 * are merged in place so streaming output updates the same message row.
 *
 * Diagnostics are returned separately: the timeline never renders them, and
 * the agent log only shows the newest {@link DIAGNOSTIC_LIMIT} lines.
 */
export function reduceEvents(
  events: ThreadEvent[],
  agentName = "Agent",
): { items: ViewItem[]; diagnostics: DiagnosticLine[] } {
  const items: ViewItem[] = [];
  const diagnostics: DiagnosticLine[] = [];
  const messageRows = new Map<string, number>();
  const toolRows = new Map<string, number>();
  const thinkingRows = new Map<string, number>();

  const upsertMessage = (
    id: string,
    role: "user" | "agent",
    name: string,
    color: string,
    chunk: string,
    ts: number,
    streaming: boolean,
  ) => {
    const existing = messageRows.get(id);
    if (existing === undefined) {
      messageRows.set(id, items.length);
      items.push({
        kind: "message",
        id,
        role,
        name,
        color,
        text: chunk,
        ts,
        streaming,
      });
      return;
    }
    const row = items[existing];
    if (row.kind !== "message") return;
    items[existing] = {
      ...row,
      text:
        chunk.startsWith(row.text) || row.text === ""
          ? chunk
          : row.text + chunk,
      streaming,
      ts,
    };
  };

  for (const event of events) {
    const payload = event.payload as ThreadEvent["payload"] | AgentLogPayload;
    if (payload.type === "agent_log") {
      diagnostics.push({
        id: event.seq,
        level: payload.level === "error" ? "error" : "info",
        text: String(payload.text ?? ""),
        ts: event.ts,
      });
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
          author.color,
          visiblePrompt,
          event.ts,
          false,
        );
        break;
      }
      case "message_delta": {
        const author = payload.author;
        upsertMessage(
          String(payload.messageId),
          payload.role === "user" ? "user" : "agent",
          displayName(author, payload.role === "user" ? "You" : agentName),
          author?.color ?? "#6f7276",
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
          message.author?.color ?? "#6f7276",
          message.text,
          message.createdAt,
          false,
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
            thinkingRows.set(id, items.length - 1);
            items[items.length - 1] = {
              ...previous,
              id,
              text,
              ts: event.ts,
              count: previous.count + 1,
            };
            break;
          }
          thinkingRows.set(id, items.length);
          items.push({ kind: "thinking", id, text, ts: event.ts, count: 1 });
        } else {
          const row = items[existing];
          if (row.kind === "thinking") {
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
          toolRows.set(call.id, items.length);
          items.push({ kind: "tool", call, ts: event.ts });
        } else {
          const row = items[existing];
          if (row.kind === "tool") {
            items[existing] = { ...row, call, ts: event.ts };
          }
        }
        break;
      }
      case "turn_error": {
        items.push({
          kind: "notice",
          level: "error",
          text: String(payload.message ?? "The turn failed."),
          ts: event.ts,
        });
        break;
      }
      case "process_exited": {
        items.push({
          kind: "notice",
          level: "error",
          text: String(payload.message ?? "The agent process exited."),
          ts: event.ts,
        });
        break;
      }
      case "system_notice": {
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
        items.push({
          kind: "notice",
          level: "info",
          text: `Agent message ${direction} ${String(payload[direction] ?? "agent")}: ${String(payload.text ?? "")}`,
          ts: event.ts,
        });
        break;
      }
      case "turn_ended": {
        for (let index = 0; index < items.length; index += 1) {
          const row = items[index];
          if (row.kind === "message" && row.streaming) {
            items[index] = { ...row, streaming: false };
          }
        }
        const reason = String(payload.stopReason ?? "");
        if (reason && reason !== "end_turn") {
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

  if (diagnostics.length > DIAGNOSTIC_LIMIT) {
    diagnostics.splice(0, diagnostics.length - DIAGNOSTIC_LIMIT);
  }

  return { items, diagnostics };
}
