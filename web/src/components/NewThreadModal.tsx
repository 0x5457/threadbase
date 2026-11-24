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
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import { IconAlertCircle, IconHistory, IconMessage } from "@tabler/icons-react";
import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isImeConfirming } from "../lib/keyboard";
import { useDelayedFlag } from "../lib/use-delayed-flag";
import { readPick } from "../lib/model-prefs";
import {
  agentItems,
  createThread,
  getState,
  listAgents,
  type Project,
  type ThreadSummary,
} from "../api";

interface NewThreadModalProps {
  opened: boolean;
  project: Project | null;
  onClose: () => void;
  onCreated: (thread: ThreadSummary) => void;
  onImport: (project: Project) => void;
}

export function NewThreadModal({
  opened,
  project,
  onClose,
  onCreated,
  onImport,
}: NewThreadModalProps) {
  const phone = useMediaQuery("(max-width: 47.99em)");
  const [title, setTitle] = useState("");
  const [agent, setAgent] = useState("codex");
  const [worktree, setWorktree] = useState(() => {
    try {
      return window.localStorage.getItem("threadbase.new-thread-worktree") !== "0";
    } catch {
      return true;
    }
  });
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const showSubmitting = useDelayedFlag(submitting, 300, 500);

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
    if (!opened) return;
    setTitle("");
    setError(null);
  }, [opened]);

  useEffect(() => {
    if (!opened || !stateQuery.data) return;
    setAgent(stateQuery.data.settings.defaultAgent);
  }, [opened, stateQuery.data]);

  const create = async () => {
    if (!project) return;
    setSubmitting(true);
    setError(null);
    try {
      const { thread } = await createThread(project.id, {
        title: title.trim() || undefined,
        agent,
        // The model picker lives in the thread composer now, where it can show
        // what the agent actually advertises. Seed it with the last explicit
        // pick for this agent, falling back to the workspace default.
        model:
          readPick(agent, "model") ??
          stateQuery.data?.settings.defaultModel?.trim() ??
          null,
        effort: readPick(agent, "effort"),
        baseBranch: project.defaultBranch,
        worktree,
      });
      notifications.show({ message: "Thread created", color: "teal" });
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
      title={`New thread in ${project?.name ?? "project"}`}
      size="md"
      centered
      fullScreen={phone}
      closeOnClickOutside={!submitting}
      closeOnEscape={!submitting}
      withCloseButton={!submitting}
    >
      <Stack gap="sm">
        <TextInput
          label="Title"
          placeholder="Optional — named from your first message"
          leftSection={<IconMessage size={14} stroke={1.5} />}
          value={title}
          onChange={(event) => setTitle(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key !== "Enter" || isImeConfirming(event)) return;
            event.preventDefault();
            void create();
          }}
          autoFocus
          data-autofocus
        />
        <Select
          label="Agent"
          data={agentItems(agentsQuery.data)}
          value={agent}
          onChange={(value) => setAgent(value ?? "codex")}
          allowDeselect={false}
        />
        <Button
          variant="light"
          leftSection={<IconHistory size={14} stroke={1.5} />}
          onClick={() => {
            if (project) onImport(project);
          }}
          fullWidth
        >
          Continue an existing session
        </Button>
        <Text size="xs" c="dimmed">
          Already have a Codex or Claude Code conversation in this folder?
          Import it instead of starting a new one.
        </Text>
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
          description="A threadbase/<id> branch and worktree; changes stay reviewable and never touch your working tree."
        />
        <Text size="xs" c="dimmed">
          Leave the title empty to name the thread from the first line of your
          first message. Base branch{" "}
          <strong>{project?.defaultBranch ?? "main"}</strong>.{" "}
          {worktree
            ? "The agent gets its own worktree so every change stays reviewable."
            : "Without a worktree the agent edits the project directory directly, so its changes are live in your working tree."}{" "}
          Pick the model inside the thread once the agent reports what it
          offers.
        </Text>
        {error ? (
          <Alert icon={<IconAlertCircle size={14} />} color="red">
            {error}
          </Alert>
        ) : null}
        {showSubmitting ? (
          <Text size="xs" c="dimmed" role="status">
            Creating the branch and worktree…
          </Text>
        ) : null}
        <Group justify="flex-end" gap="xs" mt={4}>
          <Button variant="default" onClick={onClose} disabled={submitting}>
            Cancel
          </Button>
          <Button
            color="dark"
            loading={showSubmitting}
            disabled={submitting}
            onClick={() => void create()}
          >
            Start thread
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
