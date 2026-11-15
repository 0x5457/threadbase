import { Group, Text } from "@mantine/core";
import { stateLabel } from "../../lib/machine";

export function statusTone(status: string) {
  return status === "completed"
    ? "completed"
    : status === "failed" || status === "error"
      ? "failed"
      : status === "pending" || status === "requires_action"
        ? "waiting"
        : status === "running" || status === "in_progress"
          ? "running"
          : "idle";
}

export function StatusDot({
  status,
  label,
}: {
  status: string;
  /** Machine label when the status belongs to a known machine. */
  label?: string;
}) {
  const tone = statusTone(status);
  return (
    <Group gap={4} className="status-inline" wrap="nowrap">
      <span className={`dot ${tone}`} />
      <Text size="xs" c="dimmed">
        {label ?? stateLabel("thread", status)}
      </Text>
    </Group>
  );
}
