import {
  AlertTriangle,
  ArrowLeftRight,
  Brain,
  CheckCircle2,
  CircleDot,
  FileSearch,
  FolderInput,
  Globe,
  MessageSquare,
  PencilLine,
  Search,
  ShieldAlert,
  Terminal,
  Trash2,
  Wrench,
} from "lucide-react";
import { useMemo } from "react";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { EventEnvelope } from "@/api/types/EventEnvelope";
import type { MessageDto } from "@/api/types/MessageDto";
import type { ToolCallData } from "@/api/types/ToolCallData";
import type { ToolKind } from "@/api/types/ToolKind";
import {
  Avatar,
  Badge,
  EmptyState,
  ErrorState,
  Skeleton,
} from "@/components/ui";
import { formatTimestamp } from "@/lib/format";
import { cn } from "@/lib/utils";

interface MessageItem {
  kind: "message";
  key: string;
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

interface ApprovalItem {
  kind: "approval";
  key: string;
  approval: ApprovalDto;
  resolved: boolean;
  decision: string | null;
}

interface NoticeItem {
  kind: "notice";
  key: string;
  tone: "muted" | "danger" | "warning" | "info";
  label: string;
  text: string;
}

type TimelineItem = MessageItem | ToolItem | ApprovalItem | NoticeItem;

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
  const resolvedApprovals = new Map<string, string>();
  const startedMessages = new Set<string>();

  for (const event of sorted) {
    if (
      event.payload.type === "tool_call" ||
      event.payload.type === "tool_call_update"
    ) {
      latestCalls.set(event.payload.call.id, event.payload.call);
    }
    if (event.payload.type === "permission_resolved") {
      resolvedApprovals.set(event.payload.approvalId, event.payload.decision);
    }
    if (event.payload.type === "turn_started") {
      startedMessages.add(event.payload.messageId);
    }
  }

  const items: TimelineItem[] = [];
  const seenTools = new Set<string>();

  for (const event of sorted) {
    const payload = event.payload;
    if (payload.type === "turn_started") {
      items.push({
        kind: "message",
        key: `turn-${payload.turnId}`,
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
        role: payload.message.role,
        text: payload.message.text,
        author: payload.message.author,
        authors: payload.message.authors,
        createdAt: payload.message.createdAt,
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

  return items;
}

export interface ThreadTimelineProps {
  events: EventEnvelope[];
  approvals: ApprovalDto[];
  loading?: boolean;
  error?: unknown;
  onRetry?: () => void;
}

export function ThreadTimeline({
  events,
  approvals,
  loading = false,
  error,
  onRetry,
}: ThreadTimelineProps) {
  const items = useMemo(() => buildTimeline(events), [events]);
  const approvalById = useMemo(
    () => new Map(approvals.map((approval) => [approval.id, approval])),
    [approvals],
  );

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
        <EmptyState
          icon={MessageSquare}
          title="No thread activity yet"
          description="Prompts, agent responses and tool calls appear here as the thread runs."
        />
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
              <div key={item.key} className="flex gap-3">
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
            );
          }

          if (item.kind === "tool") {
            const Icon = TOOL_ICONS[item.call.kind];
            return (
              <div key={item.key} className="flex gap-3">
                <div className="w-7 shrink-0" />
                <div className="min-w-0 flex-1 overflow-hidden rounded-lg border border-border bg-panel">
                  <div className="flex items-center gap-2 px-3 py-2">
                    <Icon className="size-3.5 shrink-0 text-faint" aria-hidden />
                    <span className="truncate text-[11.5px] font-medium text-fg">
                      {item.call.title}
                    </span>
                    <Badge
                      variant={toolStatusVariant(item.call.status)}
                      size="sm"
                      className="ml-auto shrink-0"
                    >
                      {item.call.status.replace("_", " ")}
                    </Badge>
                  </div>
                  {item.call.locations.length > 0 ? (
                    <ul className="border-t border-border/70 px-3 py-1.5">
                      {item.call.locations.map((location) => (
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
                  {item.call.input || item.call.output ? (
                    <details className="border-t border-border/70 px-3 py-1.5 text-[10.5px] text-muted">
                      <summary className="cursor-pointer select-none text-faint hover:text-fg">
                        Details
                      </summary>
                      {item.call.input ? (
                        <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-canvas p-2 font-mono text-[10px] text-fg">
                          {item.call.input}
                        </pre>
                      ) : null}
                      {item.call.output ? (
                        <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap break-all rounded bg-canvas p-2 font-mono text-[10px] text-fg">
                          {item.call.output}
                        </pre>
                      ) : null}
                    </details>
                  ) : null}
                </div>
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
