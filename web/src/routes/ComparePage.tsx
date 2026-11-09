import { ArrowLeftRight, ChevronLeft } from "lucide-react";
import { useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import {
  useProjectThreads,
  useThreadBundle,
  useThreadEvents,
} from "@/api/queries";
import type { EventEnvelope } from "@/api/types/EventEnvelope";
import { ThreadStatusBadge } from "@/components/thread/ThreadStatusBadge";
import { EmptyState, Skeleton } from "@/components/ui";

function recentText(events: EventEnvelope[]): string[] {
  return events
    .slice(-40)
    .flatMap((event) => {
      const payload = event.payload;
      if (payload.type === "turn_started") return [`You: ${payload.prompt}`];
      if (payload.type === "message_completed") {
        return [
          `${payload.message.role === "agent" ? "Agent" : "You"}: ${payload.message.text}`,
        ];
      }
      if (payload.type === "tool_call") return [`Tool: ${payload.call.title}`];
      if (payload.type === "turn_error") return [`Error: ${payload.message}`];
      return [];
    })
    .slice(-6);
}

function CompareColumn({ threadId }: { threadId: string }) {
  const bundle = useThreadBundle(threadId);
  const events = useThreadEvents(threadId);
  if (bundle.isLoading) {
    return <Skeleton className="h-64 w-full rounded-xl" />;
  }
  if (!bundle.data) {
    return (
      <EmptyState
        title="Cannot load thread"
        description="The selected thread may have been archived or removed."
        compact
      />
    );
  }
  const data = bundle.data.bundle;
  const recent = recentText(events.data?.events ?? []);
  return (
    <section className="min-w-0 rounded-xl border border-border bg-panel">
      <div className="border-b border-border px-3 py-2.5">
        <div className="flex items-center gap-2">
          <h2 className="min-w-0 flex-1 truncate text-[12px] font-semibold text-fg">
            {data.thread.title}
          </h2>
          <ThreadStatusBadge status={data.thread.status} />
        </div>
        <p className="mt-0.5 text-[10px] text-faint">
          {data.thread.agent}
          {data.thread.model ? ` · ${data.thread.model}` : ""} ·{" "}
          {data.thread.branch}
        </p>
      </div>
      <div className="grid grid-cols-3 gap-1 border-b border-border px-3 py-2 text-center">
        <div>
          <p className="text-[9px] uppercase text-faint">Messages</p>
          <p className="font-mono text-[11px] text-fg">{events.data?.events.length ?? 0}</p>
        </div>
        <div>
          <p className="text-[9px] uppercase text-faint">Changes</p>
          <p className="font-mono text-[11px] text-fg">
            +{data.thread.changes.additions} −{data.thread.changes.deletions}
          </p>
        </div>
        <div>
          <p className="text-[9px] uppercase text-faint">Approvals</p>
          <p className="font-mono text-[11px] text-fg">
            {data.thread.pendingApprovals}
          </p>
        </div>
      </div>
      <div className="space-y-1.5 p-3">
        {recent.length === 0 ? (
          <p className="text-[10.5px] text-faint">No messages yet.</p>
        ) : (
          recent.map((text, index) => (
            <p
              key={`${threadId}-${index}`}
              className="line-clamp-2 rounded bg-canvas px-2 py-1 text-[10.5px] leading-relaxed text-muted"
            >
              {text}
            </p>
          ))
        )}
      </div>
      <div className="border-t border-border px-3 py-1.5">
        <Link
          to={`/p/${encodeURIComponent(data.project.id)}/t/${encodeURIComponent(
            data.thread.id,
          )}`}
          className="text-[10px] text-accent hover:underline"
        >
          Open thread
        </Link>
      </div>
    </section>
  );
}

export function ComparePage() {
  const { projectId } = useParams();
  const threads = useProjectThreads(projectId);
  const list = useMemo(
    () => (threads.data?.threads ?? []).filter((thread) => !thread.archivedAt),
    [threads.data?.threads],
  );
  const [left, setLeft] = useState("");
  const [right, setRight] = useState("");
  const selectedLeft = left || list[0]?.id || "";
  const selectedRight = right || list[1]?.id || "";

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto">
      <div className="mx-auto w-full max-w-6xl px-6 py-5">
        <Link
          to={projectId ? `/p/${encodeURIComponent(projectId)}` : "/"}
          className="inline-flex items-center gap-1 text-[11px] text-faint hover:text-fg"
        >
          <ChevronLeft className="size-3" aria-hidden />
          Back to project
        </Link>
        <div className="mt-3 flex flex-wrap items-end justify-between gap-3">
          <div>
            <h1 className="flex items-center gap-2 text-[19px] font-semibold tracking-[-0.02em] text-fg">
              <ArrowLeftRight className="size-4 text-muted" aria-hidden />
              Compare threads
            </h1>
            <p className="text-[11px] text-muted">
              Same-task runs side by side. Pick any two threads in this project.
            </p>
          </div>
          <div className="flex flex-wrap gap-1.5">
            <select
              value={selectedLeft}
              onChange={(event) => setLeft(event.target.value)}
              className="h-8 max-w-56 rounded-md border border-border bg-panel px-2 text-[11px]"
            >
              {list.map((thread) => (
                <option key={thread.id} value={thread.id}>
                  {thread.title} · {thread.agent}
                </option>
              ))}
            </select>
            <select
              value={selectedRight}
              onChange={(event) => setRight(event.target.value)}
              className="h-8 max-w-56 rounded-md border border-border bg-panel px-2 text-[11px]"
            >
              {list.map((thread) => (
                <option key={thread.id} value={thread.id}>
                  {thread.title} · {thread.agent}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="mt-4 grid gap-3 lg:grid-cols-2">
          {selectedLeft ? (
            <CompareColumn threadId={selectedLeft} />
          ) : (
            <EmptyState title="No thread selected" compact />
          )}
          {selectedRight ? (
            <CompareColumn threadId={selectedRight} />
          ) : (
            <EmptyState title="Create another thread to compare" compact />
          )}
        </div>
      </div>
    </div>
  );
}
