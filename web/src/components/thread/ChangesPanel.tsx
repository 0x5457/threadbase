import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check, GitCompareArrows, RotateCcw } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { createComment, mergeBackChanges, setSeen } from "@/api/endpoints";
import { queryKeys, useChanges, useComments, useFileDiff } from "@/api/queries";
import type { BaselineKind } from "@/api/types/BaselineKind";
import type { CommentDto } from "@/api/types/CommentDto";
import type { DiffLineDto } from "@/api/types/DiffLineDto";
import type { FileChangeDto } from "@/api/types/FileChangeDto";
import type { FileStatus } from "@/api/types/FileStatus";
import {
  Badge,
  Button,
  Checkbox,
  EmptyState,
  ErrorState,
  Skeleton,
  Textarea,
} from "@/components/ui";
import { cn } from "@/lib/utils";

const BASELINES: { value: BaselineKind; label: string }[] = [
  { value: "base_branch", label: "Base branch" },
  { value: "last_commit", label: "Last commit" },
  { value: "last_turn", label: "Last turn" },
];

function statusLabel(status: FileStatus) {
  switch (status) {
    case "added":
      return "A";
    case "deleted":
      return "D";
    case "renamed":
      return "R";
    case "untracked":
      return "U";
    default:
      return "M";
  }
}

function statusClass(status: FileStatus) {
  switch (status) {
    case "added":
    case "untracked":
      return "border-success/30 bg-success/10 text-success";
    case "deleted":
      return "border-danger/30 bg-danger/10 text-danger";
    case "renamed":
      return "border-info/30 bg-info/10 text-info";
    default:
      return "border-border bg-raised text-muted";
  }
}

function lineClass(line: DiffLineDto) {
  if (line.kind === "add") {
    return "bg-add text-success";
  }
  if (line.kind === "delete") {
    return "bg-del text-danger";
  }
  return "text-muted";
}

export function ChangesPanel({
  threadId,
  canWrite,
}: {
  threadId: string;
  canWrite: boolean;
}) {
  const queryClient = useQueryClient();
  const [baseline, setBaseline] = useState<BaselineKind>("base_branch");
  const [selected, setSelected] = useState<string | null>(null);
  const [commentTarget, setCommentTarget] = useState<{
    hunkId: string;
    line: number;
    contextHash: string;
  } | null>(null);
  const [commentBody, setCommentBody] = useState("");
  const changes = useChanges(threadId, baseline);
  const diff = useFileDiff(threadId, selected, baseline);
  const comments = useComments(threadId);
  const invalidate = () => {
    void queryClient.invalidateQueries({
      queryKey: queryKeys.threadBundle(threadId),
    });
    void queryClient.invalidateQueries({
      queryKey: queryKeys.changes(threadId, baseline),
    });
  };
  const seen = useMutation({
    mutationFn: ({ file, value }: { file: FileChangeDto; value: boolean }) =>
      setSeen(threadId, [file.path], value),
    onSuccess: invalidate,
    onError: (error) => toast.error(String(error)),
  });
  const addComment = useMutation({
    mutationFn: () => {
      if (!selected || !commentTarget) {
        throw new Error("Select a diff line first");
      }
      return createComment(threadId, {
        body: commentBody.trim(),
        anchor: {
          kind: "diff",
          path: selected,
          hunkId: commentTarget.hunkId,
          line: commentTarget.line,
          contextHash: commentTarget.contextHash,
        },
        parentId: null,
        send: false,
      });
    },
    onSuccess: () => {
      setCommentBody("");
      setCommentTarget(null);
      void queryClient.invalidateQueries({
        queryKey: queryKeys.comments(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadEvents(threadId),
      });
    },
    onError: (error) => toast.error(String(error)),
  });
  const markAllSeen = useMutation({
    mutationFn: () => setSeen(threadId, (changes.data?.files ?? []).map((file) => file.path), true),
    onSuccess: invalidate,
    onError: (error) => toast.error(String(error)),
  });
  const merge = useMutation({
    mutationFn: () => mergeBackChanges(threadId, false, null),
    onSuccess: (result) => {
      toast.success(`Applied ${result.files.length} files to the project`);
      invalidate();
      void queryClient.invalidateQueries({ queryKey: queryKeys.projects });
    },
    onError: (error) => toast.error(String(error)),
  });

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();
      if (modifier && event.shiftKey && key === "y") {
        event.preventDefault();
        markAllSeen.mutate();
      }
      if (modifier && event.altKey && key === "y" && selected) {
        event.preventDefault();
        const file = changes.data?.files.find((entry) => entry.path === selected);
        if (file) {
          seen.mutate({ file, value: !file.seen });
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [changes.data?.files, markAllSeen, seen, selected]);

  if (changes.isLoading) {
    return (
      <div className="space-y-2 px-3 py-3">
        <Skeleton className="h-5 w-40" />
        <Skeleton className="h-10 w-full" />
        <Skeleton className="h-16 w-full" />
      </div>
    );
  }
  if (changes.isError) {
    return (
      <ErrorState
        error={changes.error}
        title="Cannot load changes"
        onRetry={() => void changes.refetch()}
        compact
      />
    );
  }
  const data = changes.data;
  if (!data || data.files.length === 0) {
    return (
      <EmptyState
        icon={GitCompareArrows}
        title="No file changes yet."
        description="Changes appear here after the agent edits files."
        compact
      />
    );
  }

  const fileComments = (comments.data?.comments ?? []).filter(
    (comment): comment is CommentDto & {
      anchor: { kind: "diff"; path: string; hunkId: string; line: number };
    } =>
      comment.anchor.kind === "diff" && comment.anchor.path === selected,
  );

  return (
    <div className="flex min-h-0 flex-col">
      <div className="space-y-2 border-b border-border px-3 py-2.5">
        <div className="flex items-center gap-2">
          <select
            value={baseline}
            onChange={(event) =>
              setBaseline(event.target.value as BaselineKind)
            }
            className="h-7 min-w-0 flex-1 rounded-md border border-border bg-surface px-2 text-[10.5px] text-muted"
          >
            {BASELINES.map((entry) => (
              <option key={entry.value} value={entry.value}>
                {entry.label}
              </option>
            ))}
          </select>
          <span className="font-mono text-[10px] text-faint">
            {data.stats.seen}/{data.stats.total}
          </span>
        </div>
        <div className="flex items-center justify-between text-[10.5px]">
          <span className="text-muted">{data.baseLabel}</span>
          <span className="font-mono">
            <span className="text-success">+{data.stats.additions}</span>{" "}
            <span className="text-danger">−{data.stats.deletions}</span>
          </span>
        </div>
        {canWrite ? (
          <Button
            size="sm"
            variant="secondary"
            className="w-full"
            loading={merge.isPending}
            onClick={() => {
              if (
                window.confirm(
                  "Apply these changes to the project working tree? Nothing is committed.",
                )
              ) {
                merge.mutate();
              }
            }}
          >
            <RotateCcw aria-hidden />
            Apply to project
          </Button>
        ) : null}
      </div>

      <ul className="shrink-0 border-b border-border">
        {data.files.map((file) => (
          <li
            key={file.path}
            className={cn(
              "flex items-center gap-2 border-b border-border/60 px-3 py-1.5 last:border-b-0",
              selected === file.path && "bg-hover",
            )}
          >
            <Checkbox
              checked={file.seen}
              aria-label={`Mark ${file.path} as seen`}
              onCheckedChange={(value) =>
                seen.mutate({ file, value: value === true })
              }
            />
            <button
              type="button"
              className="flex min-w-0 flex-1 items-center gap-2 text-left"
              onClick={() =>
                setSelected(selected === file.path ? null : file.path)
              }
            >
              <span
                className={cn(
                  "flex size-5 shrink-0 items-center justify-center rounded border font-mono text-[10px] font-semibold",
                  statusClass(file.status),
                )}
              >
                {statusLabel(file.status)}
              </span>
              <span className="min-w-0 flex-1 truncate font-mono text-[10.5px] text-fg">
                {file.path}
              </span>
              <span className="shrink-0 font-mono text-[9.5px]">
                <span className="text-success">+{file.additions}</span>{" "}
                <span className="text-danger">−{file.deletions}</span>
              </span>
            </button>
          </li>
        ))}
      </ul>

      {selected ? (
        <div className="min-h-0 flex-1 overflow-auto">
          {diff.isLoading ? (
            <div className="p-3">
              <Skeleton className="h-24 w-full" />
            </div>
          ) : diff.isError ? (
            <ErrorState
              error={diff.error}
              title="Cannot load diff"
              onRetry={() => void diff.refetch()}
              compact
            />
          ) : diff.data?.binary ? (
            <div className="p-3">
              <Badge size="sm" variant="outline">
                binary file
              </Badge>
            </div>
          ) : (
            <div className="font-mono text-[10px]">
              {diff.data?.hunks.map((hunk) => (
                <div key={hunk.id}>
                  <div className="sticky top-0 border-y border-border bg-raised px-2 py-1 text-[9.5px] text-faint">
                    {hunk.header}
                  </div>
                  {hunk.lines.map((line, index) => {
                    const lineNumber = line.newLine ?? line.oldLine ?? 0;
                    return (
                      <button
                        type="button"
                        key={`${hunk.id}-${index}`}
                        onClick={() =>
                          setCommentTarget({
                            hunkId: hunk.id,
                            line: lineNumber,
                            contextHash: hunk.contextHash,
                          })
                        }
                        className={cn(
                          "grid w-full grid-cols-[2.5rem_2.5rem_1fr] gap-2 px-2 py-px text-left hover:bg-hover",
                          lineClass(line),
                        )}
                      >
                        <span className="text-right text-faint/70">
                          {line.oldLine ?? ""}
                        </span>
                        <span className="text-right text-faint/70">
                          {line.newLine ?? ""}
                        </span>
                        <span className="whitespace-pre-wrap break-all">
                          {line.content || " "}
                        </span>
                      </button>
                    );
                  })}
                  {fileComments
                    .filter((comment) => comment.anchor.hunkId === hunk.id)
                    .map((comment) => (
                      <div
                        key={comment.id}
                        className="border-y border-info/20 bg-info/5 px-2 py-1.5"
                      >
                        <p className="text-[9.5px] text-info">
                          line {comment.anchor.line} · {comment.author.displayName}
                          {comment.status === "pending" ? " · pending" : ""}
                        </p>
                        <p className="whitespace-pre-wrap text-[10.5px] text-fg">
                          {comment.body}
                        </p>
                      </div>
                    ))}
                </div>
              ))}
              {commentTarget ? (
                <div className="space-y-1.5 border-t border-border bg-panel p-2.5">
                  <p className="text-[10px] text-info">
                    Comment on {selected}:{commentTarget.line}
                  </p>
                  <Textarea
                    value={commentBody}
                    onChange={(event) => setCommentBody(event.target.value)}
                    placeholder="Leave a review comment…"
                    rows={2}
                    className="text-[11px]"
                  />
                  <div className="flex justify-end gap-1.5">
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => setCommentTarget(null)}
                    >
                      Cancel
                    </Button>
                    <Button
                      size="sm"
                      variant="primary"
                      disabled={!commentBody.trim()}
                      loading={addComment.isPending}
                      onClick={() => addComment.mutate()}
                    >
                      Add pending comment
                    </Button>
                  </div>
                </div>
              ) : null}
            </div>
          )}
        </div>
      ) : (
        <div className="p-3 text-[10.5px] text-faint">
          <Check className="mr-1 inline size-3" aria-hidden />
          Select a file to review its diff.
        </div>
      )}
    </div>
  );
}
