import { Loader, NavLink } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useQueries, useQueryClient } from "@tanstack/react-query";
import { IconHistory } from "@tabler/icons-react";
import { useState } from "react";
import {
  attachThread,
  listAgentSessions,
  type Project,
  type ThreadSummary,
} from "../../api";
import { ThreadShelf } from "./ThreadRow";

const AGENTS = [
  { id: "codex", label: "Codex" },
  { id: "claude", label: "Claude Code" },
] as const;
const VISIBLE = 5;

type SessionRow = {
  key: string;
  id: string;
  title: string;
  agent: string;
  agentLabel: string;
  updatedAt: number;
};

/**
 * Importable native sessions for one project. The queries share their keys
 * with the picker, so opening the dialog or importing here refreshes both.
 */
export function ProjectSessions({
  project,
  onOpen,
  onMore,
}: {
  project: Project;
  onOpen: (thread: ThreadSummary) => void;
  onMore: () => void;
}) {
  const [importing, setImporting] = useState<string | null>(null);
  const queryClient = useQueryClient();
  const sessions = useQueries({
    queries: AGENTS.map((agent) => ({
      queryKey: ["agent-sessions", project.id, agent.id],
      queryFn: () => listAgentSessions(project.id, agent.id),
      staleTime: 30_000,
      retry: false,
    })),
  });
  const rows: SessionRow[] = AGENTS.flatMap((agent, index) =>
    (sessions[index].data?.sessions ?? []).map((session) => ({
      key: `${agent.id}:${session.id}`,
      id: session.id,
      title: session.title,
      agent: agent.id,
      agentLabel: agent.label,
      updatedAt: session.updatedAt,
    })),
  ).sort((left, right) => right.updatedAt - left.updatedAt);
  if (rows.length === 0) return null;

  const pick = async (row: SessionRow) => {
    setImporting(row.key);
    try {
      const { thread } = await attachThread(project.id, {
        sessionId: row.id,
        agent: row.agent,
      });
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: ["agent-sessions", project.id],
        }),
        queryClient.invalidateQueries({ queryKey: ["threads-all"] }),
        queryClient.invalidateQueries({ queryKey: ["projects"] }),
      ]);
      onOpen(thread);
    } catch (cause) {
      void queryClient.invalidateQueries({
        queryKey: ["agent-sessions", project.id],
      });
      notifications.show({
        color: "red",
        title: "Could not import session",
        message: cause instanceof Error ? cause.message : "Import failed",
      });
    } finally {
      setImporting(null);
    }
  };

  return (
    <ThreadShelf
      shelfKey={`${project.id}:sessions`}
      label="Agent sessions"
      count={rows.length}
    >
      {rows.slice(0, VISIBLE).map((row) => (
        <NavLink
          key={row.key}
          component="button"
          disabled={importing !== null}
          onClick={() => void pick(row)}
          leftSection={<IconHistory size={13} stroke={1.5} />}
          rightSection={importing === row.key ? <Loader size="xs" /> : null}
          label={row.title}
          description={`${row.agentLabel} · ${new Date(row.updatedAt).toLocaleDateString()}`}
        />
      ))}
      {rows.length > VISIBLE ? (
        <NavLink
          component="button"
          disabled={importing !== null}
          onClick={onMore}
          label="More sessions…"
        />
      ) : null}
    </ThreadShelf>
  );
}
