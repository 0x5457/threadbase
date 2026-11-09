/* eslint-disable react-refresh/only-export-components */
import type { ThreadStatus } from "@/api/types/ThreadStatus";
import { Badge, type BadgeProps } from "@/components/ui";

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

function threadStatusVariant(
  status: ThreadStatus,
): NonNullable<BadgeProps["variant"]> {
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
  return (
    <Badge
      variant={threadStatusVariant(status)}
      size="sm"
      title={
        status === "preparing"
          ? "Creating worktree → starting agent → binding session"
          : undefined
      }
    >
      <span
        className={
          status === "running" || status === "preparing"
            ? "size-1.5 animate-pulse rounded-full bg-current"
            : "size-1.5 rounded-full bg-current"
        }
        aria-hidden
      />
      {threadStatusLabel(status)}
    </Badge>
  );
}
