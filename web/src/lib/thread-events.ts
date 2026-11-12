import type { ThreadEvent, ToolCallData } from "../api";
import type { UserBrief } from "../api/types/UserBrief";

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
  | { kind: "thinking"; id: string; text: string; ts: number }
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
 */
export function reduceEvents(
  events: ThreadEvent[],
  agentName = "Agent",
): ViewItem[] {
  const items: ViewItem[] = [];
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
    row.text =
      chunk.startsWith(row.text) || row.text === ""
        ? chunk
        : row.text + chunk;
    row.streaming = streaming;
    row.ts = ts;
  };

  for (const event of events) {
    const payload = event.payload;
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
          thinkingRows.set(id, items.length);
          items.push({ kind: "thinking", id, text, ts: event.ts });
        } else {
          const row = items[existing];
          if (row.kind === "thinking") {
            row.text =
              text.startsWith(row.text) || row.text === ""
                ? text
                : row.text + text;
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
            row.call = call;
            row.ts = event.ts;
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
        // Adapter install noise ("agent stderr: npm notice run npx") is
        // diagnostics, not part of the conversation.
        if (/^agent stderr: npm (notice|WARN)/.test(String(payload.message ?? ""))) {
          break;
        }
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
        for (const row of items) {
          if (row.kind === "message") row.streaming = false;
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

  return items;
}
