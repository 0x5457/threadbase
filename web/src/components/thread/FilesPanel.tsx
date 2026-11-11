import { ChevronLeft, FileCode2, Folder, FolderOpen } from "lucide-react";
import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  ActionIcon,
  Box,
  Button,
  Flex,
  Skeleton,
  Stack,
  Text,
  Textarea,
  UnstyledButton,
} from "@mantine/core";
import { createComment } from "@/api/endpoints";
import { queryKeys, useFile, useFiles } from "@/api/queries";
import { EmptyState, ErrorState } from "@/components/app";
import { usePresenceStore } from "@/lib/presence";

export function FilesPanel({ threadId }: { threadId: string }) {
  const [path, setPath] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [line, setLine] = useState<number | null>(null);
  const [comment, setComment] = useState("");
  const queryClient = useQueryClient();
  const files = useFiles(threadId, path);
  const setLocal = usePresenceStore((state) => state.setLocal);
  const addComment = useMutation({
    mutationFn: () =>
      createComment(threadId, {
        body: comment.trim(),
        anchor: {
          kind: "file",
          path: selected ?? "",
          line: line ?? 1,
          contextHash: null,
        },
        parentId: null,
        send: false,
      }),
    onSuccess: () => {
      setComment("");
      setLine(null);
      void queryClient.invalidateQueries({
        queryKey: queryKeys.comments(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadEvents(threadId),
      });
    },
  });
  useEffect(() => {
    setLocal({
      panel: "files",
      target: selected ? { kind: "file", path: selected, line: 1 } : null,
    });
    return () => setLocal({ target: null, panel: "transcript" });
  }, [selected, setLocal]);
  const file = useFile(threadId, selected);
  const segments = path.split("/").filter(Boolean);

  if (selected) {
    return (
      <Stack gap={0} style={{ minHeight: 0 }}>
        <Flex
          align="center"
          gap="xs"
          h={32}
          px="xs"
          style={{
            borderBottom: "1px solid var(--mantine-color-default-border)",
          }}
        >
          <ActionIcon
            variant="subtle"
            color="gray"
            size="sm"
            aria-label="Back to file list"
            onClick={() => setSelected(null)}
          >
            <ChevronLeft size={14} aria-hidden />
          </ActionIcon>
          <Text size="sm" ff="monospace" c="dimmed" truncate="end">
            {selected}
          </Text>
        </Flex>
        {file.isLoading ? (
          <Box p="sm">
            <Skeleton height={160} />
          </Box>
        ) : file.isError ? (
          <ErrorState
            error={file.error}
            title="Cannot open file"
            onRetry={() => void file.refetch()}
            compact
          />
        ) : (
          <Box style={{ flex: 1, minHeight: 0, overflow: "auto" }}>
            <Stack gap={0} style={{ fontFamily: "var(--mantine-font-family-monospace)", fontSize: 13, lineHeight: 1.6 }}>
              {(file.data?.file.content ?? "").split("\n").map((text, index) => {
                const number = index + 1;
                return (
                  <UnstyledButton
                    type="button"
                    key={number}
                    onClick={() => setLine(number)}
                    style={{
                      display: "grid",
                      width: "100%",
                      gridTemplateColumns: "2.5rem 1fr",
                      gap: "0.5rem",
                      padding: "0 0.5rem",
                      textAlign: "left",
                      cursor: "pointer",
                      background:
                        line === number
                          ? "var(--mantine-color-default-hover)"
                          : "transparent",
                    }}
                  >
                    <span
                      style={{
                        textAlign: "right",
                        color: "var(--mantine-color-dimmed)",
                      }}
                    >
                      {number}
                    </span>
                    <span
                      style={{
                        whiteSpace: "pre-wrap",
                        overflowWrap: "break-word",
                      }}
                    >
                      {text || " "}
                    </span>
                  </UnstyledButton>
                );
              })}
            </Stack>
            {line ? (
              <Box
                p="xs"
                style={{
                  position: "sticky",
                  bottom: 0,
                  borderTop: "1px solid var(--mantine-color-default-border)",
                  background: "var(--mantine-color-default)",
                }}
              >
                <Text fz={11} c="blue.6" mb={4}>
                  Comment on {selected}:{line}
                </Text>
                <Textarea
                  value={comment}
                  onChange={(event) => setComment(event.target.value)}
                  placeholder="Leave a file comment…"
                  rows={2}
                />
                <Flex justify="flex-end" gap="xs" mt="xs">
                  <Button
                    variant="subtle"
                    color="dark"
                    size="xs"
                    onClick={() => setLine(null)}
                  >
                    Cancel
                  </Button>
                  <Button
                    size="xs"
                    disabled={!comment.trim()}
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
      </Stack>
    );
  }

  return (
    <Stack gap={0} style={{ minHeight: 0 }}>
      <Flex
        align="center"
        gap={4}
        px="xs"
        py="xs"
        style={{
          minHeight: 32,
          borderBottom: "1px solid var(--mantine-color-default-border)",
        }}
        wrap="wrap"
      >
        <Button
          variant="subtle"
          color="blue"
          size="compact-xs"
          px={0}
          onClick={() => setPath("")}
        >
          worktree
        </Button>
        {segments.map((segment, index) => {
          const next = segments.slice(0, index + 1).join("/");
          return (
            <Flex key={next} align="center" gap={4}>
              <Text size="xs" c="dimmed">
                /
              </Text>
              <Button
                variant="subtle"
                color="dark"
                size="compact-xs"
                px={0}
                onClick={() => setPath(next)}
              >
                {segment}
              </Button>
            </Flex>
          );
        })}
      </Flex>
      {files.isLoading ? (
        <Stack gap="xs" p="xs">
          {Array.from({ length: 6 }).map((_, index) => (
            <Skeleton key={index} height={24} />
          ))}
        </Stack>
      ) : files.isError ? (
        <ErrorState
          error={files.error}
          title="Cannot list files"
          onRetry={() => void files.refetch()}
          compact
        />
      ) : (files.data?.entries.length ?? 0) === 0 ? (
        <EmptyState
          icon={FolderOpen}
          title="Empty directory"
          compact
        />
      ) : (
        <Box
          component="ul"
          style={{
            flex: 1,
            minHeight: 0,
            overflowY: "auto",
            listStyle: "none",
            padding: 4,
            margin: 0,
          }}
        >
          {files.data?.entries.map((entry) => (
            <li key={entry.path}>
              <Button
                variant="subtle"
                color="dark"
                fullWidth
                justify="flex-start"
                leftSection={
                  entry.isDir ? (
                    <Folder
                      size={14}
                      aria-hidden
                      style={{ color: "var(--mantine-color-blue-6)" }}
                    />
                  ) : (
                    <FileCode2
                      size={14}
                      aria-hidden
                      style={{ color: "var(--mantine-color-dimmed)" }}
                    />
                  )
                }
                rightSection={
                  !entry.isDir ? (
                    <Text fz={11} c="dimmed">
                      {entry.size} B
                    </Text>
                  ) : undefined
                }
                onClick={() => {
                  if (entry.isDir) {
                    setPath(entry.path);
                  } else {
                    setSelected(entry.path);
                  }
                }}
              >
                <Text
                  size="sm"
                  ff="monospace"
                  truncate="end"
                  component="span"
                >
                  {entry.name}
                </Text>
              </Button>
            </li>
          ))}
        </Box>
      )}
    </Stack>
  );
}
