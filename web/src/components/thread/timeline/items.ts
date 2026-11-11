/** Event → timeline item projection. Pure; no React. */
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { EventEnvelope } from "@/api/types/EventEnvelope";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { CommentAnchor } from "@/api/types/CommentAnchor";
import type { ElicitationAction } from "@/api/types/ElicitationAction";
import type { PlanEntry } from "@/api/types/PlanEntry";
import type { ToolCallData } from "@/api/types/ToolCallData";
import type { NoticeItem, TimelineItem } from "./types";

export function noticeForEvent(event: EventEnvelope): NoticeItem | null {
  const payload = event.payload;
  const key = `notice-${String(event.seq)}`;
  switch (payload.type) {
    case "turn_ended":
      return {
        kind: "notice",
        key,
        tone: "muted",
        label: "Turn ended",
        text: payload.stopReason,
      };
    case "turn_error":
      return {
        kind: "notice",
        key,
        tone: "danger",
        label: "Turn failed",
        text: payload.message,
      };
    case "process_exited":
      return {
        kind: "notice",
        key,
        tone: "danger",
        label: "Agent process exited",
        text: payload.message,
      };
    case "system_notice":
      return {
        kind: "notice",
        key,
        tone:
          payload.level === "error"
            ? "danger"
            : payload.level === "warning"
              ? "warning"
              : "info",
        label: "Notice",
        text: payload.message,
      };
    case "usage_updated":
      return {
        kind: "notice",
        key,
        tone: "muted",
        label: "Usage",
        text: `${payload.usage.totalTokens} tokens${
          payload.usage.contextUsed && payload.usage.contextSize
            ? ` · context ${payload.usage.contextUsed}/${payload.usage.contextSize}`
            : ""
        }`,
      };
    case "changes_updated":
      return {
        kind: "notice",
        key,
        tone: "muted",
        label: "Changes updated",
        text: `+${payload.stats.additions} −${payload.stats.deletions} across ${payload.stats.files} files`,
      };
    case "context_compacted":
      return {
        kind: "notice",
        key,
        tone: "muted",
        label: "Context compacted",
        text: payload.summary ?? "The agent compacted its context window.",
      };
    case "elicitation_requested":
      return {
        kind: "notice",
        key,
        tone: "warning",
        label: "Agent requested input",
        text: payload.elicitation.message,
      };
    case "permission_resolved":
      return {
        kind: "notice",
        key,
        tone: "muted",
        label: "Approval resolved",
        text: `${payload.decision}${payload.scope ? ` · ${payload.scope}` : ""}`,
      };
    case "permission_cancelled":
      return {
        kind: "notice",
        key,
        tone: "muted",
        label: "Approval cancelled",
        text: "The turn was cancelled",
      };
    default:
      return null;
  }
}

export function buildTimeline(events: EventEnvelope[], activeSubagentCount = 0): TimelineItem[] {
  const sorted = [...events].sort(
    (left, right) => Number(left.seq) - Number(right.seq),
  );
  const latestCalls = new Map<string, ToolCallData>();
  const latestSubagents = new Map<string, import("@/api/types/SubagentDto").SubagentDto>();
  const subagentOrder: string[] = [];
  const resolvedApprovals = new Map<string, string>();
  const resolvedElicitations = new Map<string, ElicitationAction>();
  const startedMessages = new Set<string>();

  for (const event of sorted) {
    if (
      event.payload.type === "tool_call" ||
      event.payload.type === "tool_call_update"
    ) {
      latestCalls.set(event.payload.call.id, event.payload.call);
    }
    if (event.payload.type === "subagent_updated") {
      const subagent = event.payload.subagent;
      latestSubagents.set(subagent.id, subagent);
      if (!subagentOrder.includes(subagent.id)) {
        subagentOrder.push(subagent.id);
      }
    }
    if (event.payload.type === "permission_resolved") {
      resolvedApprovals.set(event.payload.approvalId, event.payload.decision);
    }
    if (event.payload.type === "permission_cancelled") {
      resolvedApprovals.set(event.payload.approvalId, "cancelled");
    }
    if (event.payload.type === "elicitation_resolved") {
      resolvedElicitations.set(
        event.payload.elicitationId,
        event.payload.action,
      );
    }
    if (event.payload.type === "turn_started") {
      startedMessages.add(event.payload.messageId);
    }
  }

  const items: TimelineItem[] = [];
  const seenTools = new Set<string>();
  const seenSubagents = new Set<string>();
  let latestPlan: PlanEntry[] | null = null;

  for (const event of sorted) {
    const payload = event.payload;
    if (payload.type === "turn_started") {
      items.push({
        kind: "message",
        key: `turn-${payload.turnId}`,
        messageId: payload.messageId,
        seq: Number(event.seq),
        role: "user",
        text: payload.prompt,
        author: payload.author,
        authors: payload.authors?.length ? payload.authors : [payload.author],
        createdAt: event.ts,
      });
      continue;
    }
    if (payload.type === "message_completed") {
      if (startedMessages.has(payload.message.id)) {
        continue;
      }
      items.push({
        kind: "message",
        key: `message-${payload.message.id}`,
        messageId: payload.message.id,
        seq: Number(event.seq),
        role: payload.message.role,
        text: payload.message.text,
        author: payload.message.author,
        authors: payload.message.authors,
        createdAt: payload.message.createdAt,
      });
      continue;
    }
    if (payload.type === "elicitation_requested") {
      items.push({
        kind: "elicitation",
        key: `elicitation-${payload.elicitation.id}`,
        elicitation: payload.elicitation,
        resolvedAction:
          resolvedElicitations.get(payload.elicitation.id) ?? null,
      });
      continue;
    }
    if (payload.type === "tool_call" || payload.type === "tool_call_update") {
      const call = latestCalls.get(payload.call.id);
      if (!call || seenTools.has(call.id)) {
        continue;
      }
      seenTools.add(call.id);
      items.push({
        kind: "tool",
        key: `tool-${call.id}`,
        call,
      });
      continue;
    }
    if (payload.type === "plan_updated") {
      latestPlan = payload.entries;
      continue;
    }
    if (payload.type === "subagent_updated") {
      const subagent = latestSubagents.get(payload.subagent.id) ?? payload.subagent;
      if (!seenSubagents.has(subagent.id)) {
        seenSubagents.add(subagent.id);
        items.push({
          kind: "subagent",
          key: `subagent-${subagent.id}`,
          subagent,
        });
      }
      continue;
    }
    if (payload.type === "agent_message") {
      items.push({
        kind: "agent_message",
        key: `agent-message-${event.seq}`,
        direction: payload.direction,
        from: payload.from,
        to: payload.to,
        text: payload.text,
      });
      continue;
    }
    if (payload.type === "permission_requested") {
      const decision = resolvedApprovals.get(payload.approval.id) ?? null;
      items.push({
        kind: "approval",
        key: `approval-${payload.approval.id}`,
        approval: payload.approval,
        resolved: decision !== null || payload.approval.status !== "pending",
        decision,
      });
      continue;
    }
    const notice = noticeForEvent(event);
    if (notice) {
      items.push(notice);
    }
  }

  const grouped: TimelineItem[] = [];

  for (const item of items) {
    if (item.kind === "tool") {
      const last = grouped.at(-1);
      if (last?.kind === "tool_group") {
        last.calls.push(item.call);
      } else {
        grouped.push({
          kind: "tool_group",
          key: `tools-${item.key}`,
          calls: [item.call],
        });
      }
    } else {
      grouped.push(item);
    }
  }
  const derivedActiveSubagents = [...latestSubagents.values()].filter(
    (subagent) => subagent.status === "running" || subagent.status === "requires_action",
  );
  const activeSubagents =
    activeSubagentCount > 0 ? activeSubagentCount : derivedActiveSubagents.length;
  if (activeSubagents > 0) {
    grouped.push({
      kind: "notice",
      key: "subagents-running",
      tone: "info",
      label: `${activeSubagents} subagent${activeSubagents === 1 ? "" : "s"} still running`,
      text: derivedActiveSubagents.map((subagent) => subagent.title).join(", "),
    });
  }
  if (latestPlan && latestPlan.length > 0) {
    grouped.push({ kind: "plan", key: "plan-latest", entries: latestPlan });
  }
  return grouped;
}

export function summarizeTools(calls: ToolCallData[]): string {
  const counts = new Map<string, number>();
  for (const call of calls) {
    counts.set(call.kind, (counts.get(call.kind) ?? 0) + 1);
  }
  const parts: string[] = [];
  const commands = counts.get("execute") ?? 0;
  if (commands > 0) {
    parts.push(`Ran ${commands} command${commands === 1 ? "" : "s"}`);
  }
  const reads = (counts.get("read") ?? 0) + (counts.get("search") ?? 0);
  if (reads > 0) {
    parts.push(`read ${reads} file${reads === 1 ? "" : "s"}`);
  }
  const writes =
    (counts.get("edit") ?? 0) +
    (counts.get("move") ?? 0) +
    (counts.get("delete") ?? 0);
  if (writes > 0) {
    parts.push(`edited ${writes} file${writes === 1 ? "" : "s"}`);
  }
  const thoughts = counts.get("think") ?? 0;
  if (thoughts > 0) {
    parts.push("thought");
  }
  const fetches = counts.get("fetch") ?? 0;
  if (fetches > 0) {
    parts.push(`fetched ${fetches} URL${fetches === 1 ? "" : "s"}`);
  }
  const other = counts.get("other") ?? 0;
  if (other > 0) {
    parts.push(`called ${other} tool${other === 1 ? "" : "s"}`);
  }
  return parts.length > 0 ? parts.join(", ") : "Ran tools";
}

export interface ThreadTimelineProps {
  events: EventEnvelope[];
  approvals: ApprovalDto[];
  /** Active subagent count from the shared ThreadState projection. */
  activeSubagentCount?: number;
  loading?: boolean;
  error?: unknown;
  onRetry?: () => void;
  onDecide?: (
    approvalId: string,
    decision: ApprovalDecision,
    note?: string,
  ) => void;
  onRespondElicitation?: (
    elicitationId: string,
    action: ElicitationAction,
    values: Record<string, unknown>,
  ) => void;
  onComment?: (anchor: CommentAnchor) => void;
  onOpenSubagent?: (subagentId: string) => void;
}
