import {
  Alert,
  Button,
  Group,
  Modal,
  Select,
  Stack,
  Text,
  TextInput,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import { IconAlertCircle, IconMessage } from "@tabler/icons-react";
import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  createThread,
  getState,
  type Project,
  type ThreadSummary,
} from "../api";

interface NewThreadModalProps {
  opened: boolean;
  project: Project | null;
  onClose: () => void;
  onCreated: (thread: ThreadSummary) => void;
}

const agents = [
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude Code" },
  { value: "opencode", label: "opencode" },
  { value: "pi", label: "pi" },
];

export function NewThreadModal({
  opened,
  project,
  onClose,
  onCreated,
}: NewThreadModalProps) {
  const phone = useMediaQuery("(max-width: 48em)");
  const [title, setTitle] = useState("");
  const [agent, setAgent] = useState("codex");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const stateQuery = useQuery({
    queryKey: ["state"],
    queryFn: getState,
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
    if (!project || !title.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      const { thread } = await createThread(project.id, {
        title: title.trim(),
        agent,
        // The model picker lives in the thread composer now, where it can show
        // what the agent actually advertises. Only the saved default applies here.
        model: stateQuery.data?.settings.defaultModel?.trim() || null,
        baseBranch: project.defaultBranch,
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
    >
      <Stack gap="sm">
        <TextInput
          label="Title"
          placeholder="What should the agent work on?"
          leftSection={<IconMessage size={14} stroke={1.5} />}
          value={title}
          onChange={(event) => setTitle(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void create();
          }}
          autoFocus
          data-autofocus
        />
        <Select
          label="Agent"
          data={agents}
          value={agent}
          onChange={(value) => setAgent(value ?? "codex")}
          allowDeselect={false}
        />
        <Text size="xs" c="dimmed">
          Base branch <strong>{project?.defaultBranch ?? "main"}</strong>. The
          agent gets its own worktree so every change stays reviewable. Pick the
          model inside the thread once the agent reports what it offers.
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
            disabled={!title.trim()}
            onClick={() => void create()}
          >
            Start thread
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
