import {
  AlertCircle,
  ArrowLeft,
  Bot,
  ChevronRight,
  GitBranch,
  Layers,
  ShieldCheck,
  WifiOff,
} from "lucide-react";
import { Link, useParams } from "react-router-dom";
import { useThreadBundle, useThreadEvents } from "@/api/queries";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { EventEnvelope } from "@/api/types/EventEnvelope";
import type { SubagentDto } from "@/api/types/SubagentDto";
import { ThreadStatusBadge } from "@/components/thread/ThreadStatusBadge";
import { ThreadTimeline } from "@/components/thread/ThreadTimeline";
import {
  Badge,
  EmptyState,
  ErrorState,
  Skeleton,
  buttonVariants,
} from "@/components/ui";
import { useOnlineStatus } from "@/lib/hooks";
import { formatTokenCount } from "@/lib/format";
import { cn } from "@/lib/utils";

function subagentStatus(status: SubagentDto["status"]) {
  switch (status) {
    case "running":
      return "running" as const;
    case "requires_action":
      return "requires_action" as const;
    case "error":
      return "error" as const;
    case "cancelled":
      return "interrupted" as const;
    default:
      return "idle" as const;
  }
}

export function ThreadPage() {
  const { projectId, threadId, subagentId } = useParams();
  const online = useOnlineStatus();
  const bundle = useThreadBundle(threadId);
  const events = useThreadEvents(threadId);

  if (!projectId || !threadId) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <ErrorState
          title="Thread not found"
          description="The URL does not contain a project id and thread id."
        />
      </div>
    );
  }

  if (bundle.isLoading) {
    return (
      <div className="flex h-full flex-col">
        <div className="flex h-9 items-center gap-2 border-b border-border px-3">
          <Skeleton className="h-4 w-32" />
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-3xl space-y-4 px-5 py-6">
            <Skeleton className="h-16 w-4/5" />
            <Skeleton className="h-28 w-full" />
            <Skeleton className="h-16 w-2/3" />
          </div>
        </div>
      </div>
    );
  }

  if (bundle.isError || !bundle.data) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="w-full max-w-md rounded-xl border border-border bg-panel">
          <ErrorState
            error={bundle.error}
            title="Cannot open thread"
            onRetry={() => void bundle.refetch()}
          />
          <div className="border-t border-border px-4 py-3 text-center">
            <Link
              to={`/p/${encodeURIComponent(projectId)}`}
              className="inline-flex items-center gap-1 text-[11px] text-muted hover:text-fg"
            >
              <ArrowLeft className="size-3" aria-hidden />
              Back to project
            </Link>
          </div>
        </div>
      </div>
    );
  }

  const { thread, project, approvals, subagents } = bundle.data.bundle;
  const selectedSubagent = subagentId
    ? subagents.find((subagent) => subagent.id === subagentId)
    : undefined;
  const threadEvents = events.data?.events ?? [];
  const visibleEvents = selectedSubagent
    ? threadEvents.filter((event) => event.subagentId === selectedSubagent.id)
    : threadEvents;
  const visibleApprovals = selectedSubagent
    ? approvals.filter(
        (approval) => approval.subagentId === selectedSubagent.id,
      )
    : approvals;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {!online ? (
        <div className="flex h-8 shrink-0 items-center gap-2 border-b border-warning/30 bg-warning/10 px-3 text-[11px] text-warning">
          <WifiOff className="size-3.5" aria-hidden />
          Reconnecting… Input is preserved while the connection is offline.
        </div>
      ) : null}
      {events.data?.resync ? (
        <div className="flex h-8 shrink-0 items-center gap-2 border-b border-info/25 bg-info/10 px-3 text-[11px] text-info">
          <AlertCircle className="size-3.5" aria-hidden />
          Event history was compacted. The latest window has been loaded.
        </div>
      ) : null}

      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border bg-panel/70 px-3 text-[11px]">
        <Link
          to={`/p/${encodeURIComponent(project.id)}`}
          className="truncate text-muted hover:text-fg"
        >
          {project.name}
        </Link>
        <ChevronRight className="size-3 shrink-0 text-faint" aria-hidden />
        {selectedSubagent ? (
          <>
            <Link
              to={`/p/${encodeURIComponent(project.id)}/t/${encodeURIComponent(
                thread.id,
              )}`}
              className="max-w-[180px] truncate text-muted hover:text-fg"
            >
              {thread.title}
            </Link>
            <ChevronRight className="size-3 shrink-0 text-faint" aria-hidden />
            <span className="flex min-w-0 items-center gap-1 text-fg">
              <Bot className="size-3 shrink-0 text-faint" aria-hidden />
              <span className="truncate">{selectedSubagent.title}</span>
            </span>
          </>
        ) : (
          <span className="truncate text-fg">{thread.title}</span>
        )}
        <ThreadStatusBadge status={thread.status} />
        <div className="ml-auto flex items-center gap-2.5 text-faint">
          <span className="flex items-center gap-1">
            <GitBranch className="size-3" aria-hidden />
            {thread.branch}
          </span>
          <span className="hidden sm:inline">{thread.agent}</span>
          {thread.model ? (
            <span className="hidden text-muted sm:inline">{thread.model}</span>
          ) : null}
          {thread.pendingApprovals > 0 ? (
            <Badge variant="warning" size="sm">
              <ShieldCheck className="size-2.5" aria-hidden />
              {thread.pendingApprovals} waiting
            </Badge>
          ) : null}
          {thread.queuedInputs > 0 ? (
            <Badge variant="info" size="sm">
              {thread.queuedInputs} queued
            </Badge>
          ) : null}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {selectedSubagent ? (
          <SubagentView
            subagent={selectedSubagent}
            threadId={thread.id}
            threadTitle={thread.title}
            projectId={project.id}
            events={visibleEvents}
            approvals={visibleApprovals}
            loading={events.isLoading}
            error={events.error}
            onRetry={() => void events.refetch()}
          />
        ) : (
          <ThreadTimeline
            events={visibleEvents}
            approvals={visibleApprovals}
            loading={events.isLoading}
            error={events.error}
            onRetry={() => void events.refetch()}
          />
        )}
      </div>
    </div>
  );
}

function SubagentView({
  subagent,
  threadId,
  threadTitle,
  projectId,
  events,
  approvals,
  loading,
  error,
  onRetry,
}: {
  subagent: SubagentDto;
  threadId: string;
  threadTitle: string;
  projectId: string;
  events: EventEnvelope[];
  approvals: ApprovalDto[];
  loading: boolean;
  error: unknown;
  onRetry: () => void;
}) {
  return (
    <div className="flex min-h-full flex-col">
      <div className="border-b border-border bg-panel/60">
        <div className="mx-auto w-full max-w-3xl px-5 py-3">
          <div className="flex flex-wrap items-start gap-2">
            <span className="flex size-7 shrink-0 items-center justify-center rounded-md border border-border bg-raised text-muted">
              <Bot className="size-4" aria-hidden />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <h1 className="truncate text-[13px] font-semibold text-fg">
                  {subagent.title}
                </h1>
                <ThreadStatusBadge status={subagentStatus(subagent.status)} />
              </div>
              {subagent.description ? (
                <p className="mt-0.5 text-[11.5px] leading-relaxed text-muted">
                  {subagent.description}
                </p>
              ) : null}
              <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[10px] text-faint">
                {subagent.cwd ? (
                  <span className="break-all font-mono">cwd: {subagent.cwd}</span>
                ) : null}
                {subagent.usage ? (
                  <span>
                    {formatTokenCount(subagent.usage.totalTokens)} tokens
                  </span>
                ) : null}
                <span className="font-mono">
                  session {subagent.id.slice(0, 12)}
                </span>
              </div>
            </div>
            <Link
              to={`/p/${encodeURIComponent(projectId)}/t/${encodeURIComponent(
                threadId,
              )}`}
              className={cn(buttonVariants({ size: "sm", variant: "secondary" }))}
            >
              <Layers aria-hidden />
              Back to {threadTitle}
            </Link>
          </div>
        </div>
      </div>

      <div className="min-h-0 flex-1">
        {events.length === 0 && !loading && !error ? (
          <div className="mx-auto w-full max-w-3xl px-5 py-8">
            <EmptyState
              icon={Bot}
              title="No subagent activity recorded yet"
              description="The subagent transcript will appear here as it reports progress."
              compact
            />
          </div>
        ) : (
          <ThreadTimeline
            events={events}
            approvals={approvals}
            loading={loading}
            error={error}
            onRetry={onRetry}
          />
        )}
      </div>

      <div className="border-t border-border bg-panel px-5 py-2.5">
        <p className="mx-auto max-w-3xl text-[10.5px] leading-relaxed text-faint">
          Subagent sessions cannot be prompted from the client. Ask the parent
          agent to relay the instruction in the parent thread.
        </p>
      </div>
    </div>
  );
}
