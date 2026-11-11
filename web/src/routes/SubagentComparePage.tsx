import { ChevronLeft, Columns2 } from "lucide-react";
import {
  Anchor,
  Box,
  Group,
  Paper,
  ScrollArea,
  Select,
  Skeleton,
  Stack,
  Text,
  Title,
} from "@mantine/core";
import { useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useThreadBundle, useThreadEvents } from "@/api/queries";
import { ThreadTimeline } from "@/components/thread/ThreadTimeline";
import { EmptyState, PageContainer } from "@/components/app";

export function SubagentComparePage() {
  const { projectId, threadId } = useParams();
  const bundle = useThreadBundle(threadId);
  const events = useThreadEvents(threadId);
  const subagents = useMemo(
    () => bundle.data?.bundle.subagents ?? [],
    [bundle.data?.bundle.subagents],
  );
  const [leftId, setLeftId] = useState("");
  const [rightId, setRightId] = useState("");
  const left = leftId || subagents[0]?.id || "";
  const right = rightId || subagents[1]?.id || "";
  const allEvents = events.data?.events ?? [];
  const leftEvents = allEvents.filter((event) => event.subagentId === left);
  const rightEvents = allEvents.filter((event) => event.subagentId === right);
  const data = subagents.map((subagent) => ({
    value: subagent.id,
    label: subagent.title,
  }));

  return (
    <ScrollArea h="100%">
      <PageContainer>
        <Anchor
          component={Link}
          to={
            projectId && threadId
              ? `/p/${encodeURIComponent(projectId)}/t/${encodeURIComponent(threadId)}`
              : "/"
          }
          size="sm"
          c="dimmed"
        >
          <Group gap={4}>
            <ChevronLeft size={12} aria-hidden />
            Back to thread
          </Group>
        </Anchor>

        <Group justify="space-between" align="flex-end" wrap="wrap" gap="sm" mt="sm">
          <Stack gap={2}>
            <Group gap="xs">
              <Columns2 size={16} aria-hidden />
              <Title order={1}>
                Compare subagents
              </Title>
            </Group>
            <Text size="xs" c="dimmed">
              Two child sessions from the same thread, side by side.
            </Text>
          </Stack>
          <Group gap="sm" align="flex-end">
            <Select
              label="Left subagent"
              value={left || null}
              onChange={(value) => setLeftId(value ?? "")}
              data={data}
              placeholder="Left subagent"
              w={192}
              size="xs"
            />
            <Select
              label="Right subagent"
              value={right || null}
              onChange={(value) => setRightId(value ?? "")}
              data={data}
              placeholder="Right subagent"
              w={192}
              size="xs"
            />
          </Group>
        </Group>

        {bundle.isLoading ? (
          <Skeleton h={256} mt="md" radius="md" />
        ) : subagents.length < 2 ? (
          <EmptyState
            title="Not enough subagents"
            description="At least two child sessions are required for a comparison."
            compact
          />
        ) : (
          <Box
            mt="md"
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(320px, 1fr))",
              gap: 12,
              alignItems: "start",
            }}
          >
            {[left, right].map((id, index) => {
              const subagent = subagents.find((item) => item.id === id);
              const timeline = index === 0 ? leftEvents : rightEvents;
              return (
                <Paper key={id} withBorder radius="md" p="sm" style={{ minWidth: 0 }}>
                  <Text size="sm" fw={500}>
                    {subagent?.title ?? "Subagent"}
                  </Text>
                  <Text size="xs" c="dimmed" mt={2}>
                    {subagent?.status ?? "unknown"}
                    {subagent?.cwd ? ` · ${subagent.cwd}` : ""}
                    {subagent?.usage
                      ? ` · ${subagent.usage.totalTokens} tokens`
                      : ""}
                  </Text>
                  {timeline.length === 0 ? (
                    <Text size="xs" c="dimmed" mt="sm">
                      No child-session transcript was emitted by this adapter.
                      The parent timeline still contains the dispatch card.
                    </Text>
                  ) : (
                    <ThreadTimeline events={timeline} approvals={[]} />
                  )}
                </Paper>
              );
            })}
          </Box>
        )}
      </PageContainer>
    </ScrollArea>
  );
}
