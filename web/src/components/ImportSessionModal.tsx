import {
  Alert,
  Button,
  Group,
  Loader,
  Modal,
  ScrollArea,
  SegmentedControl,
  Stack,
  Text,
  TextInput,
  UnstyledButton,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import { IconAlertCircle } from "@tabler/icons-react";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  attachThread,
  listAgentSessions,
  type Project,
  type ThreadSummary,
} from "../api";

const AGENTS = [
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude Code" },
] as const;

/**
 * Continue a conversation that already exists in the agent's own session
 * store. Picking one creates the thread behind it, so the agent CLI and
 * threadbase keep driving the same conversation.
 */
export function ImportSessionModal({
  opened,
  project,
  onClose,
  onImported,
}: {
  opened: boolean;
  project: Project | null;
  onClose: () => void;
  onImported: (thread: ThreadSummary) => void;
}) {
  const phone = useMediaQuery("(max-width: 47.99em)");
  const [agent, setAgent] = useState<string>("codex");
  const [search, setSearch] = useState("");
  const [importing, setImporting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!opened) return;
    setSearch("");
    setImporting(null);
    setError(null);
  }, [opened]);

  const sessionsQuery = useQuery({
    queryKey: ["agent-sessions", project?.id, agent],
    queryFn: () => listAgentSessions(project!.id, agent),
    enabled: opened && project !== null,
    staleTime: 5_000,
  });
  const needle = search.trim().toLowerCase();
  const sessions = (sessionsQuery.data?.sessions ?? []).filter(
    (session) =>
      needle === "" ||
      session.title.toLowerCase().includes(needle) ||
      session.cwd.toLowerCase().includes(needle),
  );

  const pick = async (sessionId: string) => {
    if (!project) return;
    setImporting(sessionId);
    setError(null);
    try {
      const { thread } = await attachThread(project.id, { sessionId, agent });
      notifications.show({ message: "Session imported", color: "teal" });
      onImported(thread);
      onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Cannot import session");
    } finally {
      setImporting(null);
    }
  };

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title={`Continue a session in ${project?.name ?? "project"}`}
      size="md"
      centered
      fullScreen={phone}
      closeOnClickOutside={importing === null}
      closeOnEscape={importing === null}
      withCloseButton={importing === null}
    >
      <Stack gap="sm">
        <Text size="xs" c="dimmed">
          Conversations read from the agent's own session store in{" "}
          <Text span ff="monospace" size="xs">
            {project?.path ?? ""}
          </Text>
          . The thread continues the same conversation the CLI sees.
        </Text>
        <SegmentedControl
          fullWidth
          data={AGENTS.map((entry) => ({ ...entry }))}
          value={agent}
          onChange={setAgent}
        />
        <TextInput
          placeholder="Search sessions"
          value={search}
          onChange={(event) => setSearch(event.currentTarget.value)}
        />
        {sessionsQuery.isLoading ? (
          <Group justify="center" py="lg">
            <Loader size="sm" />
          </Group>
        ) : sessionsQuery.isError ? (
          <Alert icon={<IconAlertCircle size={14} />} color="red">
            Could not read existing sessions: {String(sessionsQuery.error)}
          </Alert>
        ) : sessions.length === 0 ? (
          <Text size="sm" c="dimmed" py="sm">
            No {agent} sessions found in this project directory. Start a new
            thread instead, or check that the project path matches where the
            agent ran.
          </Text>
        ) : (
          <ScrollArea.Autosize mah={360} type="auto">
            <Stack gap={4}>
              {sessions.map((session) => (
                <UnstyledButton
                  key={session.id}
                  disabled={importing !== null}
                  onClick={() => void pick(session.id)}
                  p="sm"
                  style={{
                    border: "1px solid var(--mantine-color-default-border)",
                    borderRadius: 8,
                    opacity: importing === null ? 1 : 0.6,
                  }}
                >
                  <Group justify="space-between" wrap="nowrap" gap="sm">
                    <div style={{ minWidth: 0 }}>
                      <Text size="sm" fw={500} truncate>
                        {session.title}
                      </Text>
                      <Text size="xs" c="dimmed" truncate>
                        {session.cwd}
                      </Text>
                    </div>
                    {importing === session.id ? (
                      <Loader size="xs" />
                    ) : (
                      <Text size="xs" c="dimmed" style={{ flexShrink: 0 }}>
                        {new Date(session.updatedAt).toLocaleDateString()}
                      </Text>
                    )}
                  </Group>
                </UnstyledButton>
              ))}
            </Stack>
          </ScrollArea.Autosize>
        )}
        {error ? (
          <Alert icon={<IconAlertCircle size={14} />} color="red">
            {error}
          </Alert>
        ) : null}
        <Group justify="flex-end">
          <Button variant="default" onClick={onClose} disabled={importing !== null}>
            Cancel
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
