/* eslint-disable react-refresh/only-export-components */
import type { SubagentStatus } from "@/api/types/SubagentStatus";
import { Badge, type BadgeProps } from "@/components/ui";

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

function subagentStatusVariant(
  status: SubagentStatus,
): NonNullable<BadgeProps["variant"]> {
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
  return (
    <Badge variant={subagentStatusVariant(status)} size="sm">
      <span
        className={
          status === "running"
            ? "size-1.5 animate-pulse rounded-full bg-current"
            : "size-1.5 rounded-full bg-current"
        }
        aria-hidden
      />
      {subagentStatusLabel(status)}
    </Badge>
  );
}
