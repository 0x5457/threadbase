import {
  Alert,
  Button,
  Checkbox,
  Group,
  Modal,
  Select,
  Stack,
  Text,
  TextInput,
  Textarea,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { IconAlertCircle, IconMessage, IconTerminal2 } from "@tabler/icons-react";
import { useEffect, useState } from "react";
import {
  agentItems,
  createThreadFromTmux,
  getState,
  listAgents,
  type ThreadSummary,
  type TmuxSessionSummary,
} from "../../api";
import { readPick } from "../../lib/model-prefs";

function defaultPrompt(session: TmuxSessionSummary) {
  return `Continue the work from the tmux session "${session.name}"${
    session.cwd ? `. It was running in ${session.cwd}` : ""
  }. Inspect the current state before making changes.`;
}

/**
 * Turn a host tmux session into an agent thread. The project is inferred from
 * the session's live working directory on the server, so this dialog never
 * needs a project picker and cannot create a thread in the wrong checkout.
 */
export function ThreadFromTmuxDialog({
  opened,
  session,
  onClose,
  onCreated,
}: {
  opened: boolean;
  session: TmuxSessionSummary | null;
  onClose: () => void;
  onCreated: (thread: ThreadSummary) => void;
}) {
  const phone = useMediaQuery("(max-width: 47.99em)");
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [agent, setAgent] = useState("codex");
  const [worktree, setWorktree] = useState(true);
  const [prompt, setPrompt] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const stateQuery = useQuery({
    queryKey: ["state"],
    queryFn: getState,
    enabled: opened,
    staleTime: 60_000,
  });
  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: listAgents,
    enabled: opened,
    staleTime: 60_000,
  });

  useEffect(() => {
    if (!opened || !session) return;
    setTitle(session.name);
    setPrompt("");
    setError(null);
    try {
      setWorktree(
        window.localStorage.getItem("threadbase.new-thread-worktree") !== "0",
      );
    } catch {
      setWorktree(true);
    }
  }, [opened, session]);

  useEffect(() => {
    if (!opened || !stateQuery.data) return;
    setAgent(stateQuery.data.settings.defaultAgent);
  }, [opened, stateQuery.data]);

  const create = async () => {
    if (!session) return;
    setSubmitting(true);
    setError(null);
    try {
      const { thread } = await createThreadFromTmux(session.socket, session.id, {
        title: title.trim() || null,
        agent,
        model:
          readPick(agent, "model") ??
          stateQuery.data?.settings.defaultModel?.trim() ??
          null,
        effort: readPick(agent, "effort"),
        worktree,
        initialPrompt: prompt.trim() || null,
      });
      notifications.show({ message: "Thread created from tmux", color: "teal" });
      void queryClient.invalidateQueries({ queryKey: ["sessions"] });
      void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
      onCreated(thread);
      onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Cannot create thread");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title={session ? `Start a thread from ${session.name}` : "Start a thread"}
      size="md"
      centered
      fullScreen={phone}
    >
      {session ? (
        <Stack gap="sm">
          <Alert
            variant="light"
            color="gray"
            icon={<IconTerminal2 size={14} stroke={1.5} />}
          >
            <Text size="xs">
              {session.projectName ?? "No project"} ·{" "}
              <span style={{ fontFamily: "var(--mono)" }}>{session.cwd ?? "unknown cwd"}</span>
            </Text>
          </Alert>
          <TextInput
            label="Title"
            placeholder="Thread title"
            leftSection={<IconMessage size={14} stroke={1.5} />}
            value={title}
            onChange={(event) => setTitle(event.currentTarget.value)}
            autoFocus
          />
          <Select
            label="Agent"
            data={agentItems(agentsQuery.data)}
            value={agent}
            onChange={(value) => setAgent(value ?? "codex")}
            allowDeselect={false}
          />
          <Checkbox
            checked={worktree}
            onChange={(event) => {
              const next = event.currentTarget.checked;
              setWorktree(next);
              try {
                window.localStorage.setItem(
                  "threadbase.new-thread-worktree",
                  next ? "1" : "0",
                );
              } catch {
                // Private mode: the preference simply does not persist.
              }
            }}
            label="Isolated git worktree"
            description={
              worktree
                ? "Keeps the agent's changes reviewable and out of the directory the tmux session is using."
                : "The agent edits the project directory directly, so its changes are live in the same tree as the tmux session."
            }
          />
          <Textarea
            label="First message"
            description="Optional. Leave empty to create an idle thread and send the first prompt later."
            value={prompt}
            onChange={(event) => setPrompt(event.currentTarget.value)}
            autosize
            minRows={3}
            maxRows={8}
          />
          {prompt.trim() ? null : (
            <Button
              variant="subtle"
              size="compact-xs"
              w="fit-content"
              onClick={() => setPrompt(defaultPrompt(session))}
            >
              Use a continuation prompt
            </Button>
          )}
          <Text size="xs" c="dimmed">
            The thread is created in{" "}
            <strong>{session.projectName ?? "the inferred project"}</strong>,
            using its default branch as the base. It stays linked to this tmux
            session.
          </Text>
          {error ? (
            <Alert icon={<IconAlertCircle size={14} />} color="red">
              {error}
            </Alert>
          ) : null}
          <Group justify="flex-end" gap="xs" mt={4}>
            <Button variant="default" onClick={onClose}>
              Cancel
            </Button>
            <Button
              color="dark"
              loading={submitting}
              onClick={() => void create()}
            >
              Start thread
            </Button>
          </Group>
        </Stack>
      ) : null}
    </Modal>
  );
}
