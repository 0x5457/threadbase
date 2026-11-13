import { Drawer, Group, Loader, ScrollArea, Text } from "@mantine/core";
import { useEffect, useState } from "react";
import { getSubagentLog, type Subagent } from "../../api";
import type { SubagentLogEntryDto } from "../../api/types/SubagentLogEntryDto";
import { statusTone } from "../thread/StatusDot";

/**
 * A subagent's own transcript, opened from its row in the sidebar tree. pi
 * leaves each detached run's session on disk, so Threadbase can replay it.
 */
export function SubagentLogDrawer({
  threadId,
  subagent,
  onClose,
}: {
  threadId: string | null;
  subagent: Subagent | null;
  onClose: () => void;
}) {
  const [entries, setEntries] = useState<SubagentLogEntryDto[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const subagentId = subagent ? String(subagent.id) : null;

  useEffect(() => {
    if (!threadId || !subagentId) return;
    let alive = true;
    setEntries(null);
    setError(null);
    getSubagentLog(threadId, subagentId)
      .then((response) => {
        if (alive) setEntries(response.entries);
      })
      .catch((cause) => {
        if (alive) {
          setError(
            cause instanceof Error ? cause.message : "Could not load the subagent",
          );
        }
      });
    return () => {
      alive = false;
    };
  }, [threadId, subagentId]);

  return (
    <Drawer
      opened={Boolean(subagent)}
      onClose={onClose}
      position="right"
      size="lg"
      title={
        subagent ? (
          <Group gap={7} wrap="nowrap">
            <span className={`dot ${statusTone(subagent.status)}`} />
            <Text fw={600}>{subagent.title}</Text>
          </Group>
        ) : null
      }
    >
      {error ? (
        <Text size="sm" c="red">
          {error}
        </Text>
      ) : entries === null ? (
        <Group justify="center" py="xl">
          <Loader size="sm" />
        </Group>
      ) : entries.length === 0 ? (
        <Text size="sm" c="dimmed">
          No transcript is available for this subagent.
        </Text>
      ) : (
        <ScrollArea h="calc(100dvh - 120px)" type="hover">
          <div className="subagent-log">
            {entries.map((entry, index) => (
              <div
                key={`${entry.role}:${index}`}
                className={`subagent-log-entry ${entry.role}`}
              >
                <span className="subagent-log-role">{entry.role}</span>
                <pre>{entry.text}</pre>
              </div>
            ))}
          </div>
        </ScrollArea>
      )}
    </Drawer>
  );
}
