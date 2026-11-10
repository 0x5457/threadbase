import { ArrowLeftRight, ChevronLeft } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useThreadBundle, useThreadEvents } from "@/api/queries";
import { ThreadTimeline } from "@/components/thread/ThreadTimeline";
import {
  EmptyState,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Skeleton,
} from "@/components/ui";

export function SubagentComparePage() {
  const { projectId, threadId } = useParams();
  const bundle = useThreadBundle(threadId);
  const events = useThreadEvents(threadId);
  const subagents = useMemo(
    () => bundle.data?.bundle.subagents ?? [],
    [bundle.data?.bundle.subagents],
  );
  const [leftId, setLeftId] = useState("");
  const [rightId, setRightId] = useState("");
  const left = leftId || subagents[0]?.id || "";
  const right = rightId || subagents[1]?.id || "";
  const allEvents = events.data?.events ?? [];
  const leftEvents = allEvents.filter((event) => event.subagentId === left);
  const rightEvents = allEvents.filter((event) => event.subagentId === right);

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto">
      <div className="mx-auto w-full max-w-6xl px-5 py-4">
        <Link
          to={
            projectId && threadId
              ? `/p/${encodeURIComponent(projectId)}/t/${encodeURIComponent(threadId)}`
              : "/"
          }
          className="inline-flex items-center gap-1 text-[13px] text-faint hover:text-fg"
        >
          <ChevronLeft className="size-3" aria-hidden />
          Back to thread
        </Link>
        <div className="mt-3 flex flex-wrap items-end justify-between gap-3">
          <div>
            <h1 className="flex items-center gap-2 text-[24px] font-semibold tracking-[-0.03em] text-foreground">
              <ArrowLeftRight className="size-4 text-muted-foreground" aria-hidden />
              Compare subagents
            </h1>
            <p className="text-[13px] text-muted-foreground">
              Two child sessions from the same thread, side by side.
            </p>
          </div>
          <div className="flex gap-1.5">
            <Select value={left} onValueChange={setLeftId}>
              <SelectTrigger className="h-8 w-48">
                <SelectValue placeholder="Left subagent" />
              </SelectTrigger>
              <SelectContent>
                {subagents.map((subagent) => (
                  <SelectItem key={subagent.id} value={subagent.id}>
                    {subagent.title}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Select value={right} onValueChange={setRightId}>
              <SelectTrigger className="h-8 w-48">
                <SelectValue placeholder="Right subagent" />
              </SelectTrigger>
              <SelectContent>
                {subagents.map((subagent) => (
                  <SelectItem key={subagent.id} value={subagent.id}>
                    {subagent.title}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
        {bundle.isLoading ? (
          <Skeleton className="mt-4 h-64 w-full" />
        ) : subagents.length < 2 ? (
          <EmptyState
            title="Not enough subagents"
            description="At least two child sessions are required for a comparison."
            compact
          />
        ) : (
          <div className="mt-4 grid gap-3 lg:grid-cols-2">
            {[left, right].map((id, index) => {
              const subagent = subagents.find((item) => item.id === id);
              const timeline = index === 0 ? leftEvents : rightEvents;
              return (
                <div
                  key={id}
                  className="min-w-0 rounded-xl border border-border/40 bg-card/30"
                >
                  <div className="border-b border-border/40 px-4 py-3">
                    <p className="text-[13px] font-medium text-fg">
                      {subagent?.title ?? "Subagent"}
                    </p>
                    <p className="mt-0.5 text-[11px] text-faint">
                      {subagent?.status ?? "unknown"}
                      {subagent?.cwd ? ` · ${subagent.cwd}` : ""}
                      {subagent?.usage
                        ? ` · ${subagent.usage.totalTokens} tokens`
                        : ""}
                    </p>
                  </div>
                  {timeline.length === 0 ? (
                    <div className="p-4 text-[12px] text-muted-foreground">
                      No child-session transcript was emitted by this adapter.
                      The parent timeline still contains the dispatch card.
                    </div>
                  ) : (
                    <ThreadTimeline events={timeline} approvals={[]} />
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
