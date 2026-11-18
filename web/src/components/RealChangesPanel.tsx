import {
  ActionIcon,
  Box,
  Button,
  Group,
  Text,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import type { AnnotationSide, DiffLineAnnotation } from "@pierre/diffs/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Virtuoso } from "react-virtuoso";
import { formatPatch, type StructuredPatch } from "diff";
import {
  IconArrowLeft,
  IconFile,
  IconFiles,
  IconGitBranch,
  IconHistory,
  IconTextWrap,
  IconTextWrapDisabled,
  IconX,
} from "@tabler/icons-react";
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import {
  createComment,
  deleteComment,
  getCommitChanges,
  getCommitFileDiff,
  getFileDiff,
  getMe,
  getThreadBundle,
  getTurnChanges,
  getTurnFileDiff,
  sendComment,
  updateComment,
  type Comment,
  type CommentAnchor,
  type CommentSide,
  type CommitSummary,
  type FileChange,
  type FileDiff,
} from "../api";
import {
  toggleDiffWrap,
  useDiffWrap,
} from "../lib/diff-wrap";
import {
  exceedsLineBudget,
  MAX_CHANGE_FILES,
  MAX_DIFF_LINES,
} from "../lib/diff-size";
import { CommentCard, CommentDraft } from "./thread/CommentCard";
import { DiffView, type DiffCommentMeta } from "./DiffView";
import { useDelayedFlag } from "../lib/use-delayed-flag";
import { ListSkeleton, DiffSkeleton } from "./loading/Skeleton";
import { LoadError } from "./loading/LoadError";
import {
  insertOptimisticComment,
  removeOptimisticComment,
  replaceOptimisticComment,
} from "../lib/comment-cache";

function patchFromDiff(diff: FileDiff, oldPath: string | null) {
  const previousPath = oldPath ?? diff.oldPath ?? diff.path;
  const patch: StructuredPatch = {
    oldFileName: `a/${previousPath}`,
    newFileName: `b/${diff.path}`,
    oldHeader: undefined,
    newHeader: undefined,
    isGit: true,
    hunks: diff.hunks.map((hunk) => ({
      // `diff` subtracts one from a zero-length range start and emits an
      // invalid `@@ --1,0` header for added files. Feed it the equivalent
      // one-based start so the emitted header is `@@ -0,0`.
      oldStart: hunk.oldLines === 0 ? 1 : hunk.oldStart,
      oldLines: hunk.oldLines,
      newStart: hunk.newLines === 0 ? 1 : hunk.newStart,
      newLines: hunk.newLines,
      lines: hunk.lines.map((line) => {
        const prefix =
          line.kind === "add" ? "+" : line.kind === "delete" ? "-" : " ";
        return `${prefix}${line.content}`;
      }),
    })),
  };
  return formatPatch(patch);
}

const statusLetter: Record<string, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  untracked: "U",
};

interface CommentTarget {
  key: string;
  path: string;
  hunkId: string;
  line: number;
  side: CommentSide;
  contextHash: string | null;
}

function FileRow({
  file,
  active,
  onClick,
}: {
  file: FileChange;
  active: boolean;
  onClick: () => void;
}) {
  // A plain button keeps a 139-file change list cheap to mount; Mantine's
  // NavLink renders a full component tree per row and made switching the
  // Changes panel visibly stall.
  return (
    <button
      type="button"
      className="change-row"
      data-active={active ? "true" : undefined}
      onClick={onClick}
      title={file.path}
    >
      <IconFile size={13} stroke={1.5} className="change-icon" />
      <span className="change-path">{file.path}</span>
      <span className="change-stats">
        <span className="stat-add">+{file.additions}</span>
        <span className="stat-del">−{file.deletions}</span>
        <span className={`change-status file-status ${file.status}`}>
          {statusLetter[file.status] ?? "?"}
        </span>
      </span>
    </button>
  );
}

/** The side a stored anchor points at; older anchors only carry the number. */
function anchorSide(diff: FileDiff, hunkId: string, line: number, side: CommentSide | null) {
  if (side) return side;
  const hunk =
    diff.hunks.find((entry) => entry.id === hunkId) ??
    diff.hunks.find((entry) =>
      entry.lines.some((entryLine) => entryLine.newLine === line),
    );
  if (!hunk) return null;
  if (hunk.lines.some((entryLine) => entryLine.newLine === line)) return "new";
  if (hunk.lines.some((entryLine) => entryLine.oldLine === line)) return "old";
  return null;
}

export const RealChangesPanel = memo(function RealChangesPanel({
  threadId,
  active = true,
  commit,
  turn,
  onClose,
  onShowFiles,
  onShowHistory,
  onShowCurrent,
  onShowTurn,
}: {
  threadId: string;
  /**
   * The Changes tab is the visible one. A hidden tab stays mounted (so its
   * scroll and parsed diff survive a switch) but must not poll in the
   * background.
   */
  active?: boolean;
  /** Non-null when the panel is showing one commit instead of the worktree. */
  commit?: CommitSummary | null;
  /** Non-null when the panel is showing one turn's span. */
  turn?: { turnId: string; label: string } | null;
  onClose?: () => void;
  onShowFiles?: () => void;
  onShowHistory?: () => void;
  onShowCurrent?: () => void;
  /** Widen a commit to everything the turn that wrote it changed. */
  onShowTurn?: (turnId: string) => void;
}) {
  const [selected, setSelected] = useState<string | null>(null);
  const [commentTarget, setCommentTarget] = useState<CommentTarget | null>(null);
  const [busyComment, setBusyComment] = useState<string | null>(null);
  const [commentActionError, setCommentActionError] = useState<{
    id: string;
    message: string;
  } | null>(null);
  const [optimisticComments, setOptimisticComments] = useState<Set<string>>(
    () => new Set(),
  );
  const [savedComments, setSavedComments] = useState<Set<string>>(
    () => new Set(),
  );
  // The panel is resizable, so "wrap automatically" has to follow this box, not
  // the window; the toggle in the diff header must agree with what is painted.
  const { ref: panelRef, narrow, wrap } = useDiffWrap<HTMLDivElement>();
  const queryClient = useQueryClient();
  const me = useQuery({
    queryKey: ["me"],
    queryFn: getMe,
    staleTime: 60_000,
  });

  const bundle = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => getThreadBundle(threadId),
    staleTime: 2_000,
    // A full bundle runs a git diff on the server. Ten seconds is still live
    // enough for a panel the user is staring at; hidden tabs do not poll.
    refetchInterval: commit || turn || !active ? false : 10_000,
    refetchIntervalInBackground: false,
    enabled: !commit && !turn && active,
  });
  const commitChanges = useQuery({
    queryKey: ["commit-changes", threadId, commit?.sha],
    queryFn: () => getCommitChanges(threadId, commit!.sha),
    enabled: Boolean(commit) && active,
    staleTime: Infinity,
  });
  const turnChanges = useQuery({
    queryKey: ["turn-changes", threadId, turn?.turnId],
    queryFn: () => getTurnChanges(threadId, turn!.turnId),
    enabled: Boolean(turn) && active,
    staleTime: 5_000,
    // The newest turn's span ends at the worktree, so it moves while the agent
    // works; older turns are fixed and this poll is one cheap git call.
    refetchInterval: turn && active ? 5_000 : false,
    refetchIntervalInBackground: false,
  });
  const changes = commit
    ? commitChanges.data
    : turn
      ? turnChanges.data
      : bundle.data?.bundle.changes;
  const comments = commit || turn ? [] : (bundle.data?.bundle.comments ?? []);
  const baseline = changes?.baseline;
  // Diff comments are reconciled against the base-branch diff on the server,
  // so a different baseline cannot host new line anchors.
  const canComment =
    !commit && !turn && baseline?.kind === "base_branch" && Boolean(selected);
  const loading = commit
    ? commitChanges.isPending
    : turn
      ? turnChanges.isPending
      : bundle.isPending;
  const loadError = commit
    ? commitChanges.error
    : turn
      ? turnChanges.error
      : bundle.error;
  const showLoadingSkeleton = useDelayedFlag(loading, 300, 500);
  const retryChanges = () => {
    if (commit) void commitChanges.refetch();
    else if (turn) void turnChanges.refetch();
    else void bundle.refetch();
  };

  const selectedIsCurrent =
    selected !== null &&
    (changes?.files.some((file) => file.path === selected) ?? false);
  const selectedFile =
    changes?.files.find((file) => file.path === selected) ?? null;
  // A changed file can be arbitrarily large (generated bundles, lockfiles,
  // logs). Asking the server for its patch ships megabytes and parsing it
  // renders every line as DOM; skip both and say why. The counts come from
  // the file list the panel already holds.
  const selectedLines = selectedFile
    ? selectedFile.additions + selectedFile.deletions
    : 0;
  const selectedTooLarge =
    selectedFile !== null &&
    !selectedFile.binary &&
    exceedsLineBudget(selectedFile.additions, selectedFile.deletions);
  const diff = useQuery({
    queryKey: commit
      ? ["commit-file-diff", threadId, commit.sha, selected]
      : turn
        ? ["turn-file-diff", threadId, turn.turnId, selected]
        : ["file-diff", threadId, selected, baseline?.kind, baseline?.commit],
    queryFn: () =>
      commit
        ? getCommitFileDiff(threadId, commit.sha, selected as string)
        : turn
          ? getTurnFileDiff(threadId, turn.turnId, selected as string)
          : getFileDiff(threadId, selected as string, {
              kind: baseline?.kind,
              commit: baseline?.commit,
            }),
    enabled:
      active &&
      selectedIsCurrent &&
      !selectedTooLarge &&
      (commit || turn ? true : Boolean(baseline)),
    retry: false,
  });
  const showDiffSkeleton = useDelayedFlag(diff.isPending, 300, 500);

  const refresh = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: ["thread", threadId] });
  }, [queryClient, threadId]);

  const currentAuthor = useMemo(() => {
    const user = me.data?.user;
    if (!user) return null;
    const participant = bundle.data?.bundle.thread.participants.find(
      (entry) => entry.user.id === user.id,
    )?.user;
    return {
      id: user.id,
      username: user.username,
      displayName: user.displayName,
      avatarUrl: user.avatarUrl,
      color: participant?.color ?? "#878a98",
    };
  }, [bundle.data?.bundle.thread.participants, me.data?.user]);

  const saveComment = useCallback(
    async (target: CommentTarget, body: string) => {
      if (!currentAuthor) return false;
      const anchor: CommentAnchor = {
        kind: "diff",
        path: target.path,
        hunkId: target.hunkId,
        line: target.line,
        side: target.side,
        contextHash: target.contextHash,
      };
      const tempId = `pending-${crypto.randomUUID()}`;
      const now = Date.now();
      const optimistic: Comment = {
        id: tempId,
        threadId,
        parentId: null,
        author: currentAuthor,
        body,
        anchor,
        anchorChanged: false,
        status: "pending",
        createdAt: now,
        updatedAt: now,
        deliveredSeq: null,
      };
      insertOptimisticComment(queryClient, threadId, optimistic);
      setOptimisticComments((current) => new Set(current).add(tempId));
      try {
        const { comment } = await createComment(threadId, body, anchor, false);
        replaceOptimisticComment(queryClient, threadId, tempId, comment);
        setOptimisticComments((current) => {
          const next = new Set(current);
          next.delete(tempId);
          return next;
        });
        setSavedComments((current) => new Set(current).add(comment.id));
        window.setTimeout(() => {
          setSavedComments((current) => {
            const next = new Set(current);
            next.delete(comment.id);
            return next;
          });
        }, 1200);
        setCommentTarget(null);
        refresh();
        return true;
      } catch (cause) {
        removeOptimisticComment(queryClient, threadId, tempId);
        setOptimisticComments((current) => {
          const next = new Set(current);
          next.delete(tempId);
          return next;
        });
        notifications.show({
          message:
            cause instanceof Error ? cause.message : "Could not save comment",
          color: "red",
        });
        return false;
      }
    },
    [currentAuthor, queryClient, refresh, threadId],
  );

  const actOnComment = useCallback(
    async (comment: Comment, action: "resolve" | "send" | "delete") => {
      setBusyComment(comment.id);
      setCommentActionError(null);
      try {
        if (action === "resolve") {
          await updateComment(comment.id, { status: "resolved" });
        } else if (action === "send") {
          await sendComment(comment.id);
        } else {
          await deleteComment(comment.id);
        }
        refresh();
      } catch (cause) {
        setCommentActionError({
          id: comment.id,
          message:
            cause instanceof Error ? cause.message : "Could not update comment",
        });
      } finally {
        setBusyComment(null);
      }
    },
    [refresh],
  );

  const patch = useMemo(
    () =>
      diff.data && selectedFile && !selectedTooLarge
        ? patchFromDiff(diff.data, selectedFile.oldPath)
        : null,
    [diff.data, selectedFile, selectedTooLarge],
  );

  useEffect(() => {
    const first = changes?.files[0]?.path ?? null;
    if (selected !== null && !selectedIsCurrent) {
      setSelected(first);
    } else if (selected === null && first !== null) {
      // Let the thread paint and become interactive before pulling and
      // highlighting the first diff; opening a thread should not wait on the
      // changes panel.
      const timer = window.setTimeout(() => setSelected(first), 700);
      return () => window.clearTimeout(timer);
    }
  }, [changes, selected, selectedIsCurrent]);

  // A new file or view owns a different hunk space; a draft on the previous
  // file would attach to the wrong line if it survived the switch.
  useEffect(() => {
    setCommentTarget(null);
  }, [selected, commit?.sha]);

  const { annotations, attached } = useMemo(() => {
    const groups = new Map<string, DiffLineAnnotation<DiffCommentMeta>>();
    const attachedIds = new Set<string>();
    if (!canComment || !diff.data) {
      return { annotations: [], attached: attachedIds };
    }
    for (const comment of comments) {
      const anchor = comment.anchor;
      if (anchor.kind !== "diff" || anchor.path !== selected) {
        continue;
      }
      const hunk =
        diff.data.hunks.find((entry) => entry.id === anchor.hunkId) ??
        diff.data.hunks.find(
          (entry) => entry.contextHash === anchor.contextHash,
        );
      const side = hunk
        ? anchorSide(diff.data, anchor.hunkId, anchor.line, anchor.side)
        : null;
      if (!hunk || !side) continue;
      const key = `${hunk.id}:${side}:${anchor.line}`;
      const existing = groups.get(key);
      if (existing) {
        existing.metadata.comments.push(comment);
      } else {
        groups.set(key, {
          side: side === "old" ? "deletions" : "additions",
          lineNumber: anchor.line,
          metadata: { key, comments: [comment], draft: false },
        });
      }
      attachedIds.add(comment.id);
    }
    if (
      commentTarget &&
      commentTarget.path === selected &&
      !groups.has(commentTarget.key)
    ) {
      groups.set(commentTarget.key, {
        side: commentTarget.side === "old" ? "deletions" : "additions",
        lineNumber: commentTarget.line,
        metadata: { key: commentTarget.key, comments: [], draft: true },
      });
    } else if (commentTarget && commentTarget.path === selected) {
      const group = groups.get(commentTarget.key);
      if (group) group.metadata.draft = true;
    }
    return { annotations: [...groups.values()], attached: attachedIds };
  }, [canComment, commentTarget, comments, diff.data, selected]);

  const unattached = useMemo(() => {
    if (!canComment) return [];
    return comments.filter((comment) => {
      if (attached.has(comment.id)) return false;
      if (comment.anchor.kind === "diff" || comment.anchor.kind === "file") {
        return comment.anchor.path === selected;
      }
      return false;
    });
  }, [attached, canComment, comments, selected]);

  const openComment = useCallback(
    (side: AnnotationSide, lineNumber: number) => {
      if (!canComment || !diff.data || !selected) return;
      const hunk = diff.data.hunks.find((entry) =>
        entry.lines.some((line) =>
          side === "deletions"
            ? line.oldLine === lineNumber
            : line.newLine === lineNumber,
        ),
      );
      if (!hunk) return;
      const commentSide: CommentSide =
        side === "deletions" ? "old" : "new";
      setCommentTarget({
        key: `${hunk.id}:${commentSide}:${lineNumber}`,
        path: selected,
        hunkId: hunk.id,
        line: lineNumber,
        side: commentSide,
        contextHash: hunk.contextHash,
      });
    },
    [canComment, diff.data, selected],
  );

  const renderAnnotation = useCallback(
    (annotation: DiffLineAnnotation<DiffCommentMeta>) => {
      const target = commentTarget;
      const save =
        annotation.metadata.draft && target
          ? (body: string) => saveComment(target, body)
          : null;
      return (
        <div className="diff-comment-thread">
          {annotation.metadata.comments.map((comment) => (
            <CommentCard
              key={comment.id}
              comment={comment}
              busy={busyComment === comment.id}
              pending={optimisticComments.has(comment.id)}
              saved={savedComments.has(comment.id)}
              error={
                commentActionError?.id === comment.id
                  ? commentActionError.message
                  : null
              }
              onResolve={() => void actOnComment(comment, "resolve")}
              onSend={() => void actOnComment(comment, "send")}
              onDelete={() => void actOnComment(comment, "delete")}
            />
          ))}
          {save ? (
            <CommentDraft
              onSave={save}
              onCancel={() => setCommentTarget(null)}
            />
          ) : null}
        </div>
      );
    },
    [actOnComment, busyComment, commentTarget, saveComment],
  );

  // The file list and its inline diff are one virtualized list: a worktree
  // with thousands of untracked files used to mount one row per file before
  // the first scroll.
  const listRows = useMemo(() => {
    const rows: Array<
      | { kind: "file"; file: FileChange }
      | { kind: "diff"; path: string }
    > = [];
    for (const file of changes?.files ?? []) {
      rows.push({ kind: "file", file });
      if (selected === file.path) rows.push({ kind: "diff", path: file.path });
    }
    return rows;
  }, [changes?.files, selected]);

  return (
    <div className="changes" ref={panelRef}>
      <div className="changes-head changes-head-stacked">
        <div className="changes-head-main">
          {commit ? (
            <Group gap={6} wrap="nowrap">
              <Text size="sm" fw={500}>
                Commit
              </Text>
              <Text size="xs" c="dimmed" className="changes-commit-sha">
                {commit.shortSha}
              </Text>
            </Group>
          ) : (
            <Text size="sm" fw={500}>
              Changes
            </Text>
          )}
          <Text size="sm" className="stat-add">
            +{changes?.stats.additions ?? 0}
          </Text>
          <Text size="sm" className="stat-del">
            −{changes?.stats.deletions ?? 0}
          </Text>
          <Box className="changes-head-gap" />
          {commit?.turnId && onShowTurn ? (
            <Button
              variant="subtle"
              size="compact-xs"
              color="gray"
              leftSection={<IconHistory size={12} stroke={1.5} />}
              onClick={() => onShowTurn(commit.turnId!)}
            >
              This turn
            </Button>
          ) : null}
          {commit || turn ? (
            onShowCurrent ? (
              <Button
                variant="subtle"
                size="compact-xs"
                color="gray"
                leftSection={<IconArrowLeft size={12} stroke={1.5} />}
                onClick={onShowCurrent}
              >
                Current
              </Button>
            ) : null
          ) : null}
          {!commit && onShowHistory ? (
            <ActionIcon
              aria-label="Open history"
              size={22}
              onClick={onShowHistory}
            >
              <IconHistory size={13} stroke={1.5} />
            </ActionIcon>
          ) : null}
          {!commit && !turn && onShowFiles ? (
            <Button
              variant="subtle"
              size="compact-xs"
              color="gray"
              leftSection={<IconFiles size={12} stroke={1.5} />}
              onClick={onShowFiles}
            >
              Files
            </Button>
          ) : null}
          {onClose ? (
            <ActionIcon
              aria-label={
                commit ? "Close commit" : turn ? "Close turn" : "Close changes"
              }
              data-autofocus
              size={22}
              onClick={onClose}
            >
              <IconX size={14} stroke={1.5} />
            </ActionIcon>
          ) : null}
        </div>
        <div className="changes-head-meta">
          {commit ? (
            <Text
              size="xs"
              c="dimmed"
              className="changes-commit-title"
              title={commit.turnPrompt ?? commit.subject}
            >
              {commit.turnPrompt ?? commit.subject}
            </Text>
          ) : turn ? (
            <Text size="xs" c="dimmed" className="changes-commit-title">
              Turn · {turn.label}
            </Text>
          ) : (
            <>
              <Group gap={4} wrap="nowrap" className="changes-baseline">
                <IconGitBranch size={11} stroke={1.5} />
                <Text size="xs" c="dimmed">
                  Since {changes?.baseLabel ?? "base"}
                </Text>
              </Group>
              <span className="changes-meta-sep">·</span>
              <Text size="xs" c="dimmed">
                {changes?.stats.total ?? changes?.files.length ?? 0}{" "}
                {(changes?.stats.total ?? changes?.files.length ?? 0) === 1
                  ? "file"
                  : "files"}
              </Text>
            </>
          )}
        </div>
      </div>
      {loading ? (
        showLoadingSkeleton ? (
          <ListSkeleton
            label="Loading changes"
            rows={8}
            lines={1}
            meta
            className="changes-scroll changes-files changes-skeleton-list"
            rowClassName="skeleton-change-row"
          />
        ) : null
      ) : loadError ? (
        <div className="changes-scroll">
          <LoadError
            title="Cannot load changes"
            message={
              loadError instanceof Error ? loadError.message : undefined
            }
            onRetry={retryChanges}
          />
        </div>
      ) : changes && changes.files.length > 0 ? (
        <div className="changes-scroll changes-files">
          <Virtuoso
            style={{ height: "100%" }}
            data={listRows}
            increaseViewportBy={400}
            computeItemKey={(_, row) =>
              row.kind === "file" ? row.file.path : `diff:${row.path}`
            }
            components={{
              Footer: () =>
                changes?.truncated ? (
                  <Text size="xs" c="dimmed" ta="center" py={8} px="sm">
                    Showing the first {MAX_CHANGE_FILES.toLocaleString()} changed
                    files; use Files to inspect the rest.
                  </Text>
                ) : null,
            }}
            itemContent={(_, row) =>
              row.kind === "file" ? (
                <FileRow
                  file={row.file}
                  active={selected === row.file.path}
                  onClick={() =>
                    setSelected((current) =>
                      current === row.file.path ? null : row.file.path,
                    )
                  }
                />
              ) : (
                <div className="change-diff inline">
                  <div className="change-diff-tools">
                    {canComment ? (
                      <Text size="xs" c="dimmed" className="diff-comment-hint">
                        Click a line number to comment
                      </Text>
                    ) : null}
                    <button
                      type="button"
                      className="diff-wrap-toggle"
                      aria-pressed={wrap}
                      onClick={() => toggleDiffWrap(narrow)}
                    >
                      {wrap ? (
                        <IconTextWrap size={13} stroke={1.5} />
                      ) : (
                        <IconTextWrapDisabled size={13} stroke={1.5} />
                      )}
                      {wrap ? "Wrap" : "Scroll"}
                    </button>
                  </div>
                  {unattached.length > 0 ? (
                    <div className="diff-file-comments">
                      {unattached.map((comment) => (
                        <CommentCard
                          key={comment.id}
                          comment={comment}
                          busy={busyComment === comment.id}
                          pending={optimisticComments.has(comment.id)}
                          saved={savedComments.has(comment.id)}
                          error={
                            commentActionError?.id === comment.id
                              ? commentActionError.message
                              : null
                          }
                          onResolve={() => void actOnComment(comment, "resolve")}
                          onSend={() => void actOnComment(comment, "send")}
                          onDelete={() => void actOnComment(comment, "delete")}
                        />
                      ))}
                    </div>
                  ) : null}
                  {selectedTooLarge ? (
                    <div className="diff-too-large">
                      <Text size="sm" c="dimmed">
                        {selectedLines.toLocaleString()} lines changed — too
                        large to show here.
                      </Text>
                      <Text size="xs" c="dimmed">
                        Diffs over {MAX_DIFF_LINES.toLocaleString()} lines open
                        in your editor instead.
                      </Text>
                    </div>
                  ) : diff.isPending ? (
                    showDiffSkeleton ? (
                      <DiffSkeleton />
                    ) : (
                      <div className="diff-loading-space" aria-busy="true" />
                    )
                  ) : diff.isError ? (
                    <LoadError
                      title="Cannot load diff"
                      message={
                        diff.error instanceof Error ? diff.error.message : undefined
                      }
                      onRetry={() => void diff.refetch()}
                      compact
                    />
                  ) : diff.data && diff.data.hunks.length === 0 ? (
                    <Text size="sm" c="dimmed" ta="center" py="xl">
                      No textual changes to show (binary file or metadata-only
                      rename).
                    </Text>
                  ) : patch ? (
                    <DiffView
                      patch={patch}
                      annotations={annotations}
                      renderAnnotation={renderAnnotation}
                      onLineNumberClick={openComment}
                      commentable={canComment}
                    />
                  ) : null}
                </div>
              )
            }
          />
        </div>
      ) : (
        <Text size="sm" c="dimmed" ta="center" py="xl" className="changes-scroll">
          {commit
            ? "This commit has no changes"
            : turn
              ? "This turn did not change any files"
              : "No changes yet"}
        </Text>
      )}
    </div>
  );
});
