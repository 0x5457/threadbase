import { IconArrowBackUp } from "@tabler/icons-react";
import { EmptyState } from "../EmptyState";

export function EmptyThread({ agent }: { agent: string }) {
  return (
    <EmptyState icon={<IconArrowBackUp size={16} stroke={1.5} />} title="No activity yet">
      Send a message below to start {agent}. Every tool call, approval and code
      change will appear in this thread.
    </EmptyState>
  );
}
