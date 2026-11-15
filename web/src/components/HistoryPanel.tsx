import { ActionIcon, Box, Button, Group, Loader, ScrollArea, Text } from "@mantine/core";
import { useQuery } from "@tanstack/react-query";
import { IconArrowsDiff, IconGitBranch, IconGitCommit, IconX } from "@tabler/icons-react";
import { getThreadHistory, type CommitSummary } from "../api";

const stamp = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
});

function titleOf(entry: CommitSummary) {
  if (entry.turnPrompt) return entry.turnPrompt;
  if (entry.checkpoint) return "Checkpoint";
  return entry.subject;
}

function HistoryRow({
  entry,
  current,
  selected,
  onSelect,
}: {
  entry: CommitSummary;
  current: boolean;
  selected: boolean;
  onSelect: (entry: CommitSummary) => void;
}) {
  return (
    <button
      type="button"
      className="history-row"
      data-active={selected ? "true" : undefined}
      onClick={() => onSelect(entry)}
      title={`${entry.subject} · ${entry.sha}`}
    >
      <IconGitCommit
        size={13}
        stroke={1.5}
        className={entry.checkpoint ? "history-icon checkpoint" : "history-icon"}
      />
      <span className="history-main">
        <span className="history-title">{titleOf(entry)}</span>
        <span className="history-meta">
          {entry.shortSha} · {stamp.format(new Date(entry.at))}
          {current ? " · latest" : ""}
        </span>
      </span>
      <span className="history-stats">
        <span className="stat-add">+{entry.additions}</span>
        <span className="stat-del">−{entry.deletions}</span>
        <span className="history-files">
          {entry.files} {entry.files === 1 ? "file" : "files"}
        </span>
      </span>
    </button>
  );
}

/**
 * The thread's checkpoint log. Rows are commits, not transcript turns: each
 * one shows the work it captured and opens that commit's own diff in the
 * Changes panel. The newest completed work may still be uncommitted and is
 * represented by the working-tree view behind "Changes".
 */
export function HistoryPanel({
  threadId,
  selectedSha,
  onSelect,
  onShowChanges,
  onClose,
}: {
  threadId: string;
  selectedSha: string | null;
  onSelect: (entry: CommitSummary) => void;
  onShowChanges: () => void;
  onClose?: () => void;
}) {
  const history = useQuery({
    queryKey: ["thread-history", threadId],
    queryFn: () => getThreadHistory(threadId),
    staleTime: 3_000,
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
  });
  const data = history.data?.history;
  const entries = data?.entries ?? [];

  return (
    <div className="changes history">
      <div className="changes-head">
        <Text size="sm" fw={500}>
          History
        </Text>
        <Text size="sm" c="dimmed" className="history-count">
          {entries.length}
        </Text>
        <Box className="changes-head-gap" />
        <Button
          variant="subtle"
          size="compact-xs"
          color="gray"
          leftSection={<IconArrowsDiff size={12} stroke={1.5} />}
          onClick={onShowChanges}
        >
          Changes
        </Button>
        {onClose ? (
          <ActionIcon aria-label="Close history" size={22} onClick={onClose}>
            <IconX size={14} stroke={1.5} />
          </ActionIcon>
        ) : null}
      </div>
      <ScrollArea className="changes-scroll" type="hover">
        {history.isPending ? (
          <Group justify="center" py="xl">
            <Loader size="sm" />
          </Group>
        ) : history.isError ? (
          <Text size="sm" c="red" p="sm">
            {history.error instanceof Error
              ? history.error.message
              : "Cannot load history"}
          </Text>
        ) : entries.length === 0 ? (
          <div className="history-empty">
            <Text size="sm" c="dimmed">
              No checkpoints yet.
            </Text>
            <Text size="xs" c="dimmed">
              Threadbase saves one before each agent turn, so this list fills in
              as the thread works.
            </Text>
          </div>
        ) : (
          <div className="history-list">
            {entries.map((entry) => (
              <HistoryRow
                key={entry.sha}
                entry={entry}
                current={entry.sha === data?.head}
                selected={entry.sha === selectedSha}
                onSelect={onSelect}
              />
            ))}
          </div>
        )}
        {data ? (
          <div className="history-base">
            <IconGitBranch size={11} stroke={1.5} />
            <span>
              Base · {data.baseBranch}
              {data.baseSha ? ` · ${data.baseSha.slice(0, 7)}` : ""}
            </span>
          </div>
        ) : null}
      </ScrollArea>
    </div>
  );
}
