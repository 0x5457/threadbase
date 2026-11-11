import {
  ChevronRight,
  MessageSquare,
} from "lucide-react";
import { useMemo } from "react";
import {
  Badge,
  Box,
  Flex,
  Skeleton,
  Stack,
  Text,
} from "@mantine/core";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { CommentAnchor } from "@/api/types/CommentAnchor";
import type { ElicitationAction } from "@/api/types/ElicitationAction";
import type { EventEnvelope } from "@/api/types/EventEnvelope";
import { EmptyState, ErrorState } from "@/components/app";
import { ElicitationCard } from "@/components/thread/ElicitationCard";
import { usePresenceStore } from "@/lib/presence";
import { buildTimeline, summarizeTools } from "./timeline/items";
import { AgentMessageRow } from "./timeline/AgentMessageRow";
import { ApprovalRow } from "./timeline/ApprovalRow";
import { MessageRow } from "./timeline/MessageRow";
import { NoticeRow } from "./timeline/NoticeRow";
import { PlanCard } from "./timeline/PlanCard";
import { SubagentRow } from "./timeline/SubagentRow";
import { ToolCard } from "./timeline/ToolCard";
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
  const approvalById = useMemo(
    () => new Map(approvals.map((approval) => [approval.id, approval])),
    [approvals],
  );


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
            return (
              <MessageRow
                key={item.key}
                message={item}
                onSelect={() =>
                  setLocal({
                    panel: "transcript",
                    target: { kind: "message", messageId: item.messageId },
                  })
                }
                onComment={onComment}
              />
            );
          }

          if (item.kind === "tool") {
            return (
              <ToolCard
                key={item.key}
                call={item.call}
                onSelect={() =>
                  setLocal({
                    panel: "transcript",
                    target: { kind: "tool_call", toolCallId: item.call.id },
                  })
                }
                onComment={onComment}
              />
            );
          }

          if (item.kind === "tool_group") {
            if (item.calls.length === 1) {
              return (
                <ToolCard
                  key={item.key}
                  call={item.calls[0]}
                  onSelect={() =>
                    setLocal({
                      panel: "transcript",
                      target: {
                        kind: "tool_call",
                        toolCallId: item.calls[0].id,
                      },
                    })
                  }
                  onComment={onComment}
                />
              );
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
                        fontSize: "var(--mantine-font-size-md)",
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
                      {item.calls.map((call) => (
                        <ToolCard
                          key={call.id}
                          call={call}
                          onSelect={() =>
                            setLocal({
                              panel: "transcript",
                              target: { kind: "tool_call", toolCallId: call.id },
                            })
                          }
                          onComment={onComment}
                        />
                      ))}
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
            return <PlanCard key={item.key} entries={item.entries} />;
          }

          if (item.kind === "agent_message") {
            return <AgentMessageRow key={item.key} item={item} />;
          }

          if (item.kind === "subagent") {
            return (
              <SubagentRow
                key={item.key}
                subagent={item.subagent}
                onOpen={onOpenSubagent}
              />
            );
          }

          if (item.kind === "approval") {
            return (
              <ApprovalRow
                key={item.key}
                item={item}
                approval={approvalById.get(item.approval.id) ?? item.approval}
                onDecide={onDecide}
              />
            );
          }

          return <NoticeRow key={item.key} item={item} />;
        })}
      </Stack>
    </Box>
  );
}
