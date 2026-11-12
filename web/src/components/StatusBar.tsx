import { AppShell, Divider, Group, Text } from "@mantine/core";
import { IconFolder, IconGitBranch, IconLayoutGrid } from "@tabler/icons-react";
import { useQuery } from "@tanstack/react-query";
import { getMe, getThreadBundle, type ThreadSummary } from "../api";

export function StatusBar({
  thread,
  workspace,
}: {
  thread?: ThreadSummary;
  workspace?: boolean;
}) {
  const bundle = useQuery({
    queryKey: ["thread", thread?.id],
    queryFn: () => getThreadBundle(thread!.id),
    enabled: Boolean(thread),
    staleTime: 2_000,
    retry: false,
  });
  const stats = bundle.data?.bundle.changes.stats;
  const me = useQuery({
    queryKey: ["me"],
    queryFn: getMe,
    staleTime: 60_000,
    retry: false,
  });
  const readOnly = me.data?.user.access === "read_only";

  if (workspace || !thread) {
    return (
      <AppShell.Footer className="statusbar">
        <Group gap={4} wrap="nowrap">
          <IconLayoutGrid size={11} stroke={1.5} />
          <Text size="xs" c="dimmed">
            Workspace
          </Text>
        </Group>
        <div className="spacer" />
        <Text size="xs" c="dimmed">
          Threadbase
        </Text>
      </AppShell.Footer>
    );
  }

  const running = thread.status === "running";
  const waiting = thread.status === "requires_action";
  const statusLabel = thread.status.replaceAll("_", " ");

  return (
    <AppShell.Footer className="statusbar">
      <Group gap={10} wrap="nowrap" className="status-left">
        <Group gap={4} wrap="nowrap">
          <IconGitBranch size={11} stroke={1.5} />
          <Text size="xs" c="dimmed">
            {thread.baseBranch}
          </Text>
        </Group>
        <Divider orientation="vertical" className="status-divider" />
        <Group gap={4} wrap="nowrap" visibleFrom="sm">
          <IconFolder size={11} stroke={1.5} />
          <Text size="xs" c="dimmed" className="status-worktree">
            {thread.branch}
          </Text>
        </Group>
      </Group>
      <div className="spacer" />
      <Group gap={10} wrap="nowrap">
        {readOnly ? (
          <Text size="xs" className="status-readonly" title="Your account can read but not write">
            read-only
          </Text>
        ) : null}
        <Text size="xs" c="dimmed" visibleFrom="md">
          {thread.agent}
        </Text>
        <Text size="xs" c="dimmed" visibleFrom="md">
          {thread.model ?? "default"}
        </Text>
        <Text size="xs" c="dimmed" visibleFrom="sm">
          {thread.effort ?? ""}
        </Text>
        <Group gap={4} wrap="nowrap">
          <Text size="xs" className="stat-add">
            +{stats?.additions ?? 0}
          </Text>
          <Text size="xs" className="stat-del">
            −{stats?.deletions ?? 0}
          </Text>
        </Group>
        <Text size="xs" c="dimmed">
          {stats?.files ?? 0} {stats?.files === 1 ? "file" : "files"}
        </Text>
        <Group gap={4} wrap="nowrap" visibleFrom="sm">
          {running || waiting ? (
            <span className={`dot ${waiting ? "waiting" : "running"}`} />
          ) : null}
          <Text size="xs" c="dimmed">
            {statusLabel}
          </Text>
        </Group>
      </Group>
    </AppShell.Footer>
  );
}
