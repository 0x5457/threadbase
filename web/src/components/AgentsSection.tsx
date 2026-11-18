import {
  ActionIcon,
  Alert,
  Button,
  Card,
  Group,
  Loader,
  Modal,
  Stack,
  Text,
  TextInput,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import { IconAlertCircle, IconPlus, IconX } from "@tabler/icons-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  addAgent,
  deleteAgent,
  listAgents,
  updateAgent,
  type Agent,
} from "../api";
import type { AgentUpsertRequest } from "../api/types/AgentUpsertRequest";

/** One KEY=VALUE pair in the editor. */
interface EnvDraft {
  key: string;
  value: string;
}

interface AgentDraft {
  label: string;
  command: string;
  args: string[];
  env: EnvDraft[];
}

const EMPTY_DRAFT: AgentDraft = { label: "", command: "", args: [], env: [] };

function draftOf(agent: Agent): AgentDraft {
  return {
    label: agent.label,
    command: agent.command,
    args: [...agent.args],
    env: agent.env.map((entry) => ({ ...entry })),
  };
}

function statusOf(agent: Agent) {
  if (!agent.installed) {
    return <Text size="xs" c="red">Not on PATH</Text>;
  }
  if (agent.viaNpx) {
    return (
      <Text size="xs" c="dimmed">
        Runs through npx
      </Text>
    );
  }
  return null;
}

interface AgentModalProps {
  /** `null` creates a new agent; otherwise the agent being edited. */
  agent: Agent | null;
  onClose: () => void;
}

function AgentModal({ agent, onClose }: AgentModalProps) {
  const phone = useMediaQuery("(max-width: 47.99em)");
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState<AgentDraft>(EMPTY_DRAFT);
  const [errors, setErrors] = useState<{ label?: string; command?: string }>(
    {},
  );
  const [failure, setFailure] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (agent) {
      setDraft(draftOf(agent));
    } else {
      setDraft(EMPTY_DRAFT);
    }
    setErrors({});
    setFailure(null);
  }, [agent]);

  const patch = (part: Partial<AgentDraft>) =>
    setDraft((current) => ({ ...current, ...part }));

  const save = async () => {
    const nextErrors: typeof errors = {};
    if (draft.label.trim() === "") nextErrors.label = "Give the agent a name";
    if (draft.command.trim() === "") nextErrors.command = "Name the command to run";
    setErrors(nextErrors);
    if (nextErrors.label || nextErrors.command) return;

    const input: AgentUpsertRequest = {
      label: draft.label.trim(),
      command: draft.command.trim(),
      args: draft.args.map((arg) => arg.trim()).filter(Boolean),
      env: draft.env
        .map((entry) => ({ key: entry.key.trim(), value: entry.value }))
        .filter((entry) => entry.key !== ""),
    };
    setSaving(true);
    setFailure(null);
    try {
      if (agent) {
        await updateAgent(agent.id, input);
        notifications.show({ message: "Agent saved", color: "teal" });
      } else {
        await addAgent(input);
        notifications.show({ message: "Agent added", color: "teal" });
      }
      void queryClient.invalidateQueries({ queryKey: ["agents"] });
      onClose();
    } catch (cause) {
      setFailure(
        cause instanceof Error ? cause.message : "Could not save the agent",
      );
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      opened
      onClose={onClose}
      title={agent ? "Edit agent" : "Add agent"}
      size="md"
      centered
      fullScreen={phone}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <Stack gap="sm">
          <TextInput
            label="Name"
            description={
              agent
                ? `Renaming only changes the label; threads keep addressing it as ${agent.id}.`
                : "Shown wherever you pick an agent."
            }
            placeholder="DeepSeek Harness"
            value={draft.label}
            error={errors.label}
            onChange={(event) => {
              patch({ label: event.currentTarget.value });
              if (errors.label) setErrors((current) => ({ ...current, label: undefined }));
            }}
            data-autofocus
          />
          <TextInput
            label="Command"
            description="One executable name or path on the server; it runs without a shell."
            placeholder="dsh"
            value={draft.command}
            error={errors.command}
            onChange={(event) => {
              patch({ command: event.currentTarget.value });
              if (errors.command)
                setErrors((current) => ({ ...current, command: undefined }));
            }}
            spellCheck={false}
            autoCorrect="off"
            autoCapitalize="off"
            style={{ fontFamily: "var(--mono)" }}
          />
          <Stack gap={6}>
            <Text size="sm" fw={500}>
              Arguments
            </Text>
            {draft.args.map((arg, index) => (
              <Group key={index} gap={6} wrap="nowrap" align="center">
                <TextInput
                  aria-label={`Argument ${index + 1}`}
                  placeholder="--profile"
                  value={arg}
                  onChange={(event) =>
                    patch({
                      args: draft.args.map((value, at) =>
                        at === index ? event.currentTarget.value : value,
                      ),
                    })
                  }
                  spellCheck={false}
                  autoCorrect="off"
                  autoCapitalize="off"
                  style={{ fontFamily: "var(--mono)" }}
                />
                <ActionIcon
                  variant="subtle"
                  color="gray"
                  aria-label={`Remove argument ${index + 1}`}
                  onClick={() =>
                    patch({ args: draft.args.filter((_, at) => at !== index) })
                  }
                >
                  <IconX size={14} stroke={1.5} />
                </ActionIcon>
              </Group>
            ))}
            <Button
              variant="subtle"
              size="compact-xs"
              leftSection={<IconPlus size={14} stroke={1.5} />}
              onClick={() => patch({ args: [...draft.args, ""] })}
            >
              Add argument
            </Button>
            <Text size="xs" c="dimmed">
              One literal argument per row, passed exactly as written.
            </Text>
          </Stack>
          <Stack gap={6}>
            <Text size="sm" fw={500}>
              Environment variables
            </Text>
            {draft.env.map((entry, index) => (
              <Group
                key={index}
                gap={6}
                wrap={phone ? "wrap" : "nowrap"}
                align="center"
              >
                <TextInput
                  aria-label={`Variable ${index + 1} name`}
                  placeholder="DEEPSEEK_API_KEY"
                  value={entry.key}
                  onChange={(event) =>
                    patch({
                      env: draft.env.map((value, at) =>
                        at === index
                          ? { ...value, key: event.currentTarget.value }
                          : value,
                      ),
                    })
                  }
                  spellCheck={false}
                  autoCorrect="off"
                  autoCapitalize="characters"
                  style={{ fontFamily: "var(--mono)" }}
                  w={phone ? "100%" : 200}
                />
                <TextInput
                  aria-label={`Variable ${index + 1} value`}
                  placeholder="sk-…"
                  value={entry.value}
                  onChange={(event) =>
                    patch({
                      env: draft.env.map((value, at) =>
                        at === index
                          ? { ...value, value: event.currentTarget.value }
                          : value,
                      ),
                    })
                  }
                  style={{ flex: 1 }}
                />
                <ActionIcon
                  variant="subtle"
                  color="gray"
                  aria-label={`Remove variable ${index + 1}`}
                  onClick={() =>
                    patch({ env: draft.env.filter((_, at) => at !== index) })
                  }
                >
                  <IconX size={14} stroke={1.5} />
                </ActionIcon>
              </Group>
            ))}
            <Button
              variant="subtle"
              size="compact-xs"
              leftSection={<IconPlus size={14} stroke={1.5} />}
              onClick={() =>
                patch({ env: [...draft.env, { key: "", value: "" }] })
              }
            >
              Add variable
            </Button>
            <Text size="xs" c="dimmed">
              Set for the agent process, for example API keys or base URLs.
            </Text>
          </Stack>
          {failure ? (
            <Alert icon={<IconAlertCircle size={14} />} color="red">
              {failure}
            </Alert>
          ) : null}
          <Group justify="flex-end" gap="xs" mt={4}>
            <Button variant="default" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" color="dark" loading={saving}>
              {agent ? "Save changes" : "Add agent"}
            </Button>
          </Group>
        </Stack>
      </form>
    </Modal>
  );
}

export function AgentsSection({ isAdmin }: { isAdmin: boolean }) {
  const queryClient = useQueryClient();
  const agents = useQuery({
    queryKey: ["agents"],
    queryFn: listAgents,
    staleTime: 30_000,
  });
  const [editing, setEditing] = useState<Agent | null | undefined>(undefined);
  const [removing, setRemoving] = useState<Agent | null>(null);
  const [removingBusy, setRemovingBusy] = useState(false);

  const remove = async () => {
    if (!removing) return;
    setRemovingBusy(true);
    try {
      await deleteAgent(removing.id);
      notifications.show({
        message: `${removing.label} removed`,
        color: "teal",
      });
      setRemoving(null);
      void queryClient.invalidateQueries({ queryKey: ["agents"] });
    } catch (cause) {
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not remove the agent",
        color: "red",
      });
    } finally {
      setRemovingBusy(false);
    }
  };

  return (
    <Card withBorder p="md" className="settings-card">
      <Stack gap="sm">
        <Group justify="space-between" align="center">
          <Text size="sm" fw={500}>
            Agents
          </Text>
          {isAdmin ? (
            <Button
              size="compact-xs"
              variant="default"
              leftSection={<IconPlus size={14} stroke={1.5} />}
              onClick={() => setEditing(null)}
            >
              Add agent
            </Button>
          ) : null}
        </Group>
        <Text size="xs" c="dimmed">
          The coding agents a thread can run. Anything you add here appears in
          every agent menu.
        </Text>

        {agents.isPending ? (
          <Group justify="center" py="sm">
            <Loader size="xs" />
          </Group>
        ) : null}

        {agents.isError ? (
          <Alert
            icon={<IconAlertCircle size={14} />}
            color="red"
            variant="light"
            p="xs"
          >
            <Text size="xs">
              {agents.error instanceof Error
                ? agents.error.message
                : "Could not load agents"}
            </Text>
          </Alert>
        ) : null}

        {(agents.data ?? []).map((agent) => (
          <Group
            key={agent.id}
            justify="space-between"
            align="center"
            wrap="nowrap"
            gap="sm"
          >
            <Stack gap={1} style={{ minWidth: 0 }}>
              <Text size="sm" fw={500} truncate>
                {agent.label}
              </Text>
              <Text
                size="xs"
                c="dimmed"
                truncate
                style={{ fontFamily: "var(--mono)" }}
              >
                {[agent.command, ...agent.args].join(" ")}
                {agent.env.length > 0
                  ? ` · ${agent.env.length} env var${agent.env.length === 1 ? "" : "s"}`
                  : ""}
              </Text>
            </Stack>
            <Group gap="xs" wrap="nowrap">
              {statusOf(agent)}
              {isAdmin && agent.custom ? (
                <Group gap={5} wrap="nowrap">
                  <Button
                    size="compact-xs"
                    variant="default"
                    onClick={() => setEditing(agent)}
                  >
                    Edit
                  </Button>
                  <Button
                    size="compact-xs"
                    variant="subtle"
                    color="red"
                    onClick={() => setRemoving(agent)}
                  >
                    Remove
                  </Button>
                </Group>
              ) : null}
            </Group>
          </Group>
        ))}
      </Stack>

      {editing !== undefined ? (
        <AgentModal agent={editing} onClose={() => setEditing(undefined)} />
      ) : null}

      <Modal
        opened={removing !== null}
        onClose={() => setRemoving(null)}
        title="Remove agent"
        centered
      >
        <Text size="sm" c="dimmed" mb="md">
          <strong>{removing?.label}</strong> disappears from every agent menu.
          Threads already using it fall back to the default agent at their next
          start.
        </Text>
        <Group justify="flex-end" gap="xs">
          <Button variant="default" onClick={() => setRemoving(null)}>
            Cancel
          </Button>
          <Button color="red" loading={removingBusy} onClick={() => void remove()}>
            Remove
          </Button>
        </Group>
      </Modal>
    </Card>
  );
}
