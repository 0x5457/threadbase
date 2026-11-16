import {
  ActionIcon,
  Box,
  Button,
  Group,
  Loader,
  Text,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
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
  getThreadBundle,
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
  useEffectiveDiffWrap,
} from "../lib/diff-wrap";
import {
  exceedsLineBudget,
  MAX_CHANGE_FILES,
  MAX_DIFF_LINES,
} from "../lib/diff-size";
import { CommentCard, CommentDraft } from "./thread/CommentCard";
import { DiffView, type DiffCommentMeta } from "./DiffView";

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
  onClose,
  onShowFiles,
  onShowHistory,
  onShowCurrent,
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
  onClose?: () => void;
  onShowFiles?: () => void;
  onShowHistory?: () => void;
  onShowCurrent?: () => void;
}) {
  const [selected, setSelected] = useState<string | null>(null);
  const [commentTarget, setCommentTarget] = useState<CommentTarget | null>(null);
  const [busyComment, setBusyComment] = useState<string | null>(null);
  const narrow = useMediaQuery("(max-width: 74.99em)");
  const wrap = useEffectiveDiffWrap(narrow);
  const queryClient = useQueryClient();

  const bundle = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => getThreadBundle(threadId),
    staleTime: 2_000,
    // A full bundle runs a git diff on the server. Ten seconds is still live
    // enough for a panel the user is staring at; hidden tabs do not poll.
    refetchInterval: commit || !active ? false : 10_000,
    refetchIntervalInBackground: false,
    enabled: !commit && active,
  });
  const commitChanges = useQuery({
    queryKey: ["commit-changes", threadId, commit?.sha],
    queryFn: () => getCommitChanges(threadId, commit!.sha),
    enabled: Boolean(commit) && active,
    staleTime: Infinity,
  });
  const changes = commit ? commitChanges.data : bundle.data?.bundle.changes;
  const comments = commit ? [] : (bundle.data?.bundle.comments ?? []);
  const baseline = changes?.baseline;
  // Diff comments are reconciled against the base-branch diff on the server,
  // so a different baseline cannot host new line anchors.
  const canComment =
    !commit && baseline?.kind === "base_branch" && Boolean(selected);
  const loading = commit ? commitChanges.isPending : bundle.isPending;
  const loadError = commit ? commitChanges.error : bundle.error;

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
      : ["file-diff", threadId, selected, baseline?.kind, baseline?.commit],
    queryFn: () =>
      commit
        ? getCommitFileDiff(threadId, commit.sha, selected as string)
        : getFileDiff(threadId, selected as string, {
            kind: baseline?.kind,
            commit: baseline?.commit,
          }),
    enabled:
      active &&
      selectedIsCurrent &&
      !selectedTooLarge &&
      (commit ? true : Boolean(baseline)),
    retry: false,
  });

  const refresh = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: ["thread", threadId] });
  }, [queryClient, threadId]);

  const saveComment = useCallback(
    async (target: CommentTarget, body: string) => {
      const anchor: CommentAnchor = {
        kind: "diff",
        path: target.path,
        hunkId: target.hunkId,
        line: target.line,
        side: target.side,
        contextHash: target.contextHash,
      };
      try {
        await createComment(threadId, body, anchor, false);
        notifications.show({
          message: "Comment saved for the next turn",
          color: "teal",
        });
        setCommentTarget(null);
        refresh();
        return true;
      } catch (cause) {
        notifications.show({
          message:
            cause instanceof Error ? cause.message : "Could not save comment",
          color: "red",
        });
        return false;
      }
    },
    [refresh, threadId],
  );

  const actOnComment = useCallback(
    async (comment: Comment, action: "resolve" | "send" | "delete") => {
      setBusyComment(comment.id);
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
        notifications.show({
          message:
            cause instanceof Error ? cause.message : "Could not update comment",
          color: "red",
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
    <div className="changes">
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
          {commit && onShowCurrent ? (
            <Button
              variant="subtle"
              size="compact-xs"
              color="gray"
              leftSection={<IconArrowLeft size={12} stroke={1.5} />}
              onClick={onShowCurrent}
            >
              Current
            </Button>
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
          {!commit && onShowFiles ? (
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
              aria-label={commit ? "Close commit" : "Close changes"}
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
        <Group className="changes-scroll" justify="center" py="xl">
          <Loader size="sm" />
        </Group>
      ) : loadError ? (
        <Text size="sm" c="red" p="sm" className="changes-scroll">
          {loadError instanceof Error ? loadError.message : "Cannot load changes"}
        </Text>
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
                    <Group justify="center" py="lg">
                      <Loader size="xs" />
                    </Group>
                  ) : diff.isError ? (
                    <Text size="sm" c="red" p="sm">
                      {diff.error instanceof Error
                        ? diff.error.message
                        : "Cannot load diff"}
                    </Text>
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
          {commit ? "This commit has no changes" : "No changes yet"}
        </Text>
      )}
    </div>
  );
});
