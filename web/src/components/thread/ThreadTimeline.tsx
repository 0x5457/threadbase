import {
  AlertTriangle,
  ArrowLeftRight,
  Brain,
  CheckCircle2,
  ChevronRight,
  CircleDot,
  FileSearch,
  FolderInput,
  GitBranch,
  Globe,
  ListChecks,
  MessageSquare,
  MessageSquareText,
  PencilLine,
  Search,
  ShieldAlert,
  Terminal,
  Trash2,
  Wrench,
  XCircle,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { CommentAnchor } from "@/api/types/CommentAnchor";
import type { ElicitationAction } from "@/api/types/ElicitationAction";
import type { ElicitationDto } from "@/api/types/ElicitationDto";
import type { EventEnvelope } from "@/api/types/EventEnvelope";
import type { MessageDto } from "@/api/types/MessageDto";
import type { PlanEntry } from "@/api/types/PlanEntry";
import type { ToolCallData } from "@/api/types/ToolCallData";
import type { ToolKind } from "@/api/types/ToolKind";
import {
  Avatar,
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Skeleton,
  Textarea,
} from "@/components/ui";
import { ElicitationCard } from "@/components/thread/ElicitationCard";
import { usePresenceStore } from "@/lib/presence";
import { formatTimestamp } from "@/lib/format";
import { cn } from "@/lib/utils";

interface MessageItem {
  kind: "message";
  key: string;
  messageId: string;
  seq?: number;
  role: "user" | "agent";
  text: string;
  author: MessageDto["author"];
  authors: MessageDto["authors"];
  createdAt?: number | bigint;
}

interface ToolItem {
  kind: "tool";
  key: string;
  call: ToolCallData;
}

interface ToolGroupItem {
  kind: "tool_group";
  key: string;
  calls: ToolCallData[];
}

interface PlanItem {
  kind: "plan";
  key: string;
  entries: PlanEntry[];
}

interface ElicitationItem {
  kind: "elicitation";
  key: string;
  elicitation: ElicitationDto;
  resolvedAction: ElicitationAction | null;
}

interface ApprovalItem {
  kind: "approval";
  key: string;
  approval: ApprovalDto;
  resolved: boolean;
  decision: string | null;
}

interface SubagentItem {
  kind: "subagent";
  key: string;
  subagent: import("@/api/types/SubagentDto").SubagentDto;
}

interface NoticeItem {
  kind: "notice";
  key: string;
  tone: "muted" | "danger" | "warning" | "info";
  label: string;
  text: string;
}

type TimelineItem =
  | MessageItem
  | ToolItem
  | ToolGroupItem
  | PlanItem
  | ElicitationItem
  | ApprovalItem
  | SubagentItem
  | NoticeItem;

const TOOL_ICONS: Record<ToolKind, typeof Wrench> = {
  read: FileSearch,
  edit: PencilLine,
  delete: Trash2,
  move: FolderInput,
  search: Search,
  execute: Terminal,
  think: Brain,
  fetch: Globe,
  other: Wrench,
};

function toolStatusVariant(
  status: ToolCallData["status"],
): "neutral" | "info" | "success" | "danger" {
  switch (status) {
    case "pending":
    case "in_progress":
      return "info";
    case "completed":
      return "success";
    case "failed":
      return "danger";
  }
}

function noticeForEvent(event: EventEnvelope): NoticeItem | null {
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
    case "agent_message":
      return {
        kind: "notice",
        key,
        tone: "info",
        label:
          payload.direction === "sent"
            ? `Sent agent message to ${payload.to}`
            : `Received agent message from ${payload.from}`,
        text: payload.text,
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
    default:
      return null;
  }
}

function buildTimeline(events: EventEnvelope[]): TimelineItem[] {
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
        authors: [payload.author],
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
  const activeSubagents = [...latestSubagents.values()].filter(
    (subagent) => subagent.status === "running" || subagent.status === "requires_action",
  );
  if (activeSubagents.length > 0) {
    grouped.push({
      kind: "notice",
      key: "subagents-running",
      tone: "info",
      label: `${activeSubagents.length} subagent${activeSubagents.length === 1 ? "" : "s"} still running`,
      text: activeSubagents.map((subagent) => subagent.title).join(", "),
    });
  }
  if (latestPlan && latestPlan.length > 0) {
    grouped.push({ kind: "plan", key: "plan-latest", entries: latestPlan });
  }
  return grouped;
}

function summarizeTools(calls: ToolCallData[]): string {
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

export function ThreadTimeline({
  events,
  approvals,
  loading = false,
  error,
  onRetry,
  onDecide,
  onRespondElicitation,
  onComment,
  onOpenSubagent,
}: ThreadTimelineProps) {
  const items = useMemo(() => buildTimeline(events), [events]);
  const setLocal = usePresenceStore((state) => state.setLocal);
  const [denyFor, setDenyFor] = useState<string | null>(null);
  const [denyNote, setDenyNote] = useState("");
  const approvalById = useMemo(
    () => new Map(approvals.map((approval) => [approval.id, approval])),
    [approvals],
  );

  const renderTool = (call: ToolCallData, key: string) => {
    const Icon = TOOL_ICONS[call.kind];
    return (
      <div
        key={key}
        className="flex gap-3"
        onClick={() =>
          setLocal({
            panel: "transcript",
            target: { kind: "tool_call", toolCallId: call.id },
          })
        }
      >
        <div className="w-7 shrink-0" />
        <div className="min-w-0 flex-1 overflow-hidden rounded-lg border border-border bg-panel">
          <div className="flex items-center gap-2 px-3 py-2">
            <Icon className="size-3.5 shrink-0 text-faint" aria-hidden />
            <span className="truncate text-[11.5px] font-medium text-fg">
              {call.title}
            </span>
            {onComment ? (
              <button
                type="button"
                className="ml-auto rounded p-0.5 text-faint hover:text-fg"
                aria-label="Comment on tool call"
                onClick={() => onComment({ kind: "tool_call", toolCallId: call.id })}
              >
                <MessageSquare className="size-3" aria-hidden />
              </button>
            ) : null}
            <Badge
              variant={toolStatusVariant(call.status)}
              size="sm"
              className={onComment ? "shrink-0" : "ml-auto shrink-0"}
            >
              {call.status.replace("_", " ")}
            </Badge>
          </div>
          {call.locations.length > 0 ? (
            <ul className="border-t border-border/70 px-3 py-1.5">
              {call.locations.map((location) => (
                <li
                  key={`${location.path}:${location.line ?? 0}`}
                  className="truncate font-mono text-[10px] text-faint"
                >
                  {location.path}
                  {location.line ? `:${location.line}` : ""}
                </li>
              ))}
            </ul>
          ) : null}
          {call.diff?.patch ? (
            <pre className="mt-0 max-h-64 overflow-auto whitespace-pre-wrap break-all border-t border-border/70 bg-canvas p-2 font-mono text-[10px] text-fg">
              {call.diff.patch}
            </pre>
          ) : null}
          {call.input || call.output ? (
            <details className="border-t border-border/70 px-3 py-1.5 text-[10.5px] text-muted">
              <summary className="cursor-pointer select-none text-faint hover:text-fg">
                Details
              </summary>
              {call.input ? (
                <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-canvas p-2 font-mono text-[10px] text-fg">
                  {call.input}
                </pre>
              ) : null}
              {call.output ? (
                <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap break-all rounded bg-canvas p-2 font-mono text-[10px] text-fg">
                  {call.output}
                </pre>
              ) : null}
            </details>
          ) : null}
        </div>
      </div>
    );
  };

  if (loading) {
    return (
      <div className="mx-auto w-full max-w-3xl space-y-4 px-5 py-6">
        <Skeleton className="h-14 w-4/5" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-14 w-3/5" />
        <Skeleton className="h-32 w-full" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="mx-auto w-full max-w-3xl px-5 py-10">
        <ErrorState
          error={error}
          title="Cannot load thread activity"
          onRetry={onRetry}
        />
      </div>
    );
  }

  if (items.length === 0) {
    return (
      <div className="mx-auto w-full max-w-3xl px-5 py-10">
        <EmptyState icon={MessageSquare} title="No activity yet" compact />
      </div>
    );
  }

  return (
    <div className="mx-auto w-full max-w-3xl px-5 py-5">
      <div className="space-y-3">
        {items.map((item) => {
          if (item.kind === "message") {
            const author = item.author;
            const person = author
              ? {
                  id: author.id,
                  displayName: author.displayName,
                  avatarUrl: author.avatarUrl,
                  color: author.color,
                }
              : {
                  id: "agent",
                  displayName: "Agent",
                };
            return (
              <div key={`event-wrap-${item.key}`}>
                {item.seq ? (
                  <span id={`event-${item.seq}`} className="sr-only" aria-hidden />
                ) : null}
              <div
                id={`message-${item.messageId}`}
                data-message-id={item.messageId}
                className="flex gap-3"
                onClick={() =>
                  setLocal({
                    panel: "transcript",
                    target: { kind: "message", messageId: item.messageId },
                  })
                }
              >
                <div className="flex w-7 shrink-0 justify-center pt-0.5">
                  <Avatar person={person} size="md" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="mb-1 flex items-center gap-2 text-[10.5px] text-faint">
                    <span className="font-medium text-muted">
                      {author?.displayName ?? (item.role === "agent" ? "Agent" : "User")}
                    </span>
                    {item.authors.length > 1 ? (
                      <span>+{item.authors.length - 1}</span>
                    ) : null}
                    {item.createdAt ? (
                      <span title={formatTimestamp(item.createdAt)}>
                        {formatTimestamp(item.createdAt)}
                      </span>
                    ) : null}
                    <button
                      type="button"
                      className="ml-auto rounded p-0.5 text-faint hover:text-fg"
                      aria-label="View comments"
                      onClick={() =>
                        window.dispatchEvent(
                          new CustomEvent("threadbase:open-comments"),
                        )
                      }
                    >
                      <MessageSquareText className="size-3" aria-hidden />
                    </button>
                    {onComment ? (
                      <button
                        type="button"
                        className="rounded p-0.5 text-faint hover:text-fg"
                        aria-label="Comment on message"
                        onClick={() =>
                          onComment({
                            kind: "message",
                            messageId: item.messageId,
                          })
                        }
                      >
                        <MessageSquare className="size-3" aria-hidden />
                      </button>
                    ) : null}
                  </div>
                  <div
                    className={cn(
                      "whitespace-pre-wrap break-words rounded-lg px-3 py-2 text-[12.5px] leading-relaxed",
                      item.role === "user"
                        ? "border border-border bg-surface text-fg"
                        : "text-fg",
                    )}
                  >
                    {item.text}
                  </div>
                </div>
              </div>
              </div>
            );
          }

          if (item.kind === "tool") {
            return renderTool(item.call, item.key);
          }

          if (item.kind === "tool_group") {
            if (item.calls.length === 1) {
              return renderTool(item.calls[0], item.key);
            }
            return (
              <div key={item.key} className="flex gap-3">
                <div className="w-7 shrink-0" />
                <details className="group min-w-0 flex-1 overflow-hidden rounded-lg border border-border bg-panel">
                  <summary className="flex cursor-pointer select-none items-center gap-2 px-3 py-2 text-[11.5px] text-muted hover:text-fg">
                    <ChevronRight className="size-3.5 shrink-0 text-faint transition-transform group-open:rotate-90" aria-hidden />
                    <span className="truncate">{summarizeTools(item.calls)}</span>
                    <Badge variant="neutral" size="sm" className="ml-auto shrink-0">
                      {item.calls.length} calls
                    </Badge>
                  </summary>
                  <div className="space-y-2 border-t border-border/70 p-2">
                    {item.calls.map((call, index) =>
                      renderTool(call, `${item.key}-${index}`),
                    )}
                  </div>
                </details>
              </div>
            );
          }

          if (item.kind === "elicitation") {
            return (
              <ElicitationCard
                key={item.key}
                elicitation={item.elicitation}
                resolvedAction={item.resolvedAction}
                onRespond={onRespondElicitation}
                onComment={onComment}
              />
            );
          }

          if (item.kind === "plan") {
            return (
              <div key={item.key} className="flex gap-3">
                <div className="w-7 shrink-0" />
                <div className="min-w-0 flex-1 rounded-lg border border-border bg-panel px-3 py-2">
                  <div className="flex items-center gap-2 text-[11.5px] font-medium text-fg">
                    <ListChecks className="size-3.5 text-faint" aria-hidden />
                    Plan
                  </div>
                  <ul className="mt-1.5 space-y-1">
                    {item.entries.map((entry) => (
                      <li key={entry.content} className="flex items-start gap-1.5 text-[11px] text-muted">
                        <span
                          className={cn(
                            "mt-1 size-1.5 shrink-0 rounded-full",
                            entry.status === "completed"
                              ? "bg-success"
                              : entry.status === "in_progress"
                                ? "bg-warning"
                                : "bg-border-strong",
                          )}
                        />
                        <span className={entry.status === "completed" ? "text-faint line-through" : ""}>
                          {entry.content}
                        </span>
                      </li>
                    ))}
                  </ul>
                </div>
              </div>
            );
          }

          if (item.kind === "subagent") {
            const subagent = item.subagent;
            const active =
              subagent.status === "running" ||
              subagent.status === "requires_action";
            return (
              <div key={item.key} className="flex gap-3">
                <div className="w-7 shrink-0" />
                <details className="min-w-0 flex-1 overflow-hidden rounded-lg border border-border bg-panel">
                  <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-2">
                    <GitBranch className="size-3.5 shrink-0 text-faint" aria-hidden />
                    <span className="truncate text-[11.5px] font-medium text-fg">
                      {subagent.title}
                    </span>
                    <Badge
                      size="sm"
                      variant={
                        subagent.status === "error"
                          ? "danger"
                          : subagent.status === "requires_action"
                            ? "warning"
                            : active
                              ? "info"
                              : "neutral"
                      }
                      className="ml-auto shrink-0"
                    >
                      {subagent.status.replace("_", " ")}
                    </Badge>
                    <ChevronRight className="size-3 shrink-0 text-faint transition-transform group-open:rotate-90" aria-hidden />
                  </summary>
                  <div className="space-y-1 border-t border-border/70 px-3 py-2 text-[10.5px] text-muted">
                    {subagent.description ? (
                      <p className="whitespace-pre-wrap text-fg">{subagent.description}</p>
                    ) : null}
                    {subagent.status === "error" ? (
                      <p className="flex items-center gap-1 text-danger">
                        <XCircle className="size-3" aria-hidden />
                        Stopped with an error
                      </p>
                    ) : null}
                    {subagent.cwd ? (
                      <p className="break-all font-mono text-[9.5px] text-faint">
                        {subagent.cwd}
                      </p>
                    ) : (
                      <p className="text-faint">No independent session</p>
                    )}
                    {subagent.usage ? (
                      <p className="text-faint">
                        {subagent.usage.totalTokens.toLocaleString()} tokens
                      </p>
                    ) : null}
                    {onOpenSubagent ? (
                      <Button
                        size="sm"
                        variant="ghost"
                        className="px-0"
                        onClick={() => onOpenSubagent(subagent.id)}
                      >
                        Open subagent
                      </Button>
                    ) : null}
                  </div>
                </details>
              </div>
            );
          }

          if (item.kind === "approval") {
            const approval = approvalById.get(item.approval.id) ?? item.approval;
            return (
              <div key={item.key} className="flex gap-3">
                <div className="w-7 shrink-0" />
                <div className="min-w-0 flex-1 overflow-hidden rounded-lg border border-warning/30 bg-warning/5">
                  <div className="flex items-center gap-2 px-3 py-2">
                    <ShieldAlert className="size-3.5 shrink-0 text-warning" />
                    <span className="text-[11.5px] font-medium text-fg">
                      {approval.title}
                    </span>
                    <Badge
                      variant={
                        item.resolved && approval.status !== "pending"
                          ? "neutral"
                          : "warning"
                      }
                      size="sm"
                      className="ml-auto"
                    >
                      {item.decision ?? approval.status}
                    </Badge>
                  </div>
                  <div className="space-y-1.5 border-t border-warning/20 px-3 py-2">
                    <p className="text-[10.5px] text-muted">
                      {approval.agent}
                      {approval.subagentId
                        ? ` · subagent ${approval.subagentId}`
                        : ""}
                    </p>
                    {approval.command ? (
                      <pre className="overflow-x-auto rounded bg-canvas px-2 py-1.5 font-mono text-[10.5px] text-fg">
                        {approval.command}
                      </pre>
                    ) : null}
                    {approval.cwd ? (
                      <p className="break-all font-mono text-[10px] text-faint">
                        cwd: {approval.cwd}
                      </p>
                    ) : null}
                    {approval.paths.length > 0 ? (
                      <p className="break-all font-mono text-[10px] text-faint">
                        {approval.paths.join(", ")}
                      </p>
                    ) : null}
                    {!item.resolved && approval.status === "pending" && onDecide ? (
                      denyFor === approval.id ? (
                        <div className="space-y-1.5 border-t border-warning/20 pt-2">
                          <Textarea
                            autoFocus
                            value={denyNote}
                            onChange={(event) => setDenyNote(event.target.value)}
                            placeholder="Why deny?"
                            rows={2}
                            className="text-[10.5px]"
                          />
                          <div className="flex gap-1.5">
                            <Button
                              size="sm"
                              variant="ghost"
                              onClick={() => {
                                setDenyFor(null);
                                setDenyNote("");
                              }}
                            >
                              Cancel
                            </Button>
                            <Button
                              size="sm"
                              variant="danger"
                              disabled={!denyNote.trim()}
                              onClick={() => {
                                onDecide(approval.id, "deny", denyNote.trim());
                                setDenyFor(null);
                                setDenyNote("");
                              }}
                            >
                              Deny with reason
                            </Button>
                          </div>
                        </div>
                      ) : (
                        <div className="flex flex-wrap gap-1.5 border-t border-warning/20 pt-2">
                          {(approval.options.length > 0
                            ? approval.options
                            : [
                                { id: "once", kind: "allow_once" as const, label: "Allow once" },
                                {
                                  id: "thread",
                                  kind: "allow_thread" as const,
                                  label: "Allow for thread",
                                },
                                { id: "deny", kind: "deny" as const, label: "Deny" },
                              ]
                          ).map((option) =>
                            option.kind === "deny" ? (
                              <Button
                                key={option.id}
                                size="sm"
                                variant="danger"
                                onClick={() => {
                                  setDenyFor(approval.id);
                                  setDenyNote("");
                                }}
                              >
                                {option.label}
                              </Button>
                            ) : (
                              <Button
                                key={option.id}
                                size="sm"
                                variant="secondary"
                                onClick={() => onDecide(approval.id, option.kind)}
                              >
                                {option.label}
                              </Button>
                            ),
                          )}
                        </div>
                      )
                    ) : null}
                  </div>
                </div>
              </div>
            );
          }

          const toneClass =
            item.tone === "danger"
              ? "border-danger/25 bg-danger/5 text-danger"
              : item.tone === "warning"
                ? "border-warning/25 bg-warning/5 text-warning"
                : item.tone === "info"
                  ? "border-info/25 bg-info/5 text-info"
                  : "border-border bg-panel text-faint";
          const NoticeIcon =
            item.tone === "danger"
              ? AlertTriangle
              : item.tone === "warning"
                ? ShieldAlert
                : item.tone === "info"
                  ? ArrowLeftRight
                  : CircleDot;
          return (
            <div key={item.key} className="flex gap-3">
              <div className="w-7 shrink-0" />
              <div
                className={cn(
                  "flex min-w-0 flex-1 items-start gap-2 rounded-lg border px-3 py-2",
                  toneClass,
                )}
              >
                <NoticeIcon className="mt-0.5 size-3.5 shrink-0" aria-hidden />
                <p className="whitespace-pre-wrap break-words text-[11px] leading-relaxed">
                  <span className="font-medium">{item.label}</span>
                  {item.text ? <span className="text-muted"> · {item.text}</span> : null}
                </p>
                <CheckCircle2 className="ml-auto size-3 shrink-0 opacity-40" aria-hidden />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
