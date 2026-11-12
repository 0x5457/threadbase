import { Box, Button, Group, Text } from "@mantine/core";
import { IconGitBranch } from "@tabler/icons-react";
import type { Subagent } from "../../api";
import { StatusDot } from "./StatusDot";

export function SubagentList({
  subagents,
  limit,
  cancelling,
  onCancel,
}: {
  subagents: Subagent[];
  limit: number;
  cancelling: string | null;
  onCancel: (subagent: Subagent) => void;
}) {
  if (!subagents.length) return null;
  const visible = subagents.slice(0, limit);

  return (
    <div className="subagents">
      <Text size="xs" c="dimmed" fw={500} className="subagents-title">
        Subagents
      </Text>
      {visible.map((subagent) => (
        <div key={subagent.id} className="subagent-row">
          <Group gap={6} wrap="nowrap">
            <IconGitBranch size={12} stroke={1.5} />
            <Text size="sm" fw={500} className="subagent-title">
              {subagent.title}
            </Text>
            <StatusDot status={subagent.status} />
            <Box style={{ flex: 1 }} />
            {subagent.canCancel &&
            !["cancelled", "error"].includes(subagent.status) ? (
              <Button
                variant="subtle"
                size="compact-xs"
                color="gray"
                loading={cancelling === subagent.id}
                onClick={() => onCancel(subagent)}
              >
                Cancel
              </Button>
            ) : null}
          </Group>
          {subagent.description ? (
            <Text size="xs" c="dimmed" mt={1} className="subagent-description">
              {subagent.description}
            </Text>
          ) : null}
        </div>
      ))}
      {subagents.length > visible.length ? (
        <Text size="xs" c="dimmed">
          +{subagents.length - visible.length} more subagents
        </Text>
      ) : null}
    </div>
  );
}
