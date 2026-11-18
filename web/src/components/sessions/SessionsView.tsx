import {
  ActionIcon,
  Alert,
  Box,
  Button,
  Group,
  ScrollArea,
  SegmentedControl,
  Stack,
  Text,
  TextInput,
  Tooltip,
} from "@mantine/core";
import { useDebouncedValue, useMediaQuery } from "@mantine/hooks";
import { useQuery } from "@tanstack/react-query";
import {
  IconAlertTriangle,
  IconLayoutList,
  IconRefresh,
  IconSearch,
} from "@tabler/icons-react";
import { useState } from "react";
import { listSessions, type SessionSummary, type TmuxSessionSummary } from "../../api";
import { sessionKey, sessionMatches } from "../../lib/sessions";
import { EmptyState } from "../EmptyState";
import { ListSkeleton } from "../loading/Skeleton";
import { useDelayedFlag } from "../../lib/use-delayed-flag";
import { useSpinningIcon } from "../../lib/use-spinning-icon";
import { SessionRow } from "./SessionRow";
import { ThreadFromTmuxDialog } from "./ThreadFromTmuxDialog";

type Filter = "all" | "thread" | "tmux";



interface RowActions {
  session: SessionSummary;
  onOpen: () => void;
  onOpenShell?: () => void;
}

function sessionForRow(
  session: SessionSummary,
  onOpenThread: (threadId: string) => void,
  onOpenThreadShell: (threadId: string) => void,
  onOpenTmux: (socket: string, id: string) => void,
): RowActions {
  if (session.kind === "thread") {
    return {
      session,
      onOpen: () => onOpenThread(session.id),
      onOpenShell: () => onOpenThreadShell(session.id),
    };
  }
  return {
    session,
    onOpen: () => onOpenTmux(session.socket, session.id),
  };
}

export function SessionsView({
  hostEnabled,
  onOpenThread,
  onOpenThreadShell,
  onOpenTmux,
}: {
  hostEnabled: boolean;
  onOpenThread: (threadId: string) => void;
  onOpenThreadShell: (threadId: string) => void;
  onOpenTmux: (socket: string, id: string) => void;
}) {
  const phone = useMediaQuery("(max-width: 47.99em)");
  const [filter, setFilter] = useState<Filter>("all");
  const [needle, setNeedle] = useState("");
  const [manualRefresh, setManualRefresh] = useState(false);
  const [threadSession, setThreadSession] = useState<TmuxSessionSummary | null>(
    null,
  );
  const [debounced] = useDebouncedValue(needle.trim().toLowerCase(), 120);
  const sessions = useQuery({
    queryKey: ["sessions"],
    queryFn: listSessions,
    staleTime: 5_000,
    refetchInterval: 10_000,
    refetchIntervalInBackground: false,
  });
  const refreshing = useSpinningIcon(manualRefresh);
  const showSkeleton = useDelayedFlag(sessions.isPending, 300, 500);
  const all = sessions.data?.sessions ?? [];
  const filtered = all.filter(
    (session) =>
      (filter === "all" || session.kind === filter) &&
      sessionMatches(session, debounced),
  );
  const counts = {
    all: all.length,
    thread: all.filter((session) => session.kind === "thread").length,
    tmux: all.filter((session) => session.kind === "tmux").length,
  };
  const warnings = sessions.data?.warnings ?? [];
  const clear = () => {
    setFilter("all");
    setNeedle("");
  };

  return (
    <div className="sessions-view">
      <div className="sessions-head">
        <Group justify="space-between" wrap="nowrap" align="flex-start" gap="sm">
          <Box>
            <Text size="md" fw={500}>
              Sessions
            </Text>
            <Text size="sm" c="dimmed">
              {hostEnabled
                ? "Threads and tmux sessions on this machine. Tmux sessions are shared; one browser drives at a time."
                : "Threads and terminal shells on this machine."}
            </Text>
          </Box>
          <Tooltip label="Refresh sessions">
            <ActionIcon
              aria-label="Refresh sessions"
              aria-busy={manualRefresh}
              disabled={manualRefresh}
              onClick={() => {
                setManualRefresh(true);
                void sessions.refetch().finally(() => setManualRefresh(false));
              }}
            >
              <IconRefresh
                size={14}
                stroke={1.5}
                className={refreshing ? "icon-turn" : undefined}
              />
            </ActionIcon>
          </Tooltip>
        </Group>
        <Group className="sessions-tools" gap={8} wrap="nowrap">
          <TextInput
            aria-label="Search sessions"
            placeholder="Search sessions"
            value={needle}
            onChange={(event) => setNeedle(event.currentTarget.value)}
            leftSection={<IconSearch size={13} stroke={1.5} />}
            size="xs"
            className="sessions-search"
          />
          <Group className="sessions-filters" gap={2} wrap="nowrap">
            <SegmentedControl
              size="xs"
              value={filter}
              onChange={(value) => setFilter(value as Filter)}
              data={[
                { value: "all", label: `All ${counts.all}` },
                { value: "thread", label: `Threads ${counts.thread}` },
                ...(hostEnabled
                  ? [{ value: "tmux", label: `Tmux ${counts.tmux}` }]
                  : []),
              ]}
            />
          </Group>
        </Group>
      </div>

      {warnings.length ? (
        <Alert
          className="sessions-warning"
          color="yellow"
          variant="light"
          icon={<IconAlertTriangle size={15} stroke={1.5} />}
          title="Some tmux servers could not be read"
        >
          <Stack gap={2}>
            {warnings.map((warning) => (
              <Text key={warning} size="xs">
                {warning}
              </Text>
            ))}
          </Stack>
        </Alert>
      ) : null}

      {sessions.isPending ? (
        showSkeleton ? (
          <ListSkeleton
            label="Loading sessions"
            rows={6}
            avatar
            avatarSize={14}
            lines={2}
            className="sessions-scroll sessions-list"
          />
        ) : null
      ) : sessions.isError ? (
        <EmptyState
          icon={<IconLayoutList size={16} stroke={1.5} />}
          title="Could not load sessions"
          action={
            <Button variant="default" size="xs" onClick={() => void sessions.refetch()}>
              Retry
            </Button>
          }
        >
          {sessions.error instanceof Error
            ? sessions.error.message
            : "The server did not return a session list."}
        </EmptyState>
      ) : all.length === 0 ? (
        <EmptyState
          icon={<IconLayoutList size={16} stroke={1.5} />}
          title="No sessions yet"
          hint={
            hostEnabled
              ? "Start a thread from a project, or run tmux new -s work on this machine."
              : "Start a thread from a project."
          }
        >
          Sessions you can open will appear here.
        </EmptyState>
      ) : filtered.length === 0 ? (
        <EmptyState
          icon={<IconSearch size={16} stroke={1.5} />}
          title="No matching sessions"
          hint={phone ? undefined : "Try a thread title, project, socket or path."}
          action={
            <Button variant="default" size="xs" onClick={clear}>
              Clear filters
            </Button>
          }
        >
          Nothing matches the current search and filter.
        </EmptyState>
      ) : (
        <ScrollArea className="sessions-scroll" type="hover">
          <div className="sessions-list">
            {filtered.map((session) => {
              const row = sessionForRow(
                session,
                onOpenThread,
                onOpenThreadShell,
                onOpenTmux,
              );
              return (
                <SessionRow
                  key={sessionKey(session)}
                  session={row.session}
                  onOpen={row.onOpen}
                  onOpenShell={row.onOpenShell}
                  onOpenLinkedThread={onOpenThread}
                  onStartThread={setThreadSession}
                />
              );
            })}
          </div>
        </ScrollArea>
      )}
      <ThreadFromTmuxDialog
        opened={threadSession !== null}
        session={threadSession}
        onClose={() => setThreadSession(null)}
        onCreated={(thread) => {
          setThreadSession(null);
          onOpenThread(thread.id);
        }}
      />
    </div>
  );
}
