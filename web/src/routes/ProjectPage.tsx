import {
  ChevronDown,
  ChevronLeft,
  FolderGit2,
  GitBranch,
  MessageSquare,
  Pin,
  Plus,
  ShieldCheck,
} from "lucide-react";
import { useState } from "react";
import { Link, useNavigate, useOutletContext, useParams } from "react-router-dom";
import { useProject, useProjectOverview, useProjectThreads } from "@/api/queries";
import { Archive, ArrowLeftRight } from "lucide-react";
import type { ThreadDto } from "@/api/types/ThreadDto";
import { ThreadStatusBadge } from "@/components/thread/ThreadStatusBadge";
import {
  AvatarStack,
  Badge,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
  EmptyState,
  ErrorState,
  Input,
  Skeleton,
} from "@/components/ui";
import { formatRelativeTime } from "@/lib/format";
import type {
  NewThreadOptions,
  WorkspaceOutletContext,
} from "./workspace-context";

function threadPath(thread: ThreadDto): string {
  return `/p/${encodeURIComponent(thread.projectId)}/t/${encodeURIComponent(
    thread.id,
  )}`;
}

function ThreadRow({
  thread,
  subthreadCount,
}: {
  thread: ThreadDto;
  subthreadCount: number;
}) {
  const people = thread.participants.map((participant) => ({
    id: participant.user.id,
    displayName: participant.user.displayName,
    avatarUrl: participant.user.avatarUrl,
    color: participant.user.color,
  }));

  return (
    <Link
      to={threadPath(thread)}
      className="flex items-center gap-3 border-b border-border/70 px-3 py-2.5 transition-colors last:border-b-0 hover:bg-hover"
    >
      <span className="flex size-7 shrink-0 items-center justify-center rounded-md border border-border bg-canvas text-faint">
        <MessageSquare className="size-3.5" aria-hidden />
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <span className="truncate text-[12px] font-medium text-fg">
            {thread.title}
          </span>
          {thread.pinned ? (
            <Pin className="size-3 shrink-0 text-faint" aria-label="Pinned" />
          ) : null}
          <ThreadStatusBadge status={thread.status} />
        </div>
        <div className="mt-0.5 flex items-center gap-2 text-[10.5px] text-faint">
          <span>{thread.agent}</span>
          {subthreadCount > 0 ? (
            <span>{subthreadCount} subthreads</span>
          ) : null}
          <span>{formatRelativeTime(thread.updatedAt)}</span>
        </div>
      </div>
      <div className="hidden shrink-0 items-center gap-2.5 sm:flex">
        {thread.pendingApprovals > 0 ? (
          <Badge variant="warning" size="sm">
            <ShieldCheck className="size-2.5" aria-hidden />
            {thread.pendingApprovals}
          </Badge>
        ) : null}
        <span className="font-mono text-[10px] tabular-nums">
          <span className="text-success">+{thread.changes.additions}</span>{" "}
          <span className="text-danger">−{thread.changes.deletions}</span>
        </span>
        {people.length > 0 ? <AvatarStack people={people} max={3} size="xs" /> : null}
      </div>
    </Link>
  );
}

export function ProjectPage() {
  const { projectId } = useParams();
  const { openNewThread } = useOutletContext<WorkspaceOutletContext>();
  const navigate = useNavigate();
  const [showArchived, setShowArchived] = useState(false);
  const [newAgent, setNewAgent] = useState("");
  const [newModel, setNewModel] = useState("");
  const [newBranch, setNewBranch] = useState("");
  const [newThreadOpen, setNewThreadOpen] = useState(false);
  const project = useProject(projectId);
  const threads = useProjectThreads(projectId, showArchived);
  const overview = useProjectOverview(projectId);

  if (!projectId) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <ErrorState
          title="Project not found"
          description="The URL does not contain a project id."
        />
      </div>
    );
  }

  if (project.isLoading) {
    return (
      <div className="mx-auto w-full max-w-4xl space-y-4 px-6 py-7">
        <Skeleton className="h-6 w-48" />
        <Skeleton className="h-4 w-72" />
        <Skeleton className="h-24 w-full rounded-xl" />
        <Skeleton className="h-40 w-full rounded-xl" />
      </div>
    );
  }

  if (project.isError || !project.data) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="w-full max-w-md rounded-xl border border-border bg-panel">
          <ErrorState
            error={project.error}
            title="Cannot open project"
            onRetry={() => void project.refetch()}
          />
          <div className="border-t border-border px-4 py-3 text-center">
            <Link
              to="/"
              className="inline-flex items-center gap-1 text-[11px] text-muted hover:text-fg"
            >
              <ChevronLeft className="size-3" aria-hidden />
              Back to projects
            </Link>
          </div>
        </div>
      </div>
    );
  }

  const data = project.data.project;
  const threadList = threads.data?.threads ?? [];
  const rootThreads = threadList
    .filter(
      (thread) =>
        !thread.parentThreadId ||
        !threadList.some((candidate) => candidate.id === thread.parentThreadId),
    )
    .sort((a, b) => Number(b.updatedAt) - Number(a.updatedAt));
  const childCount = new Map<string, number>();
  for (const thread of threadList) {
    if (thread.parentThreadId) {
      childCount.set(
        thread.parentThreadId,
        (childCount.get(thread.parentThreadId) ?? 0) + 1,
      );
    }
  }
  const active = threadList.filter(
    (thread) => thread.status === "running" || thread.status === "preparing",
  ).length;
  const waiting = threadList.reduce(
    (total, thread) => total + thread.pendingApprovals,
    0,
  );
  const additions = threadList.reduce(
    (total, thread) => total + thread.changes.additions,
    0,
  );
  const deletions = threadList.reduce(
    (total, thread) => total + thread.changes.deletions,
    0,
  );

  return (
    <div className="flex h-full min-h-0 flex-col overflow-y-auto">
      <div className="mx-auto w-full max-w-4xl px-6 py-6">
        <Link
          to="/"
          className="mb-4 inline-flex items-center gap-1 text-[11px] text-faint transition-colors hover:text-fg"
        >
          <ChevronLeft className="size-3" aria-hidden />
          Projects
        </Link>

        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="flex min-w-0 items-start gap-3">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-xl border border-border bg-panel text-muted">
              <FolderGit2 className="size-5" aria-hidden />
            </span>
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <h1 className="truncate text-[20px] font-semibold tracking-[-0.02em] text-fg">
                  {data.name}
                </h1>
                {!data.available ? (
                  <Badge variant="danger" size="sm">
                    path unavailable
                  </Badge>
                ) : null}
              </div>
              <p className="truncate font-mono text-[11px] text-faint">
                {data.path}
              </p>
              <p className="mt-1 flex items-center gap-1 text-[10.5px] text-muted">
                <GitBranch className="size-2.5" aria-hidden />
                {data.defaultBranch}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="secondary"
              size="md"
              onClick={() =>
                navigate(`/p/${encodeURIComponent(projectId)}/compare`)
              }
            >
              <ArrowLeftRight aria-hidden />
              Compare
            </Button>
            <Button
              variant="secondary"
              size="md"
              onClick={() => setShowArchived((value) => !value)}
            >
              <Archive aria-hidden />
              {showArchived ? "Hide archived" : "Archived"}
            </Button>
            <div className="flex items-center">
              <Button
                variant="primary"
                size="md"
                className="rounded-r-none"
                onClick={() => openNewThread(data.id)}
              >
                <Plus aria-hidden />
                New thread
              </Button>
              <DropdownMenu open={newThreadOpen} onOpenChange={setNewThreadOpen}>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="primary"
                    size="md"
                    className="rounded-l-none border-l border-white/20 px-2"
                    aria-label="New thread options"
                  >
                    <ChevronDown className="size-3.5" aria-hidden />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-64 p-2">
                  <div className="space-y-2">
                    <label className="block text-[10.5px] text-muted">
                      Agent
                      <select
                        value={newAgent}
                        onChange={(event) => setNewAgent(event.target.value)}
                        className="mt-1 h-7 w-full rounded-md border border-border bg-surface px-2 text-[11px] text-fg"
                      >
                        <option value="">Default</option>
                        <option value="codex">Codex</option>
                        <option value="claude">Claude Code</option>
                        <option value="opencode">opencode</option>
                        <option value="pi">pi</option>
                      </select>
                    </label>
                    <label className="block text-[10.5px] text-muted">
                      Model
                      <Input
                        value={newModel}
                        onChange={(event) => setNewModel(event.target.value)}
                        placeholder="Agent default"
                        className="mt-1 h-7 text-[11px]"
                      />
                    </label>
                    <label className="block text-[10.5px] text-muted">
                      Base branch
                      <Input
                        value={newBranch}
                        onChange={(event) => setNewBranch(event.target.value)}
                        placeholder={data.defaultBranch}
                        className="mt-1 h-7 font-mono text-[11px]"
                      />
                    </label>
                    <Button
                      className="w-full"
                      size="sm"
                      variant="primary"
                      onClick={() => {
                        const options: NewThreadOptions = {
                          agent: newAgent || null,
                          model: newModel || null,
                          baseBranch: newBranch || null,
                        };
                        openNewThread(data.id, options);
                        setNewThreadOpen(false);
                      }}
                    >
                      Create
                    </Button>
                  </div>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>
        </div>

        <div className="mt-5 flex flex-wrap items-center gap-x-3 text-[10.5px] text-faint">
          <span>{threadList.length} threads</span>
          {active > 0 ? <span>{active} running</span> : null}
          {waiting > 0 ? <span className="text-warning">{waiting} waiting</span> : null}
          <span className="font-mono">
            <span className="text-success">+{additions}</span>{" "}
            <span className="text-danger">−{deletions}</span>
          </span>
        </div>

        {overview.data?.overview.recentActivity[0] ? (
          <p className="mt-2 truncate text-[10.5px] text-faint">
            Last: {overview.data.overview.recentActivity[0].actor.displayName}{" "}
            {overview.data.overview.recentActivity[0].summary}
            <span className="mx-1.5">·</span>
            {formatRelativeTime(
              overview.data.overview.recentActivity[0].ts,
            )}
          </p>
        ) : null}

        <div className="mt-3 border-y border-border/60">
          {threads.isLoading ? (
            <div className="space-y-1 py-1">
              <Skeleton className="h-12 w-full" />
              <Skeleton className="h-12 w-full" />
              <Skeleton className="h-12 w-3/4" />
            </div>
          ) : threads.isError ? (
            <ErrorState
              error={threads.error}
              title="Cannot load threads"
              onRetry={() => void threads.refetch()}
              compact
            />
          ) : rootThreads.length === 0 ? (
            <EmptyState
              icon={MessageSquare}
              title="No threads"
              compact
              action={
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => openNewThread(data.id)}
                >
                  <Plus aria-hidden />
                  New thread
                </Button>
              }
            />
          ) : (
            rootThreads.map((thread) => (
              <ThreadRow
                key={thread.id}
                thread={thread}
                subthreadCount={childCount.get(thread.id) ?? 0}
              />
            ))
          )}
        </div>
      </div>
    </div>
  );
}
