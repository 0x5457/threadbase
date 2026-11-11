import { Terminal } from "lucide-react";
import { useMemo } from "react";
import { Badge, Box, Divider, Flex, Paper, Skeleton, Stack, Text } from "@mantine/core";
import { useThreadEvents } from "@/api/queries";
import type { ToolCallData } from "@/api/types/ToolCallData";
import { EmptyState, ErrorState } from "@/components/app";

function callStatusColor(
  status: ToolCallData["status"],
): "blue" | "red" | "green" {
  switch (status) {
    case "completed":
      return "green";
    case "failed":
      return "red";
    default:
      return "blue";
  }
}

export function TerminalPanel({ threadId }: { threadId: string }) {
  const events = useThreadEvents(threadId);
  const calls = useMemo(() => {
    const latest = new Map<string, ToolCallData>();
    for (const event of events.data?.events ?? []) {
      if (
        event.payload.type === "tool_call" ||
        event.payload.type === "tool_call_update"
      ) {
        if (event.payload.call.kind === "execute") {
          latest.set(event.payload.call.id, event.payload.call);
        }
      }
    }
    return [...latest.values()];
  }, [events.data?.events]);

  if (events.isLoading) {
    return (
      <Stack gap="sm" p="sm">
        <Skeleton height={64} />
        <Skeleton height={96} />
      </Stack>
    );
  }
  if (events.isError) {
    return (
      <ErrorState
        error={events.error}
        title="Cannot load command output"
        onRetry={() => void events.refetch()}
        compact
      />
    );
  }
  if (calls.length === 0) {
    return (
      <EmptyState
        icon={Terminal}
        title="No commands run yet"
        compact
      />
    );
  }
  return (
    <Stack gap="sm" p="sm">
      {calls.map((call) => (
        <Paper key={call.id} withBorder radius="md">
          <Flex align="center" gap="xs" p="xs">
            <Terminal
              size={12}
              aria-hidden
              style={{ color: "var(--mantine-color-dimmed)" }}
            />
            <Text size="sm" ff="monospace" truncate="end" flex={1}>
              {call.title}
            </Text>
            <Badge
              variant="light"
              color={callStatusColor(call.status)}
              size="xs"
            >
              {call.status}
            </Badge>
          </Flex>
          {call.input ? (
            <>
              <Divider />
              <Box
                p="xs"
                style={{
                  fontFamily: "var(--mantine-font-family-monospace)",
                  fontSize: 11,
                  color: "var(--mantine-color-dimmed)",
                  whiteSpace: "pre-wrap",
                  overflowWrap: "break-word",
                }}
              >
                $ {call.input}
              </Box>
            </>
          ) : null}
          <Divider />
          <Box
            p="xs"
            style={{
              fontFamily: "var(--mantine-font-family-monospace)",
              fontSize: 11,
              maxHeight: 224,
              overflow: "auto",
              whiteSpace: "pre-wrap",
              overflowWrap: "break-word",
            }}
          >
            {call.output ?? "(no output)"}
          </Box>
        </Paper>
      ))}
    </Stack>
  );
}
