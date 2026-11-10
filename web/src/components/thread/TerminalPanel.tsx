import { Terminal } from "lucide-react";
import { useMemo } from "react";
import { useThreadEvents } from "@/api/queries";
import type { ToolCallData } from "@/api/types/ToolCallData";
import { Badge, EmptyState, ErrorState, Skeleton } from "@/components/ui";

export function TerminalPanel({ threadId }: { threadId: string }) {
  const events = useThreadEvents(threadId);
  const calls = useMemo(() => {
    const latest = new Map<string, ToolCallData>();
    for (const event of events.data?.events ?? []) {
      if (
        event.payload.type === "tool_call" ||
        event.payload.type === "tool_call_update"
      ) {
        if (event.payload.call.kind === "execute") {
          latest.set(event.payload.call.id, event.payload.call);
        }
      }
    }
    return [...latest.values()];
  }, [events.data?.events]);

  if (events.isLoading) {
    return (
      <div className="space-y-2 p-3">
        <Skeleton className="h-16 w-full" />
        <Skeleton className="h-24 w-full" />
      </div>
    );
  }
  if (events.isError) {
    return (
      <ErrorState
        error={events.error}
        title="Cannot load command output"
        onRetry={() => void events.refetch()}
        compact
      />
    );
  }
  if (calls.length === 0) {
    return (
      <EmptyState
        icon={Terminal}
        title="No commands run yet"
        compact
      />
    );
  }
  return (
    <div className="space-y-2 p-2.5">
      {calls.map((call) => (
        <div key={call.id} className="overflow-hidden rounded-md border border-border bg-canvas">
          <div className="flex items-center gap-2 border-b border-border px-2 py-1.5">
            <Terminal className="size-3 text-faint" aria-hidden />
            <span className="min-w-0 flex-1 truncate font-mono text-[13px] text-fg">
              {call.title}
            </span>
            <Badge
              size="sm"
              variant={
                call.status === "completed"
                  ? "success"
                  : call.status === "failed"
                    ? "danger"
                    : "info"
              }
            >
              {call.status}
            </Badge>
          </div>
          {call.input ? (
            <pre className="whitespace-pre-wrap break-all border-b border-border/60 px-2 py-1 font-mono text-[9.5px] text-faint">
              $ {call.input}
            </pre>
          ) : null}
          <pre className="max-h-56 overflow-auto whitespace-pre-wrap break-all px-2 py-1.5 font-mono text-[9.5px] text-fg">
            {call.output ?? "(no output)"}
          </pre>
        </div>
      ))}
    </div>
  );
}
