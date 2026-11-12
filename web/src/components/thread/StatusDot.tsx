import { Group, Text } from "@mantine/core";

export function StatusDot({ status }: { status: string }) {
  const tone =
    status === "completed"
      ? "completed"
      : status === "failed" || status === "error"
        ? "failed"
        : status === "pending" || status === "requires_action"
          ? "waiting"
          : status === "running" || status === "in_progress"
            ? "running"
            : "idle";
  return (
    <Group gap={4} className="status-inline" wrap="nowrap">
      <span className={`dot ${tone}`} />
      <Text size="xs" c="dimmed">
        {status.replaceAll("_", " ")}
      </Text>
    </Group>
  );
}
