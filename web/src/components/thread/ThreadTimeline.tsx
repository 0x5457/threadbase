import {
  AlertTriangle,
  ArrowLeftRight,
  Brain,
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
import {
  ActionIcon,
  Avatar,
  Badge,
  Box,
  Button,
  Flex,
  Paper,
  Skeleton,
  Stack,
  Text,
  Textarea,
} from "@mantine/core";
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
import { EmptyState, ErrorState } from "@/components/app";
import { ElicitationCard } from "@/components/thread/ElicitationCard";
import { SubagentStatusBadge } from "@/components/thread/SubagentStatusBadge";
import { usePresenceStore } from "@/lib/presence";
import { formatTimestamp, initials } from "@/lib/format";

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

interface AgentMessageItem {
  kind: "agent_message";
  key: string;
  direction: "sent" | "received";
  from: string;
  to: string;
  text: string;
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
  | AgentMessageItem
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

function toolStatusColor(
  status: ToolCallData["status"],
): "blue" | "green" | "red" {
  switch (status) {
    case "pending":
    case "in_progress":
      return "blue";
    case "completed":
      return "green";
    case "failed":
      return "red";
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

function buildTimeline(events: EventEnvelope[], activeSubagentCount = 0): TimelineItem[] {
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

export function ThreadTimeline({
  events,
  approvals,
  activeSubagentCount = 0,
  loading = false,
  error,
  onRetry,
  onDecide,
  onRespondElicitation,
  onComment,
  onOpenSubagent,
}: ThreadTimelineProps) {
  const items = useMemo(
    () => buildTimeline(events, activeSubagentCount),
    [events, activeSubagentCount],
  );
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
      <Flex
        key={key}
        gap="md"
        onClick={() =>
          setLocal({
            panel: "transcript",
            target: { kind: "tool_call", toolCallId: call.id },
          })
        }
      >
        <Box w={28} />
        <Paper
          withBorder
          radius="md"
          flex={1}
          miw={0}
          style={{ overflow: "hidden" }}
        >
          <Flex align="center" gap="xs" px="sm" py="xs">
            <Icon
              size={14}
              aria-hidden
              style={{
                color: "var(--mantine-color-dimmed)",
                flexShrink: 0,
              }}
            />
            <Text size="sm" fw={500} truncate="end" flex={1} miw={0}>
              {call.title}
            </Text>
            {onComment ? (
              <ActionIcon
                variant="subtle"
                color="gray"
                size="xs"
                aria-label="Comment on tool call"
                style={{ marginLeft: "auto" }}
                onClick={() =>
                  onComment({ kind: "tool_call", toolCallId: call.id })
                }
              >
                <MessageSquare size={12} aria-hidden />
              </ActionIcon>
            ) : null}
            <Badge
              variant="light"
              color={toolStatusColor(call.status)}
              size="xs"
              style={onComment ? undefined : { marginLeft: "auto" }}
            >
              {call.status.replace("_", " ")}
            </Badge>
          </Flex>
          {call.locations.length > 0 ? (
            <Box
              component="ul"
              px="sm"
              py="xs"
              style={{
                borderTop:
                  "1px solid var(--mantine-color-default-border)",
                listStyle: "none",
                margin: 0,
              }}
            >
              {call.locations.map((location) => (
                <li
                  key={`${location.path}:${location.line ?? 0}`}
                  style={{
                    fontFamily: "var(--mantine-font-family-monospace)",
                    fontSize: 13,
                    color: "var(--mantine-color-dimmed)",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {location.path}
                  {location.line ? `:${location.line}` : ""}
                </li>
              ))}
            </Box>
          ) : null}
          {call.diff ? (
            <details>
              <summary
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "0.5rem",
                  padding: "var(--mantine-spacing-xs) var(--mantine-spacing-sm)",
                  fontSize: 13,
                  listStyle: "none",
                  cursor: "pointer",
                  borderTop:
                    "1px solid var(--mantine-color-default-border)",
                }}
              >
                <Text
                  size="sm"
                  ff="monospace"
                  c="dimmed"
                  truncate="end"
                  flex={1}
                  miw={0}
                >
                  {call.diff.path}
                </Text>
                <Text
                  size="sm"
                  ff="monospace"
                  c="green.6"
                  style={{ marginLeft: "auto", whiteSpace: "nowrap" }}
                >
                  +{call.diff.additions}
                </Text>
                <Text
                  size="sm"
                  ff="monospace"
                  c="red.6"
                  style={{ whiteSpace: "nowrap" }}
                >
                  −{call.diff.deletions}
                </Text>
                <ChevronRight
                  size={12}
                  aria-hidden
                  style={{
                    color: "var(--mantine-color-dimmed)",
                    flexShrink: 0,
                  }}
                />
              </summary>
              {call.diff.patch ? (
                <Box
                  component="pre"
                  p="xs"
                  mah={256}
                  style={{
                    margin: 0,
                    overflow: "auto",
                    fontFamily: "var(--mantine-font-family-monospace)",
                    fontSize: 13,
                    whiteSpace: "pre-wrap",
                    wordBreak: "break-all",
                    borderTop:
                      "1px solid var(--mantine-color-default-border)",
                  }}
                >
                  {call.diff.patch}
                </Box>
              ) : null}
            </details>
          ) : null}
          {call.input || call.output ? (
            <details
              style={{
                borderTop:
                  "1px solid var(--mantine-color-default-border)",
                fontSize: 13,
                color: "var(--mantine-color-dimmed)",
              }}
            >
              <summary
                style={{
                  display: "flex",
                  padding: "var(--mantine-spacing-xs) var(--mantine-spacing-sm)",
                  listStyle: "none",
                  cursor: "pointer",
                  color: "var(--mantine-color-dimmed)",
                }}
              >
                Details
              </summary>
              {call.input ? (
                <Box
                  component="pre"
                  p="xs"
                  mah={192}
                  style={{
                    margin: 0,
                    overflow: "auto",
                    fontFamily: "var(--mantine-font-family-monospace)",
                    fontSize: 13,
                    whiteSpace: "pre-wrap",
                    wordBreak: "break-all",
                    background: "var(--mantine-color-default)",
                    borderRadius: "var(--mantine-radius-md)",
                  }}
                >
                  {call.input}
                </Box>
              ) : null}
              {call.output ? (
                <Box
                  component="pre"
                  p="xs"
                  mah={256}
                  style={{
                    margin: 0,
                    overflow: "auto",
                    fontFamily: "var(--mantine-font-family-monospace)",
                    fontSize: 13,
                    whiteSpace: "pre-wrap",
                    wordBreak: "break-all",
                    background: "var(--mantine-color-default)",
                    borderRadius: "var(--mantine-radius-md)",
                  }}
                >
                  {call.output}
                </Box>
              ) : null}
            </details>
          ) : null}
        </Paper>
      </Flex>
    );
  };

  if (loading) {
    return (
      <Box
        px="md"
        py="xl"
        style={{ maxWidth: 768, margin: "0 auto", width: "100%" }}
      >
        <Stack gap="md">
          <Skeleton height={56} width="80%" />
          <Skeleton height={96} />
          <Skeleton height={56} width="60%" />
          <Skeleton height={128} />
        </Stack>
      </Box>
    );
  }

  if (error) {
    return (
      <Box
        px="md"
        py="xl"
        style={{ maxWidth: 768, margin: "0 auto", width: "100%" }}
      >
        <ErrorState
          error={error}
          title="Cannot load thread activity"
          onRetry={onRetry}
        />
      </Box>
    );
  }

  if (items.length === 0) {
    return (
      <Box
        px="md"
        py="xl"
        style={{ maxWidth: 768, margin: "0 auto", width: "100%" }}
      >
        <EmptyState icon={MessageSquare} title="No activity yet" compact />
      </Box>
    );
  }

  return (
    <Box
      px="md"
      py="lg"
      style={{ maxWidth: 768, margin: "0 auto", width: "100%" }}
    >
      <Stack gap="sm">
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
              <Box key={`event-wrap-${item.key}`}>
                {item.seq ? (
                  <span id={`event-${item.seq}`} aria-hidden />
                ) : null}
                <Flex
                  gap="md"
                  id={`message-${item.messageId}`}
                  data-message-id={item.messageId}
                  onClick={() =>
                    setLocal({
                      panel: "transcript",
                      target: { kind: "message", messageId: item.messageId },
                    })
                  }
                >
                  <Flex w={28} justify="center" pt={2}>
                    <Avatar
                      src={person.avatarUrl ?? null}
                      alt={person.displayName}
                      name={person.displayName}
                      color={person.color ?? "initials"}
                      size={24}
                      radius="xl"
                    >
                      {initials(person.displayName)}
                    </Avatar>
                  </Flex>
                  <Box flex={1} miw={0}>
                    <Flex align="center" gap="xs" mb="xs">
                      <Text fz={12} fw={500} c="dimmed">
                        {author?.displayName ??
                          (item.role === "agent" ? "Agent" : "User")}
                      </Text>
                      {item.authors.length > 1 ? (
                        <Text fz={12} c="dimmed">
                          +{item.authors.length - 1}
                        </Text>
                      ) : null}
                      {item.createdAt ? (
                        <Text
                          fz={12}
                          c="dimmed"
                          title={formatTimestamp(item.createdAt)}
                        >
                          {formatTimestamp(item.createdAt)}
                        </Text>
                      ) : null}
                      <ActionIcon
                        variant="subtle"
                        color="gray"
                        size={24}
                        aria-label="View comments"
                        style={{ marginLeft: "auto" }}
                        onClick={() =>
                          window.dispatchEvent(
                            new CustomEvent("threadbase:open-comments"),
                          )
                        }
                      >
                        <MessageSquareText size={12} aria-hidden />
                      </ActionIcon>
                      {onComment ? (
                        <ActionIcon
                          variant="subtle"
                          color="gray"
                          size={24}
                          aria-label="Comment on message"
                          onClick={() =>
                            onComment({
                              kind: "message",
                              messageId: item.messageId,
                            })
                          }
                        >
                          <MessageSquare size={12} aria-hidden />
                        </ActionIcon>
                      ) : null}
                    </Flex>
                    <Box
                      px="sm"
                      py="xs"
                      style={{
                        whiteSpace: "pre-wrap",
                        overflowWrap: "break-word",
                        fontSize: 13,
                        lineHeight: 1.6,
                        borderRadius: "var(--mantine-radius-md)",
                        ...(item.role === "user"
                          ? {
                              background:
                                "var(--mantine-color-default-hover)",
                            }
                          : {}),
                      }}
                    >
                      {item.text}
                    </Box>
                  </Box>
                </Flex>
              </Box>
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
              <Flex key={item.key} gap="md">
                <Box w={28} />
                <Box
                  flex={1}
                  miw={0}
                  style={{
                    border:
                      "1px solid var(--mantine-color-default-border)",
                    borderRadius: "var(--mantine-radius-md)",
                    background: "var(--mantine-color-default)",
                    overflow: "hidden",
                  }}
                >
                  <details>
                    <summary
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: "0.5rem",
                        padding: "var(--mantine-spacing-xs) var(--mantine-spacing-sm)",
                        fontSize: 13,
                        listStyle: "none",
                        cursor: "pointer",
                        color: "var(--mantine-color-dimmed)",
                      }}
                    >
                      <ChevronRight
                        size={14}
                        aria-hidden
                        style={{
                          color: "var(--mantine-color-dimmed)",
                          flexShrink: 0,
                        }}
                      />
                      <Text
                        size="sm"
                        fw={500}
                        c="dimmed"
                        truncate="end"
                        flex={1}
                        miw={0}
                      >
                        {summarizeTools(item.calls)}
                      </Text>
                      <Badge
                        variant="light"
                        color="gray"
                        size="xs"
                        style={{ marginLeft: "auto" }}
                      >
                        {item.calls.length} calls
                      </Badge>
                    </summary>
                    <Stack
                      gap="xs"
                      p="xs"
                      style={{
                        borderTop:
                          "1px solid var(--mantine-color-default-border)",
                      }}
                    >
                      {item.calls.map((call, index) =>
                        renderTool(call, `${item.key}-${index}`),
                      )}
                    </Stack>
                  </details>
                </Box>
              </Flex>
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
              <Flex key={item.key} gap="md">
                <Box w={28} />
                <Paper withBorder radius="md" flex={1} p="sm">
                  <Flex align="center" gap="xs" mb="xs">
                    <ListChecks
                      size={14}
                      aria-hidden
                      style={{
                        color: "var(--mantine-color-dimmed)",
                        flexShrink: 0,
                      }}
                    />
                    <Text size="sm" fw={500}>
                      Plan
                    </Text>
                  </Flex>
                  <Box
                    component="ul"
                    style={{ listStyle: "none", margin: 0, padding: 0 }}
                  >
                    {item.entries.map((entry) => (
                      <Flex
                        component="li"
                        key={entry.content}
                        align="flex-start"
                        gap="xs"
                        fz={13}
                        c="dimmed"
                      >
                        <span
                          aria-hidden
                          style={{
                            marginTop: 4,
                            width: 6,
                            height: 6,
                            borderRadius: "50%",
                            flexShrink: 0,
                            background:
                              entry.status === "completed"
                                ? "var(--mantine-color-green-6)"
                                : entry.status === "in_progress"
                                  ? "var(--mantine-color-yellow-6)"
                                  : "var(--mantine-color-default-border)",
                          }}
                        />
                        <Text
                          component="span"
                          fz={13}
                          c={entry.status === "completed" ? "dimmed" : undefined}
                          style={
                            entry.status === "completed"
                              ? { textDecoration: "line-through" }
                              : undefined
                          }
                        >
                          {entry.content}
                        </Text>
                      </Flex>
                    ))}
                  </Box>
                </Paper>
              </Flex>
            );
          }

          if (item.kind === "agent_message") {
            const label =
              item.direction === "sent"
                ? `Sent Agent Message to ${item.to}`
                : `Received Agent Message from ${item.from}`;
            return (
              <Flex key={item.key} gap="md">
                <Box w={28} />
                <Box
                  flex={1}
                  miw={0}
                  style={{
                    border:
                      "1px solid var(--mantine-color-default-border)",
                    borderRadius: "var(--mantine-radius-md)",
                    background: "var(--mantine-color-default)",
                    overflow: "hidden",
                  }}
                >
                  <details>
                    <summary
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: "0.5rem",
                        padding: "var(--mantine-spacing-xs) var(--mantine-spacing-sm)",
                        fontSize: 13,
                        listStyle: "none",
                        cursor: "pointer",
                        color: "var(--mantine-color-dimmed)",
                      }}
                    >
                      <ArrowLeftRight
                        size={14}
                        aria-hidden
                        style={{
                          color: "var(--mantine-color-dimmed)",
                          flexShrink: 0,
                        }}
                      />
                      <Text
                        size="sm"
                        fw={500}
                        c="dimmed"
                        truncate="end"
                        flex={1}
                        miw={0}
                      >
                        {label}
                      </Text>
                      <ChevronRight
                        size={12}
                        aria-hidden
                        style={{
                          color: "var(--mantine-color-dimmed)",
                          flexShrink: 0,
                          marginLeft: "auto",
                        }}
                      />
                    </summary>
                    <Text
                      size="sm"
                      p="sm"
                      style={{
                        whiteSpace: "pre-wrap",
                        lineHeight: 1.6,
                        borderTop:
                          "1px solid var(--mantine-color-default-border)",
                      }}
                    >
                      {item.text}
                    </Text>
                  </details>
                </Box>
              </Flex>
            );
          }

          if (item.kind === "subagent") {
            const subagent = item.subagent;
            return (
              <Flex key={item.key} gap="md">
                <Box w={28} />
                <Box
                  flex={1}
                  miw={0}
                  style={{
                    border:
                      "1px solid var(--mantine-color-default-border)",
                    borderRadius: "var(--mantine-radius-md)",
                    background: "var(--mantine-color-default)",
                    overflow: "hidden",
                  }}
                >
                  <details>
                    <summary
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: "0.5rem",
                        padding: "var(--mantine-spacing-xs) var(--mantine-spacing-sm)",
                        listStyle: "none",
                        cursor: "pointer",
                      }}
                    >
                      <GitBranch
                        size={14}
                        aria-hidden
                        style={{
                          color: "var(--mantine-color-dimmed)",
                          flexShrink: 0,
                        }}
                      />
                      <Text
                        size="sm"
                        fw={500}
                        truncate="end"
                        flex={1}
                        miw={0}
                      >
                        {subagent.title}
                      </Text>
                      <SubagentStatusBadge status={subagent.status} />
                      <ChevronRight
                        size={12}
                        aria-hidden
                        style={{
                          color: "var(--mantine-color-dimmed)",
                          flexShrink: 0,
                          marginLeft: "auto",
                        }}
                      />
                    </summary>
                    <Box
                      p="sm"
                      style={{
                        borderTop:
                          "1px solid var(--mantine-color-default-border)",
                      }}
                    >
                      <Stack gap={4}>
                        {subagent.description ? (
                          <Text
                            size="sm"
                            style={{ whiteSpace: "pre-wrap" }}
                          >
                            {subagent.description}
                          </Text>
                        ) : null}
                        {subagent.status === "error" ? (
                          <Flex align="center" gap="xs">
                            <XCircle
                              size={12}
                              aria-hidden
                              style={{
                                color: "var(--mantine-color-red-6)",
                              }}
                            />
                            <Text size="sm" c="red.6">
                              Stopped with an error
                            </Text>
                          </Flex>
                        ) : null}
                        {subagent.cwd ? (
                          <Text
                            fz={12}
                            ff="monospace"
                            c="dimmed"
                            style={{ wordBreak: "break-all" }}
                          >
                            {subagent.cwd}
                          </Text>
                        ) : (
                          <Text size="sm" c="dimmed">
                            No independent session
                          </Text>
                        )}
                        {subagent.usage ? (
                          <Text size="sm" c="dimmed">
                            {subagent.usage.totalTokens.toLocaleString()} tokens
                          </Text>
                        ) : null}
                        {onOpenSubagent ? (
                          <Button
                            variant="subtle"
                            color="dark"
                            size="xs"
                            px={0}
                            onClick={() => onOpenSubagent(subagent.id)}
                          >
                            Open subagent
                          </Button>
                        ) : null}
                      </Stack>
                    </Box>
                  </details>
                </Box>
              </Flex>
            );
          }

          if (item.kind === "approval") {
            const approval = approvalById.get(item.approval.id) ?? item.approval;
            return (
              <Flex key={item.key} gap="md">
                <Box w={28} />
                <Box
                  flex={1}
                  miw={0}
                  style={{
                    border:
                      "1px solid var(--mantine-color-yellow-light)",
                    borderRadius: "var(--mantine-radius-md)",
                    background: "var(--mantine-color-yellow-light)",
                    overflow: "hidden",
                  }}
                >
                  <Flex align="center" gap="xs" px="sm" py="xs">
                    <ShieldAlert
                      size={14}
                      aria-hidden
                      style={{
                        color:
                          "var(--mantine-color-yellow-light-color)",
                        flexShrink: 0,
                      }}
                    />
                    <Text
                      size="sm"
                      fw={500}
                      truncate="end"
                      flex={1}
                      miw={0}
                    >
                      {approval.title}
                    </Text>
                    <Badge
                      variant="light"
                      color={
                        item.resolved && approval.status !== "pending"
                          ? "gray"
                          : "yellow"
                      }
                      size="xs"
                      style={{ marginLeft: "auto" }}
                    >
                      {item.decision ?? approval.status}
                    </Badge>
                  </Flex>
                  <Box
                    px="sm"
                    py="xs"
                    style={{
                      borderTop:
                        "1px solid var(--mantine-color-yellow-light-hover)",
                    }}
                  >
                    <Stack gap={4}>
                      <Text size="sm" c="dimmed">
                        {approval.agent}
                        {approval.subagentId
                          ? ` · subagent ${approval.subagentId}`
                          : ""}
                      </Text>
                      {approval.command ? (
                        <Box
                          component="pre"
                          p="xs"
                          style={{
                            margin: 0,
                            overflowX: "auto",
                            fontFamily:
                              "var(--mantine-font-family-monospace)",
                            fontSize: 13,
                            background: "var(--mantine-color-default)",
                            borderRadius: "var(--mantine-radius-md)",
                          }}
                        >
                          {approval.command}
                        </Box>
                      ) : null}
                      {approval.cwd ? (
                        <Text
                          size="sm"
                          ff="monospace"
                          c="dimmed"
                          style={{ wordBreak: "break-all" }}
                        >
                          cwd: {approval.cwd}
                        </Text>
                      ) : null}
                      {approval.paths.length > 0 ? (
                        <Text
                          size="sm"
                          ff="monospace"
                          c="dimmed"
                          style={{ wordBreak: "break-all" }}
                        >
                          {approval.paths.join(", ")}
                        </Text>
                      ) : null}
                      {approval.input ? (
                        <details style={{ fontSize: 13 }}>
                          <summary
                            style={{
                              cursor: "pointer",
                              listStyle: "none",
                              color: "var(--mantine-color-dimmed)",
                            }}
                          >
                            Full parameters
                          </summary>
                          <Box
                            component="pre"
                            p="xs"
                            mah={224}
                            mt={4}
                            style={{
                              margin: 0,
                              overflow: "auto",
                              fontFamily:
                                "var(--mantine-font-family-monospace)",
                              fontSize: 13,
                              whiteSpace: "pre-wrap",
                              wordBreak: "break-all",
                              background: "var(--mantine-color-default)",
                              borderRadius: "var(--mantine-radius-md)",
                            }}
                          >
                            {approval.input}
                          </Box>
                        </details>
                      ) : null}
                      {!item.resolved &&
                      approval.status === "pending" &&
                      onDecide ? (
                        denyFor === approval.id ? (
                          <Box
                            pt="xs"
                            style={{
                              borderTop:
                                "1px solid var(--mantine-color-yellow-light-hover)",
                            }}
                          >
                            <Textarea
                              autoFocus
                              value={denyNote}
                              onChange={(event) =>
                                setDenyNote(event.target.value)
                              }
                              placeholder="Why deny?"
                              rows={2}
                            />
                            <Flex gap="xs" mt="xs">
                              <Button
                                variant="subtle"
                                color="dark"
                                size="xs"
                                onClick={() => {
                                  setDenyFor(null);
                                  setDenyNote("");
                                }}
                              >
                                Cancel
                              </Button>
                              <Button
                                color="red"
                                size="xs"
                                disabled={!denyNote.trim()}
                                onClick={() => {
                                  onDecide(
                                    approval.id,
                                    "deny",
                                    denyNote.trim(),
                                  );
                                  setDenyFor(null);
                                  setDenyNote("");
                                }}
                              >
                                Deny with reason
                              </Button>
                            </Flex>
                          </Box>
                        ) : (
                          <Flex
                            gap="xs"
                            wrap="wrap"
                            pt="xs"
                            style={{
                              borderTop:
                                "1px solid var(--mantine-color-yellow-light-hover)",
                            }}
                          >
                            {(approval.options.length > 0
                              ? approval.options
                              : [
                                  {
                                    id: "once",
                                    kind: "allow_once" as const,
                                    label: "Allow once",
                                  },
                                  {
                                    id: "thread",
                                    kind: "allow_thread" as const,
                                    label: "Allow for thread",
                                  },
                                  {
                                    id: "deny",
                                    kind: "deny" as const,
                                    label: "Deny",
                                  },
                                ]
                            ).map((option) =>
                              option.kind === "deny" ? (
                                <Button
                                  key={option.id}
                                  size="xs"
                                  color="red"
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
                                  size="xs"
                                  variant="outline"
                                  onClick={() =>
                                    onDecide(approval.id, option.kind)
                                  }
                                >
                                  {option.label}
                                </Button>
                              ),
                            )}
                          </Flex>
                        )
                      ) : null}
                    </Stack>
                  </Box>
                </Box>
              </Flex>
            );
          }

          const toneColor =
            item.tone === "danger"
              ? "var(--mantine-color-red-filled)"
              : item.tone === "warning"
                ? "var(--mantine-color-yellow-filled)"
                : item.tone === "info"
                  ? "var(--mantine-color-blue-filled)"
                  : "var(--mantine-color-dimmed)";
          const NoticeIcon =
            item.tone === "danger"
              ? AlertTriangle
              : item.tone === "warning"
                ? ShieldAlert
                : item.tone === "info"
                  ? ArrowLeftRight
                  : CircleDot;
          return (
            <Flex key={item.key} gap="md">
              <Box w={28} />
              <Flex
                align="center"
                gap={6}
                py={2}
                flex={1}
                miw={0}
              >
                <NoticeIcon
                  size={12}
                  aria-hidden
                  style={{ color: toneColor, flexShrink: 0 }}
                />
                <Text
                  size="xs"
                  style={{
                    whiteSpace: "pre-wrap",
                    overflowWrap: "break-word",
                    lineHeight: 1.5,
                  }}
                >
                  <Text component="span" fw={500}>
                    {item.label}
                  </Text>
                  {item.text ? (
                    <Text component="span" c="dimmed">
                      {" "}
                      · {item.text}
                    </Text>
                  ) : null}
                </Text>
              </Flex>
            </Flex>
          );
        })}
      </Stack>
    </Box>
  );
}
