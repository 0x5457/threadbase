/** Dedicated view for one subagent session inside the parent thread page. */
import { ArrowLeft, Bot, Columns2 } from "lucide-react";
import {
  Box,
  Button,
  Group,
  ScrollArea,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { EventEnvelope } from "@/api/types/EventEnvelope";
import type { SubagentDto } from "@/api/types/SubagentDto";
import { Link } from "react-router-dom";
import { EmptyState } from "@/components/app";
import { SubagentStatusBadge } from "@/components/thread/SubagentStatusBadge";
import { ThreadTimeline } from "@/components/thread/ThreadTimeline";
import { formatTokenCount } from "@/lib/format";

export interface SubagentViewProps {
  subagent: SubagentDto;
  threadId: string;
  threadTitle: string;
  projectId: string;
  events: EventEnvelope[];
  approvals: ApprovalDto[];
  loading: boolean;
  error: unknown;
  onRetry: () => void;
  onCancel: () => void;
}

export function SubagentView({
  subagent,
  threadId,
  threadTitle,
  projectId,
  events,
  approvals,
  loading,
  error,
  onRetry,
  onCancel,
}: {
  subagent: SubagentDto;
  threadId: string;
  threadTitle: string;
  projectId: string;
  events: EventEnvelope[];
  approvals: ApprovalDto[];
  loading: boolean;
  error: unknown;
  onRetry: () => void;
  onCancel: () => void;
}) {
  return (
    <Stack h="100%" gap={0}>
      <Box
        px="md"
        py="sm"
        style={{ borderBottom: "1px solid var(--mantine-color-default-border)" }}
      >
        <Box maw={768} mx="auto" w="100%">
          <Group gap="xs" align="flex-start" wrap="wrap">
            <Box
              style={{
                width: 28,
                height: 28,
                flexShrink: 0,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                borderRadius: "var(--mantine-radius-md)",
                border: "1px solid var(--mantine-color-default-border)",
              }}
            >
              <Bot size={16} aria-hidden />
            </Box>
            <Stack gap={2} style={{ flex: 1, minWidth: 0 }}>
              <Group gap="xs" wrap="wrap">
                <Title order={4} fz="sm" fw={600}>
                  {subagent.title}
                </Title>
                <SubagentStatusBadge status={subagent.status} />
              </Group>
              {subagent.description ? (
                <Text size="sm" c="dimmed">
                  {subagent.description}
                </Text>
              ) : null}
              <Group gap="sm">
                {subagent.cwd ? (
                  <Text
                    size="sm"
                    c="dimmed"
                    ff="var(--mantine-font-family-monospace)"
                    style={{ wordBreak: "break-all" }}
                  >
                    cwd: {subagent.cwd}
                  </Text>
                ) : null}
                {subagent.usage ? (
                  <Text size="sm" c="dimmed">
                    {formatTokenCount(subagent.usage.totalTokens)} tokens
                  </Text>
                ) : null}
                <Text
                  size="sm"
                  c="dimmed"
                  ff="var(--mantine-font-family-monospace)"
                >
                  session {subagent.id.slice(0, 12)}
                </Text>
              </Group>
            </Stack>
            <Button
              component={Link}
              to={`/p/${encodeURIComponent(projectId)}/t/${encodeURIComponent(
                threadId,
              )}/subagents/compare`}
              variant="subtle"
              color="gray"
              size="xs"
              leftSection={<Columns2 size={14} aria-hidden />}
            >
              Compare
            </Button>
            {subagent.canCancel ? (
              <Button
                variant="light"
                color="red"
                size="xs"
                onClick={onCancel}
              >
                Cancel subagent
              </Button>
            ) : null}
            <Button
              component={Link}
              to={`/p/${encodeURIComponent(projectId)}/t/${encodeURIComponent(
                threadId,
              )}`}
              variant="default"
              size="xs"
              leftSection={<ArrowLeft size={14} aria-hidden />}
            >
              Back to {threadTitle}
            </Button>
          </Group>
        </Box>
      </Box>

      <ScrollArea style={{ flex: 1, minHeight: 0 }}>
        {events.length === 0 && !loading && !error ? (
          <Box maw={768} mx="auto" px="md" py="lg" w="100%">
            <EmptyState
              icon={Bot}
              title="No subagent activity recorded yet"
              description="The subagent transcript will appear here as it reports progress."
              compact
            />
          </Box>
        ) : (
          <ThreadTimeline
            events={events}
            approvals={approvals}
            loading={loading}
            error={error}
            onRetry={onRetry}
          />
        )}
      </ScrollArea>

      <Box
        px="md"
        py="xs"
        style={{ borderTop: "1px solid var(--mantine-color-default-border)" }}
      >
        <Text
          size="sm"
          c="dimmed"
          maw={768}
          mx="auto"
          w="100%"
          style={{ lineHeight: 1.5 }}
        >
          Subagent sessions cannot be prompted from the client. Ask the parent
          agent to relay the instruction in the parent thread.
        </Text>
      </Box>
    </Stack>
  );
}
