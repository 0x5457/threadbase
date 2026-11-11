import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check, GitCompareArrows, RotateCcw, Undo2 } from "lucide-react";
import { useEffect, useState } from "react";
import { notifications } from "@mantine/notifications";
import {
  Badge,
  Box,
  Button,
  Checkbox,
  Divider,
  Flex,
  Select,
  Skeleton,
  Stack,
  Text,
  TextInput,
  Textarea,
  UnstyledButton,
} from "@mantine/core";
import {
  createComment,
  mergeBackChanges,
  revertThread,
  setSeen,
} from "@/api/endpoints";
import { queryKeys, useChanges, useComments, useFileDiff } from "@/api/queries";
import type { BaselineKind } from "@/api/types/BaselineKind";
import type { CommentDto } from "@/api/types/CommentDto";
import type { DiffLineDto } from "@/api/types/DiffLineDto";
import type { FileChangeDto } from "@/api/types/FileChangeDto";
import type { FileStatus } from "@/api/types/FileStatus";
import { EmptyState, ErrorState } from "@/components/app";
import { usePresenceStore } from "@/lib/presence";

const BASELINES: { value: BaselineKind; label: string }[] = [
  { value: "base_branch", label: "Base branch" },
  { value: "last_commit", label: "Last commit" },
  { value: "last_turn", label: "Last turn" },
  { value: "commit", label: "Specific commit" },
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

function statusColor(status: FileStatus): string {
  switch (status) {
    case "added":
    case "untracked":
      return "green";
    case "deleted":
      return "red";
    case "renamed":
      return "blue";
    default:
      return "gray";
  }
}

function lineTone(line: DiffLineDto): { bg: string; c: string } {
  if (line.kind === "add") {
    return {
      bg: "var(--mantine-color-green-light)",
      c: "var(--mantine-color-green-light-color)",
    };
  }
  if (line.kind === "delete") {
    return {
      bg: "var(--mantine-color-red-light)",
      c: "var(--mantine-color-red-light-color)",
    };
  }
  return { bg: "transparent", c: "var(--mantine-color-dimmed)" };
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
  const [commit, setCommit] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [commentTarget, setCommentTarget] = useState<{
    hunkId: string;
    line: number;
    contextHash: string;
  } | null>(null);
  const [commentBody, setCommentBody] = useState("");
  const [showComments, setShowComments] = useState(true);
  const changes = useChanges(threadId, baseline, commit);
  const diff = useFileDiff(threadId, selected, baseline, commit);
  const comments = useComments(threadId);
  const setLocal = usePresenceStore((state) => state.setLocal);
  useEffect(() => {
    setLocal({
      panel: "changes",
      target: selected ? { kind: "file", path: selected, line: 1 } : null,
    });
    return () => setLocal({ target: null, panel: "transcript" });
  }, [selected, setLocal]);
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
      setSeen(threadId, [file.path], value, {
        kind: baseline,
        commit: baseline === "commit" ? commit || null : null,
      }),
    onSuccess: invalidate,
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
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
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
  });
  const markAllSeen = useMutation({
    mutationFn: () =>
      setSeen(
        threadId,
        (changes.data?.files ?? []).map((file) => file.path),
        true,
        {
          kind: baseline,
          commit: baseline === "commit" ? commit || null : null,
        },
      ),
    onSuccess: invalidate,
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
  });
  const revert = useMutation({
    mutationFn: () => revertThread(threadId),
    onSuccess: () => {
      notifications.show({
        message: "Reverted worktree to the base commit",
        color: "green",
      });
      invalidate();
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadEvents(threadId),
      });
    },
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
  });
  const merge = useMutation({
    mutationFn: () => mergeBackChanges(threadId, false, null),
    onSuccess: (result) => {
      notifications.show({
        message: `Applied ${result.files.length} files to the project`,
        color: "green",
      });
      invalidate();
      void queryClient.invalidateQueries({ queryKey: queryKeys.projects });
    },
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
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
      <Stack gap="xs" px="sm" py="sm">
        <Skeleton height={20} width={160} />
        <Skeleton height={40} />
        <Skeleton height={64} />
      </Stack>
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
    <Stack gap={0} style={{ minHeight: 0 }}>
      <Box p="sm">
        <Flex align="center" gap="xs">
          <Select
            value={baseline}
            onChange={(value) => {
              if (value) {
                setBaseline(value as BaselineKind);
              }
            }}
            data={BASELINES}
            size="xs"
            aria-label="Baseline"
            style={{ flex: 1 }}
          />
          <Text
            size="xs"
            ff="monospace"
            c="dimmed"
            style={{ whiteSpace: "nowrap" }}
          >
            {data.stats.seen}/{data.stats.total}
          </Text>
        </Flex>
        {baseline === "commit" ? (
          <TextInput
            value={commit}
            onChange={(event) => setCommit(event.target.value.trim())}
            aria-label="Commit SHA"
            placeholder="Commit SHA"
            size="xs"
            ff="monospace"
            mt="xs"
          />
        ) : null}
        <Flex justify="space-between" align="center" mt="xs" gap="xs">
          <Text size="xs" c="dimmed" truncate="end">
            {data.baseLabel}
          </Text>
          <Text size="xs" ff="monospace" style={{ whiteSpace: "nowrap" }}>
            <Text component="span" c="green.6">
              +{data.stats.additions}
            </Text>{" "}
            <Text component="span" c="red.6">
              −{data.stats.deletions}
            </Text>
          </Text>
        </Flex>
        {canWrite ? (
          <Flex gap="xs" mt="xs">
            <Button
              size="xs"
              variant="subtle"
              color="dark"
              flex={1}
              loading={revert.isPending}
              onClick={() => {
                if (
                  window.confirm(
                    "Revert the worktree to the base commit? Every agent change, including untracked files, will be discarded.",
                  )
                ) {
                  revert.mutate();
                }
              }}
            >
              <Undo2 size={12} aria-hidden />
              Revert
            </Button>
            <Button
              size="xs"
              variant="outline"
              flex={1}
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
              <RotateCcw size={12} aria-hidden />
              Apply
            </Button>
          </Flex>
        ) : null}
      </Box>
      <Divider />
      <Box component="ul" style={{ listStyle: "none", margin: 0, padding: 0 }}>
        {data.files.map((file) => (
          <li key={file.path}>
            <Flex
              align="center"
              gap="xs"
              px="sm"
              py="xs"
              bg={
                selected === file.path
                  ? "var(--mantine-color-default-hover)"
                  : undefined
              }
            >
              <Checkbox
                checked={file.seen}
                aria-label={`Mark ${file.path} as seen`}
                onChange={(event) =>
                  seen.mutate({ file, value: event.currentTarget.checked })
                }
              />
              <Button
                variant="subtle"
                color="dark"
                justify="flex-start"
                leftSection={
                  <Badge
                    variant="light"
                    color={statusColor(file.status)}
                    size="sm"
                    circle
                  >
                    {statusLabel(file.status)}
                  </Badge>
                }
                rightSection={
                  <Text
                    fz={11}
                    ff="monospace"
                    c="dimmed"
                    style={{ whiteSpace: "nowrap" }}
                  >
                    +{file.additions} −{file.deletions}
                  </Text>
                }
                onClick={() =>
                  setSelected(selected === file.path ? null : file.path)
                }
                style={{ flex: 1 }}
              >
                <Text size="sm" ff="monospace" truncate="end" component="span">
                  {file.path}
                </Text>
              </Button>
            </Flex>
            <Divider />
          </li>
        ))}
      </Box>

      {selected ? (
        <Box style={{ flex: 1, minHeight: 0, overflow: "auto" }}>
          {diff.isLoading ? (
            <Box p="sm">
              <Skeleton height={96} />
            </Box>
          ) : diff.isError ? (
            <ErrorState
              error={diff.error}
              title="Cannot load diff"
              onRetry={() => void diff.refetch()}
              compact
            />
          ) : diff.data?.binary ? (
            <Box p="sm">
              <Badge variant="outline" color="gray" size="sm">
                binary file
              </Badge>
            </Box>
          ) : (
            <Box
              style={{
                fontFamily: "var(--mantine-font-family-monospace)",
                fontSize: "var(--mantine-font-size-md)",
              }}
            >
              {diff.data?.hunks.map((hunk) => (
                <Box key={hunk.id}>
                  <Flex
                    align="center"
                    gap="xs"
                    px="xs"
                    py={4}
                    style={{
                      position: "sticky",
                      top: 0,
                      borderTop:
                        "1px solid var(--mantine-color-default-border)",
                      borderBottom:
                        "1px solid var(--mantine-color-default-border)",
                      background: "var(--mantine-color-default)",
                      fontSize: "var(--mantine-font-size-xs)",
                      color: "var(--mantine-color-dimmed)",
                    }}
                  >
                    <Text fz={11} ff="monospace" truncate="end">
                      {hunk.header}
                    </Text>
                    {fileComments.some(
                      (comment) => comment.anchor.hunkId === hunk.id,
                    ) ? (
                      <Button
                        variant="subtle"
                        color="blue"
                        size="compact-xs"
                        ml="auto"
                        onClick={() => setShowComments((value) => !value)}
                      >
                        {fileComments.filter(
                          (comment) => comment.anchor.hunkId === hunk.id,
                        ).length}{" "}
                        {showComments ? "hide" : "comments"}
                      </Button>
                    ) : null}
                  </Flex>
                  {hunk.lines.map((line, index) => {
                    const lineNumber = line.newLine ?? line.oldLine ?? 0;
                    const tone = lineTone(line);
                    return (
                      <UnstyledButton
                        type="button"
                        key={`${hunk.id}-${index}`}
                        onClick={() => {
                          setCommentTarget({
                            hunkId: hunk.id,
                            line: lineNumber,
                            contextHash: hunk.contextHash,
                          });
                          setLocal({
                            panel: "changes",
                            target: {
                              kind: "diff",
                              file: selected,
                              hunkId: hunk.id,
                              line: lineNumber,
                            },
                          });
                        }}
                        bg={tone.bg}
                        c={tone.c}
                        style={{
                          display: "grid",
                          width: "100%",
                          gridTemplateColumns: "2.5rem 2.5rem 1fr 1rem",
                          gap: "0.5rem",
                          padding: "0 0.5rem",
                          textAlign: "left",
                          fontFamily: "inherit",
                          fontSize: "inherit",
                          lineHeight: "inherit",
                        }}
                      >
                        <span style={{ textAlign: "right", opacity: 0.7 }}>
                          {line.oldLine ?? ""}
                        </span>
                        <span style={{ textAlign: "right", opacity: 0.7 }}>
                          {line.newLine ?? ""}
                        </span>
                        <span
                          style={{
                            whiteSpace: "pre-wrap",
                            wordBreak: "break-all",
                          }}
                        >
                          {line.content || " "}
                        </span>
                        <span
                          style={{
                            textAlign: "center",
                            color: "var(--mantine-color-blue-6)",
                          }}
                          aria-hidden
                        >
                          {fileComments.some(
                            (comment) =>
                              comment.anchor.hunkId === hunk.id &&
                              comment.anchor.line === lineNumber,
                          )
                            ? "●"
                            : ""}
                        </span>
                      </UnstyledButton>
                    );
                  })}
                  {showComments
                    ? fileComments
                        .filter((comment) => comment.anchor.hunkId === hunk.id)
                        .map((comment) => (
                          <Box
                            key={comment.id}
                            px="xs"
                            py="xs"
                            style={{
                              borderTop:
                                "1px solid var(--mantine-color-blue-light)",
                              background: "var(--mantine-color-blue-light)",
                            }}
                          >
                            <Text fz={11} c="blue.6">
                              line {comment.anchor.line} ·{" "}
                              {comment.author.displayName}
                              {comment.status === "pending"
                                ? " · pending"
                                : ""}
                            </Text>
                            <Text
                              size="sm"
                              style={{ whiteSpace: "pre-wrap" }}
                            >
                              {comment.body}
                            </Text>
                          </Box>
                        ))
                    : null}
                </Box>
              ))}
              {commentTarget ? (
                <Box
                  p="sm"
                  style={{
                    borderTop:
                      "1px solid var(--mantine-color-default-border)",
                    background: "var(--mantine-color-default)",
                  }}
                >
                  <Text size="sm" c="blue.6" mb={4}>
                    Comment on {selected}:{commentTarget.line}
                  </Text>
                  <Textarea
                    value={commentBody}
                    onChange={(event) => setCommentBody(event.target.value)}
                    placeholder="Leave a review comment…"
                    rows={2}
                  />
                  <Flex justify="flex-end" gap="xs" mt="xs">
                    <Button
                      size="xs"
                      variant="subtle"
                      color="dark"
                      onClick={() => setCommentTarget(null)}
                    >
                      Cancel
                    </Button>
                    <Button
                      size="xs"
                      disabled={!commentBody.trim()}
                      loading={addComment.isPending}
                      onClick={() => addComment.mutate()}
                    >
                      Add pending comment
                    </Button>
                  </Flex>
                </Box>
              ) : null}
            </Box>
          )}
        </Box>
      ) : (
        <Flex align="center" gap="xs" p="sm">
          <Check size={12} aria-hidden />
          <Text size="sm" c="dimmed">
            Select a file to review its diff.
          </Text>
        </Flex>
      )}
    </Stack>
  );
}
