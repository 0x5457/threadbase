import { ChevronLeft, Columns2 } from "lucide-react";
import {
  Anchor,
  Group,
  Paper,
  ScrollArea,
  Select,
  SimpleGrid,
  Skeleton,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import { useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import {
  useProjectThreads,
  useThreadBundle,
  useThreadEvents,
} from "@/api/queries";
import type { EventEnvelope } from "@/api/types/EventEnvelope";
import { ThreadStatusBadge } from "@/components/thread/ThreadStatusBadge";
import { EmptyState, PageContainer } from "@/components/app";
import { layout } from "@/lib/design";

const threadSelectData = (
  list: Array<{ id: string; title: string; agent: string }>,
) =>
  list.map((thread) => ({
    value: thread.id,
    label: `${thread.title} · ${thread.agent}`,
  }));

function recentText(events: EventEnvelope[]): string[] {
  return events
    .slice(-40)
    .flatMap((event) => {
      const payload = event.payload;
      if (payload.type === "turn_started") return [`You: ${payload.prompt}`];
      if (payload.type === "message_completed") {
        return [
          `${payload.message.role === "agent" ? "Agent" : "You"}: ${payload.message.text}`,
        ];
      }
      if (payload.type === "tool_call") return [`Tool: ${payload.call.title}`];
      if (payload.type === "turn_error") return [`Error: ${payload.message}`];
      return [];
    })
    .slice(-6);
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <Stack gap={0} align="center">
      <Text size="xs" c="dimmed" tt="uppercase">
        {label}
      </Text>
      <Text size="sm" className="tb-mono">
        {value}
      </Text>
    </Stack>
  );
}

function CompareColumn({ threadId }: { threadId: string }) {
  const bundle = useThreadBundle(threadId);
  const events = useThreadEvents(threadId);
  if (bundle.isLoading) {
    return <Skeleton h={256} />;
  }
  if (!bundle.data) {
    return (
      <EmptyState
        title="Cannot load thread"
        description="The selected thread may have been archived or removed."
        compact
      />
    );
  }
  const data = bundle.data.bundle;
  const recent = recentText(events.data?.events ?? []);
  return (
    <Paper withBorder style={{ minWidth: 0 }}>
      <Group justify="space-between" align="flex-start" gap="xs" wrap="wrap">
        <Text size="sm" fw={600} truncate="end" style={{ flex: 1 }}>
          {data.thread.title}
        </Text>
        <ThreadStatusBadge status={data.thread.status} />
      </Group>
      <Text size="xs" c="dimmed" mt={2}>
        {data.thread.agent}
        {data.thread.model ? ` · ${data.thread.model}` : ""} ·{" "}
        {data.thread.branch}
      </Text>

      <SimpleGrid cols={3} spacing="xs" mt="sm">
        <Metric label="Messages" value={String(events.data?.events.length ?? 0)} />
        <Metric
          label="Changes"
          value={`+${data.thread.changes.additions} −${data.thread.changes.deletions}`}
        />
        <Metric label="Approvals" value={String(data.thread.pendingApprovals)} />
      </SimpleGrid>

      <Stack gap={6} mt="sm">
        {recent.length === 0 ? (
          <Text size="sm" c="dimmed">
            No messages yet.
          </Text>
        ) : (
          recent.map((text, index) => (
            <Text
              key={`${threadId}-${index}`}
              size="sm"
              c="dimmed"
              bg="var(--mantine-color-default-hover)"
              p="xs"
              lineClamp={2}
            >
              {text}
            </Text>
          ))
        )}
      </Stack>

      <Anchor
        component={Link}
        to={`/p/${encodeURIComponent(data.project.id)}/t/${encodeURIComponent(
          data.thread.id,
        )}`}
        size="sm"
        mt="sm"
      >
        Open thread
      </Anchor>
    </Paper>
  );
}

export function ComparePage() {
  const { projectId } = useParams();
  const threads = useProjectThreads(projectId);
  const list = useMemo(
    () => (threads.data?.threads ?? []).filter((thread) => !thread.archivedAt),
    [threads.data?.threads],
  );
  const [left, setLeft] = useState("");
  const [right, setRight] = useState("");
  const selectedLeft = left || list[0]?.id || "";
  const selectedRight = right || list[1]?.id || "";
  const data = threadSelectData(list);

  return (
    <ScrollArea h="100%">
      <PageContainer>
        <Anchor
          component={Link}
          to={projectId ? `/p/${encodeURIComponent(projectId)}` : "/"}
          size="sm"
          c="dimmed"
        >
          <Group gap={4}>
            <ChevronLeft size={layout.icon.xs} aria-hidden />
            Back to project
          </Group>
        </Anchor>

        <Group
          justify="space-between"
          align="flex-start"
          wrap="wrap"
          gap="md"
          mt="sm"
        >
          <Stack gap={2}>
            <Group gap="xs">
              <Columns2 size={layout.icon.md} aria-hidden />
              <Title order={1}>Compare threads</Title>
            </Group>
            <Text size="xs" c="dimmed">
              Same-task runs side by side.
            </Text>
          </Stack>
        </Group>

        {/* Same two-column grid as the cards below, so the selects share the
            cards' left and right edges at every width. */}
        <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="sm" mt="md">
          <Select
            label="Left thread"
            value={selectedLeft || null}
            onChange={(value) => setLeft(value ?? "")}
            data={data}
            placeholder="Left thread"
          />
          <Select
            label="Right thread"
            value={selectedRight || null}
            onChange={(value) => setRight(value ?? "")}
            data={data}
            placeholder="Right thread"
          />
        </SimpleGrid>

        <SimpleGrid cols={{ base: 1, sm: 2 }} spacing="sm" mt="md">
          {selectedLeft ? (
            <CompareColumn threadId={selectedLeft} />
          ) : (
            <EmptyState title="No thread selected" compact />
          )}
          {selectedRight ? (
            <CompareColumn threadId={selectedRight} />
          ) : (
            <EmptyState title="Create another thread to compare" compact />
          )}
        </SimpleGrid>
      </PageContainer>
    </ScrollArea>
  );
}
