import {
  ActionIcon,
  Box,
  Button,
  Group,
  Loader,
  NavLink,
  ScrollArea,
  Text,
} from "@mantine/core";
import { useQuery } from "@tanstack/react-query";
import { formatPatch, type StructuredPatch } from "diff";
import { useMediaQuery } from "@mantine/hooks";
import {
  IconFile,
  IconFiles,
  IconGitBranch,
  IconTextWrap,
  IconTextWrapDisabled,
  IconX,
} from "@tabler/icons-react";
import { useEffect, useMemo, useState } from "react";
import {
  getFileDiff,
  getThreadBundle,
  type FileChange,
  type FileDiff,
} from "../api";
import {
  toggleDiffWrap,
  useEffectiveDiffWrap,
} from "../lib/diff-wrap";
import { DiffView } from "./DiffView";

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

function FileRow({
  file,
  active,
  onClick,
}: {
  file: FileChange;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <NavLink
      component="button"
      className="side-nav change-row"
      active={active}
      onClick={onClick}
      leftSection={<IconFile size={13} stroke={1.5} />}
      label={
        <Group gap={6} wrap="nowrap" justify="space-between">
          <Text size="sm" className="change-path">
            {file.path}
          </Text>
          <Group gap={5} wrap="nowrap">
            <Text size="xs" className="stat-add">
              +{file.additions}
            </Text>
            <Text size="xs" className="stat-del">
              −{file.deletions}
            </Text>
            <Text
              size="xs"
              ff="monospace"
              className={`file-status ${file.status}`}
            >
              {statusLetter[file.status] ?? "?"}
            </Text>
          </Group>
        </Group>
      }
    />
  );
}

export function RealChangesPanel({
  threadId,
  onClose,
  onShowFiles,
}: {
  threadId: string;
  onClose?: () => void;
  onShowFiles?: () => void;
}) {
  const [selected, setSelected] = useState<string | null>(null);
  const narrow = useMediaQuery("(max-width: 74.99em)");
  const wrap = useEffectiveDiffWrap(narrow);
  const bundle = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => getThreadBundle(threadId),
    staleTime: 2_000,
    refetchInterval: 4_000,
    refetchIntervalInBackground: false,
  });
  const changes = bundle.data?.bundle.changes;
  const baseline = changes?.baseline;

  const selectedIsCurrent =
    selected !== null &&
    (changes?.files.some((file) => file.path === selected) ?? false);
  const diff = useQuery({
    queryKey: ["file-diff", threadId, selected, baseline?.kind, baseline?.commit],
    queryFn: () =>
      getFileDiff(threadId, selected as string, {
        kind: baseline?.kind,
        commit: baseline?.commit,
      }),
    enabled: selectedIsCurrent && Boolean(baseline),
    retry: false,
  });

  const selectedFile =
    changes?.files.find((file) => file.path === selected) ?? null;
  const patch = useMemo(
    () =>
      diff.data && selectedFile
        ? patchFromDiff(diff.data, selectedFile.oldPath)
        : null,
    [diff.data, selectedFile],
  );

  useEffect(() => {
    setSelected(null);
  }, [threadId]);

  useEffect(() => {
    const first = changes?.files[0]?.path ?? null;
    if (selected !== null && !selectedIsCurrent) {
      setSelected(first);
    } else if (selected === null && first !== null) {
      setSelected(first);
    }
  }, [changes, selected, selectedIsCurrent]);

  return (
    <div className="changes">
      <div className="changes-head">
        <Text size="sm" fw={500}>
          Changes
        </Text>
        <Group gap={4} wrap="nowrap" className="changes-baseline">
          <IconGitBranch size={11} stroke={1.5} />
          <Text size="sm" c="dimmed">
            Since {changes?.baseLabel ?? "base"}
          </Text>
        </Group>
        <Text size="sm" className="stat-add">
          +{changes?.stats.additions ?? 0}
        </Text>
        <Text size="sm" className="stat-del">
          −{changes?.stats.deletions ?? 0}
        </Text>
        <Box className="changes-head-gap" />
        <Text size="xs" c="dimmed">
          {changes?.files.length ?? 0}{" "}
          {(changes?.files.length ?? 0) === 1 ? "file" : "files"}
        </Text>
        {onShowFiles ? (
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
            aria-label="Close changes"
            data-autofocus
            size={22}
            onClick={onClose}
          >
            <IconX size={14} stroke={1.5} />
          </ActionIcon>
        ) : null}
      </div>
      <ScrollArea className="changes-scroll" type="hover">
        {bundle.isPending ? (
          <Group justify="center" py="xl">
            <Loader size="sm" />
          </Group>
        ) : changes && changes.files.length > 0 ? (
          <>
            <div className="changes-files">
              {changes.files.map((file) => (
                <FileRow
                  key={file.path}
                  file={file}
                  active={selected === file.path}
                  onClick={() =>
                    setSelected((current) =>
                      current === file.path ? null : file.path,
                    )
                  }
                />
              ))}
            </div>
            {selected ? (
              <div className="change-diff">
                <div className="change-diff-tools">
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
                {diff.isPending ? (
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
                  <DiffView patch={patch} />
                ) : null}
              </div>
            ) : (
              <Text size="sm" c="dimmed" ta="center" py="xl">
                Pick a file to review its diff
              </Text>
            )}
          </>
        ) : (
          <Text size="sm" c="dimmed" ta="center" py="xl">
            No changes yet
          </Text>
        )}
      </ScrollArea>
    </div>
  );
}
