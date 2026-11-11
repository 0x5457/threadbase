/* eslint-disable react-refresh/only-export-components */
import { Badge } from "@mantine/core";
import type { SubagentStatus } from "@/api/types/SubagentStatus";

type StatusBadgeVariant = "neutral" | "info" | "warning" | "danger";

const STATUS_BADGE_TONE: Record<
  StatusBadgeVariant,
  { color: string; variant: "light" }
> = {
  neutral: { color: "gray", variant: "light" },
  info: { color: "blue", variant: "light" },
  warning: { color: "yellow", variant: "light" },
  danger: { color: "red", variant: "light" },
};

export function subagentStatusLabel(status: SubagentStatus): string {
  switch (status) {
    case "running":
      return "running";
    case "requires_action":
      return "waiting";
    case "error":
      return "error";
    case "cancelled":
      return "cancelled";
    default:
      return "idle";
  }
}

function subagentStatusVariant(status: SubagentStatus): StatusBadgeVariant {
  switch (status) {
    case "running":
      return "info";
    case "requires_action":
      return "warning";
    case "error":
      return "danger";
    default:
      return "neutral";
  }
}

/**
 * Renders the subagent lifecycle status directly. Subagent statuses are not
 * thread statuses; they come from the same server projection and must not be
 * translated into a ThreadStatus.
 */
export function SubagentStatusBadge({ status }: { status: SubagentStatus }) {
  const tone = STATUS_BADGE_TONE[subagentStatusVariant(status)];
  return (
    <Badge
      variant={tone.variant}
      color={tone.color}
      size="xs"
      leftSection={
        <span
          aria-hidden
          style={{
            width: 6,
            height: 6,
            borderRadius: "50%",
            backgroundColor: "currentColor",
          }}
        />
      }
    >
      {subagentStatusLabel(status)}
    </Badge>
  );
}
