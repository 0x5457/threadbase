import { IconMessageCircle } from "@tabler/icons-react";
import { EmptyState } from "./EmptyState";

export function WorkspaceEmpty() {
  return (
    <EmptyState
      icon={<IconMessageCircle size={16} stroke={1.5} />}
      title="Pick a thread"
    >
      Choose a thread in the sidebar, or open the inbox to see what needs your
      attention.
    </EmptyState>
  );
}
