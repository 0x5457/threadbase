import { ArrowLeftRight, ChevronLeft } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useThreadBundle, useThreadEvents } from "@/api/queries";
import { ThreadTimeline } from "@/components/thread/ThreadTimeline";
import { EmptyState, Skeleton } from "@/components/ui";

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
          className="inline-flex items-center gap-1 text-[11px] text-faint hover:text-fg"
        >
          <ChevronLeft className="size-3" aria-hidden />
          Back to thread
        </Link>
        <div className="mt-3 flex flex-wrap items-end justify-between gap-3">
          <div>
            <h1 className="flex items-center gap-2 text-[18px] font-semibold tracking-[-0.02em] text-fg">
              <ArrowLeftRight className="size-4 text-muted" aria-hidden />
              Compare subagents
            </h1>
            <p className="text-[11px] text-muted">
              Two child sessions from the same thread, side by side.
            </p>
          </div>
          <div className="flex gap-1.5">
            <select
              value={left}
              onChange={(event) => setLeftId(event.target.value)}
              className="h-8 max-w-56 rounded-md border border-border bg-panel px-2 text-[11px]"
            >
              {subagents.map((subagent) => (
                <option key={subagent.id} value={subagent.id}>
                  {subagent.title}
                </option>
              ))}
            </select>
            <select
              value={right}
              onChange={(event) => setRightId(event.target.value)}
              className="h-8 max-w-56 rounded-md border border-border bg-panel px-2 text-[11px]"
            >
              {subagents.map((subagent) => (
                <option key={subagent.id} value={subagent.id}>
                  {subagent.title}
                </option>
              ))}
            </select>
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
                  className="min-w-0 rounded-xl border border-border bg-panel"
                >
                  <div className="border-b border-border px-3 py-2">
                    <p className="text-[11px] font-medium text-fg">
                      {subagent?.title ?? "Subagent"}
                    </p>
                    <p className="mt-0.5 text-[9.5px] text-faint">
                      {subagent?.status ?? "unknown"}
                      {subagent?.cwd ? ` · ${subagent.cwd}` : ""}
                      {subagent?.usage
                        ? ` · ${subagent.usage.totalTokens} tokens`
                        : ""}
                    </p>
                  </div>
                  {timeline.length === 0 ? (
                    <div className="p-3 text-[10.5px] text-faint">
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
