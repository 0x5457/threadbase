import {
  AlertCircle,
  Archive,
  ArrowLeft,
  Bot,
  MoreHorizontal,
  ChevronRight,
  Layers,
  ShieldCheck,
  WifiOff,
} from "lucide-react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import {
  cancelSubagent,
  decideApproval,
  deleteThread,
  forkThread,
  markThreadRead,
  restartAgent,
  requestAccess,
  respondElicitation,
  revertThread,
  updateThread,
} from "@/api/endpoints";
import {
  queryKeys,
  useMe,
  useThreadBundle,
  useThreadEvents,
} from "@/api/queries";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { CommentAnchor } from "@/api/types/CommentAnchor";
import type { ElicitationAction } from "@/api/types/ElicitationAction";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { EventEnvelope } from "@/api/types/EventEnvelope";
import type { SubagentDto } from "@/api/types/SubagentDto";
import { CommentDialog } from "@/components/thread/CommentDialog";
import {
  SelectionComment,
  type SelectionTarget,
} from "@/components/thread/SelectionComment";
import { Composer } from "@/components/thread/Composer";
import { ThreadStatusBadge } from "@/components/thread/ThreadStatusBadge";
import { ThreadTimeline } from "@/components/thread/ThreadTimeline";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  EmptyState,
  Input,
  ErrorState,
  Skeleton,
  buttonVariants,
} from "@/components/ui";
import { useOnlineStatus } from "@/lib/hooks";
import { useLiveStore, useThreadLiveEvents } from "@/lib/live";
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
  const me = useMe();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const bundle = useThreadBundle(threadId);
  const events = useThreadEvents(threadId);
  useThreadLiveEvents(threadId);
  const liveConnected = useLiveStore((state) => state.connected);
  const elicitation = useMutation({
    mutationFn: ({
      elicitationId,
      action,
      values,
    }: {
      elicitationId: string;
      action: ElicitationAction;
      values: Record<string, unknown>;
    }) => respondElicitation(elicitationId, action, values),
    onSuccess: () => {
      if (!threadId) {
        return;
      }
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadEvents(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadBundle(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.inbox("unread"),
      });
    },
  });
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      if (
        modifier &&
        event.altKey &&
        event.key.toLowerCase() === "u" &&
        subagentId &&
        projectId &&
        threadId
      ) {
        event.preventDefault();
        navigate(`/p/${projectId}/t/${threadId}`);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navigate, projectId, subagentId, threadId]);

  const [commentAnchor, setCommentAnchor] = useState<CommentAnchor | null>(null);
  const cancelSubagentMutation = useMutation({
    mutationFn: (subagentId: string) =>
      cancelSubagent(threadId ?? "", subagentId),
    onSuccess: () => {
      if (threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadEvents(threadId),
        });
      }
    },
  });
  const [commentOpen, setCommentOpen] = useState(false);
  const [selectionTarget, setSelectionTarget] = useState<SelectionTarget | null>(
    null,
  );
  useEffect(() => {
    const onMouseUp = () => {
      const selection = window.getSelection();
      if (!selection || selection.isCollapsed) {
        return;
      }
      const text = selection.toString().trim();
      if (!text) {
        return;
      }
      const node = selection.anchorNode;
      const element =
        node instanceof Element ? node : node?.parentElement ?? null;
      const message = element?.closest("[data-message-id]");
      const messageId = message?.getAttribute("data-message-id");
      if (!messageId) {
        return;
      }
      const rect = selection.getRangeAt(0).getBoundingClientRect();
      setSelectionTarget({ messageId, x: rect.left, y: rect.bottom });
    };
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      if (modifier && event.altKey && event.key.toLowerCase() === "m") {
        event.preventDefault();
        const node = window.getSelection()?.anchorNode;
        const element =
          node instanceof Element ? node : node?.parentElement ?? null;
        const message = element?.closest("[data-message-id]");
        const messageId = message?.getAttribute("data-message-id");
        if (messageId) {
          setSelectionTarget({
            messageId,
            x: window.innerWidth / 2,
            y: window.innerHeight / 2,
          });
        } else {
          setCommentAnchor({ kind: "thread" });
          setCommentOpen(true);
        }
      }
    };
    document.addEventListener("mouseup", onMouseUp);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mouseup", onMouseUp);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  useEffect(() => {
    if (!threadId) {
      return;
    }
    void markThreadRead(threadId)
      .then(() => {
        void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
      })
      .catch(() => undefined);
  }, [threadId, queryClient]);

  const archive = useMutation({
    mutationFn: (archived: boolean) =>
      updateThread(threadId ?? "", {
        title: null,
        pinned: null,
        archived,
        shareLevel: null,
        model: null,
        effort: null,
      }),
    onSuccess: (response) => {
      if (!threadId) {
        return;
      }
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadBundle(threadId),
      });
      void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectThreads(response.thread.projectId, false),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectThreads(response.thread.projectId, true),
      });
      if (response.thread.status === "archived") {
        navigate(`/p/${response.thread.projectId}`);
      }
    },
  });
  const [divergedDismissed, setDivergedDismissed] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [renameValue, setRenameValue] = useState("");
  const rename = useMutation({
    mutationFn: () =>
      updateThread(threadId ?? "", {
        title: renameValue.trim(),
        pinned: null,
        archived: null,
        shareLevel: null,
        model: null,
        effort: null,
      }),
    onSuccess: () => {
      setRenameOpen(false);
      if (threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
        void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
        if (projectId) {
          void queryClient.invalidateQueries({
            queryKey: queryKeys.projectThreads(projectId, false),
          });
          void queryClient.invalidateQueries({
            queryKey: queryKeys.projectThreads(projectId, true),
          });
        }
      }
    },
  });
  const discardDiverged = useMutation({
    mutationFn: () => revertThread(threadId ?? ""),
    onSuccess: () => {
      setDivergedDismissed(true);
      if (threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadEvents(threadId),
        });
      }
    },
    onError: (error) => toast.error(String(error)),
  });
  const fork = useMutation({
    mutationFn: () => forkThread(threadId ?? "", null),
    onSuccess: (response) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectThreads(response.thread.projectId, false),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectThreads(response.thread.projectId, true),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectOverview(response.thread.projectId),
      });
      navigate(
        `/p/${encodeURIComponent(response.thread.projectId)}/t/${encodeURIComponent(
          response.thread.id,
        )}`,
      );
    },
  });
  const remove = useMutation({
    mutationFn: () => deleteThread(threadId ?? "", true),
    onSuccess: () => {
      if (projectId) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.projectThreads(projectId, false),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.projectThreads(projectId, true),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.projectOverview(projectId),
        });
        navigate(`/p/${encodeURIComponent(projectId)}`);
      }
    },
  });
  const restart = useMutation({
    mutationFn: () => restartAgent(threadId ?? ""),
    onSuccess: () => {
      if (threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadEvents(threadId),
        });
      }
    },
  });
  const configure = useMutation({
    mutationFn: ({ model, effort }: { model: string; effort: string }) =>
      updateThread(threadId ?? "", {
        title: null,
        pinned: null,
        archived: null,
        shareLevel: null,
        model: model.trim() || null,
        effort: effort.trim() || null,
      }),
    onSuccess: () => {
      if (threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
        void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
      }
    },
  });
  const requestAccessMutation = useMutation({
    mutationFn: () => requestAccess(threadId ?? ""),
    onSuccess: () => toast.success("Access request sent"),
    onError: (error) => toast.error(String(error)),
  });
  const decision = useMutation({
    mutationFn: ({
      approvalId,
      value,
      note,
    }: {
      approvalId: string;
      value: ApprovalDecision;
      note?: string;
    }) => decideApproval(threadId ?? "", approvalId, value, note ?? null),
    onSuccess: () => {
      if (!threadId) {
        return;
      }
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadBundle(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadEvents(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.inbox("unread"),
      });
    },
  });

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

  const { thread, project, approvals, subagents, members } = bundle.data.bundle;
  const myRole = members.find(
    (member) => member.user.id === me.data?.user.id,
  )?.role;
  const canWrite =
    Boolean(me.data?.user.isAdmin) || myRole === "owner" || myRole === "editor";
  const onDecide = (
    approvalId: string,
    value: ApprovalDecision,
    note?: string,
  ) => decision.mutate({ approvalId, value, note });
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
      {!online || !liveConnected ? (
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
          {thread.status === "error" || thread.status === "interrupted" ? (
            <Button
              size="sm"
              variant="danger"
              loading={restart.isPending}
              disabled={!canWrite}
              onClick={() => restart.mutate()}
            >
              Restart agent
            </Button>
          ) : null}
          <button
            type="button"
            className="rounded p-1 text-faint hover:bg-hover hover:text-fg disabled:opacity-40"
            title={thread.status === "archived" ? "Unarchive thread" : "Archive thread"}
            disabled={!canWrite || archive.isPending}
            onClick={() => archive.mutate(thread.status !== "archived")}
          >
            <Archive className="size-3.5" aria-hidden />
          </button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <button
                type="button"
                aria-label="Thread actions"
                className="rounded p-1 text-faint hover:bg-hover hover:text-fg"
              >
                <MoreHorizontal className="size-3.5" aria-hidden />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-40">
                <DropdownMenuItem
                  onSelect={() => {
                    setRenameValue(thread.title);
                    setRenameOpen(true);
                  }}
                >
                  Rename
                </DropdownMenuItem>
                <DropdownMenuItem onSelect={() => fork.mutate()}>
                  Fork thread
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={() => setDeleteOpen(true)}>
                  Delete thread…
                </DropdownMenuItem>
              </DropdownMenuContent>
          </DropdownMenu>
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
            onCancel={() => cancelSubagentMutation.mutate(selectedSubagent.id)}
          />
        ) : (
          <ThreadTimeline
            events={visibleEvents}
            approvals={visibleApprovals}
            loading={events.isLoading}
            error={events.error}
            onRetry={() => void events.refetch()}
            onDecide={onDecide}
            onRespondElicitation={(elicitationId, action, values) =>
              elicitation.mutate({ elicitationId, action, values })
            }
            onComment={(anchor) => {
              setCommentAnchor(anchor);
              setCommentOpen(true);
            }}
            onOpenSubagent={(subagentId) => {
              if (!projectId || !threadId) {
                return;
              }
              navigate(
                `/p/${encodeURIComponent(projectId)}/t/${encodeURIComponent(
                  threadId,
                )}/s/${encodeURIComponent(subagentId)}`,
              );
            }}
          />
        )}
      </div>
      <Dialog open={renameOpen} onOpenChange={setRenameOpen}>
        <DialogContent
          title="Rename thread"
          footer={
            <>
              <Button size="sm" variant="ghost" onClick={() => setRenameOpen(false)}>
                Cancel
              </Button>
              <Button
                size="sm"
                variant="primary"
                loading={rename.isPending}
                disabled={!renameValue.trim()}
                onClick={() => rename.mutate()}
              >
                Save
              </Button>
            </>
          }
        >
          <Input
            autoFocus
            aria-label="Thread title"
            value={renameValue}
            onChange={(event) => setRenameValue(event.target.value)}
          />
        </DialogContent>
      </Dialog>
      <Dialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <DialogContent
          title="Delete thread?"
          description="The worktree and all uncommitted changes will be removed. This cannot be undone."
          footer={
            <>
              <Button size="sm" variant="ghost" onClick={() => setDeleteOpen(false)}>
                Cancel
              </Button>
              <Button
                size="sm"
                variant="danger"
                loading={remove.isPending}
                onClick={() => remove.mutate()}
              >
                Delete
              </Button>
            </>
          }
        >
          <p className="sr-only">Delete this thread and its worktree.</p>
        </DialogContent>
      </Dialog>
      {threadId && selectionTarget ? (
        <SelectionComment
          threadId={threadId}
          target={selectionTarget}
          onClose={() => setSelectionTarget(null)}
        />
      ) : null}
      {threadId ? (
        <CommentDialog
          open={commentOpen}
          onOpenChange={setCommentOpen}
          threadId={threadId}
          anchor={commentAnchor}
        />
      ) : null}
      {thread.diverged && !divergedDismissed ? (
        <div className="flex flex-wrap items-center gap-2 border-t border-warning/30 bg-warning/5 px-4 py-2 text-[11px] text-warning">
          <span>This thread has diverged from the project branch.</span>
          <button
            type="button"
            className="font-medium text-fg underline decoration-warning/50 underline-offset-2 hover:decoration-fg"
            onClick={() => fork.mutate()}
          >
            Keep local changes in a new thread
          </button>
          <button
            type="button"
            className="text-faint hover:text-danger"
            disabled={discardDiverged.isPending}
            onClick={() => discardDiverged.mutate()}
          >
            Discard
          </button>
        </div>
      ) : null}
      {selectedSubagent ? null : (
        <Composer
          threadId={thread.id}
          canWrite={canWrite}
          onRequestAccess={() => requestAccessMutation.mutate()}
          threadStatus={thread.status}
          participants={thread.participants}
          model={thread.model}
          effort={thread.effort}
          onApplyConfig={(model, effort) =>
            configure.mutate({ model, effort })
          }
        />
      )}
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
  onCancel,
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
  onCancel: () => void;
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
              )}/subagents/compare`}
              className={cn(buttonVariants({ size: "sm", variant: "ghost" }))}
            >
              <Layers aria-hidden />
              Compare
            </Link>
            {subagent.canCancel ? (
              <Button size="sm" variant="danger" onClick={onCancel}>
                Cancel subagent
              </Button>
            ) : null}
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
