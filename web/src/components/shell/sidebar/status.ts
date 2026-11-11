import type { SubagentStatus } from "@/api/types/SubagentStatus";
import type { ThreadStatus } from "@/api/types/ThreadStatus";

export type AnyStatus = ThreadStatus | SubagentStatus;

export function statusColor(status: AnyStatus): string {
  switch (status) {
    case "running":
      return "var(--mantine-color-teal-filled)";
    case "preparing":
      return "var(--mantine-color-blue-filled)";
    case "requires_action":
      return "var(--mantine-color-yellow-filled)";
    case "error":
      return "var(--mantine-color-red-filled)";
    case "interrupted":
      return "var(--mantine-color-red-filled)";
    case "archived":
      return "var(--mantine-color-gray-6)";
    default:
      return "var(--mantine-color-gray-6)";
  }
}

export function statusLabel(status: AnyStatus): string {
  switch (status) {
    case "running":
      return "Running";
    case "preparing":
      return "Preparing";
    case "requires_action":
      return "Waiting for approval";
    case "error":
      return "Error";
    case "interrupted":
      return "Interrupted";
    case "archived":
      return "Archived";
    case "cancelled":
      return "Cancelled";
    default:
      return "Idle";
  }
}
