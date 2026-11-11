/* eslint-disable react-refresh/only-export-components */
import { Badge } from "@mantine/core";
import type { ThreadStatus } from "@/api/types/ThreadStatus";

type StatusBadgeVariant = "neutral" | "info" | "warning" | "danger" | "outline";

const STATUS_BADGE_TONE: Record<
  StatusBadgeVariant,
  { color: string; variant: "light" | "outline" }
> = {
  neutral: { color: "gray", variant: "light" },
  info: { color: "blue", variant: "light" },
  warning: { color: "yellow", variant: "light" },
  danger: { color: "red", variant: "light" },
  outline: { color: "gray", variant: "outline" },
};

export function threadStatusLabel(status: ThreadStatus): string {
  switch (status) {
    case "preparing":
      return "Preparing…";
    case "running":
      return "running";
    case "requires_action":
      return "waiting";
    case "interrupted":
      return "interrupted";
    case "error":
      return "error";
    case "archived":
      return "archived";
    default:
      return "idle";
  }
}

function threadStatusVariant(status: ThreadStatus): StatusBadgeVariant {
  switch (status) {
    case "running":
    case "preparing":
      return "info";
    case "requires_action":
      return "warning";
    case "error":
    case "interrupted":
      return "danger";
    case "archived":
      return "outline";
    default:
      return "neutral";
  }
}

export function ThreadStatusBadge({ status }: { status: ThreadStatus }) {
  const tone = STATUS_BADGE_TONE[threadStatusVariant(status)];
  return (
    <Badge
      variant={tone.variant}
      color={tone.color}
      size="xs"
      title={
        status === "preparing"
          ? "Creating worktree → starting agent → binding session"
          : undefined
      }
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
      {threadStatusLabel(status)}
    </Badge>
  );
}
