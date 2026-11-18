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
import { IconAlertCircle, IconFolder } from "@tabler/icons-react";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { updateProject, type Project } from "../api";
import { useDelayedFlag } from "../lib/use-delayed-flag";
import type { PermissionMode } from "../api/types/PermissionMode";
import {
  PERMISSION_MODES,
  permissionModeHint,
  permissionModeLabel,
  permissionModeManaged,
  usePermissionPolicy,
} from "../lib/permissions";

interface ProjectSettingsModalProps {
  opened: boolean;
  project: Project | null;
  onClose: () => void;
  onSaved?: (project: Project) => void;
}

export function ProjectSettingsModal({
  opened,
  project,
  onClose,
  onSaved,
}: ProjectSettingsModalProps) {
  const phone = useMediaQuery("(max-width: 47.99em)");
  const permissions = usePermissionPolicy();
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [remoteUrl, setRemoteUrl] = useState("");
  const [verifyCommand, setVerifyCommand] = useState("");
  const [setupCommand, setSetupCommand] = useState("");
  const [permissionMode, setPermissionMode] = useState<string>("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const showSaving = useDelayedFlag(saving, 300, 500);

  useEffect(() => {
    if (!opened) return;
    setName(project?.name ?? "");
    setPath(project?.path ?? "");
    setRemoteUrl("");
    setVerifyCommand(project?.verifyCommand ?? "");
    setSetupCommand(project?.setupCommand ?? "");
    setPermissionMode(project?.permissionMode ?? "");
    setError(null);
  }, [opened, project]);

  const trimmedName = name.trim();
  const trimmedPath = path.trim();
  const canSave = project !== null && trimmedName !== "" && trimmedPath !== "";

  const save = async () => {
    if (!project || !canSave) return;
    setSaving(true);
    setError(null);
    try {
      const trimmedRemote = remoteUrl.trim();
      const { project: updated } = await updateProject(project.id, {
        name: trimmedName,
        path: trimmedPath,
        remoteUrl: trimmedRemote || undefined,
        verifyCommand: verifyCommand.trim(),
        setupCommand: setupCommand.trim(),
        permissionMode,
      });
      notifications.show({ message: `${updated.name} updated`, color: "teal" });
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
      onSaved?.(updated);
      onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Cannot save project");
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Project settings"
      size="md"
      centered
      fullScreen={phone}
      closeOnClickOutside={!saving}
      closeOnEscape={!saving}
      withCloseButton={!saving}
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
            placeholder="Project name"
            value={name}
            onChange={(event) => setName(event.currentTarget.value)}
            data-autofocus
          />
          <TextInput
            label="Directory"
            description="Absolute path to a git repository on this machine."
            placeholder="/home/you/work/project"
            leftSection={<IconFolder size={14} stroke={1.5} />}
            value={path}
            onChange={(event) => setPath(event.currentTarget.value)}
          />
          <Group gap={6} wrap="nowrap">
            <Text size="sm" c="dimmed">
              Base branch
            </Text>
            <Text size="sm">{project?.defaultBranch ?? "—"}</Text>
          </Group>
          <TextInput
            label="Remote URL"
            description="Optional. Set or change the git remote for this project."
            placeholder="https://github.com/org/project.git"
            value={remoteUrl}
            onChange={(event) => setRemoteUrl(event.currentTarget.value)}
          />
          <TextInput
            label="Setup command"
            description="Optional. Runs once in each new thread worktree before the agent starts."
            placeholder="pnpm install"
            value={setupCommand}
            onChange={(event) => setSetupCommand(event.currentTarget.value)}
          />
          <TextInput
            label="Verify command"
            description="Optional. Runs in the thread worktree before Apply changes; it must exit 0."
            placeholder="pnpm test"
            value={verifyCommand}
            onChange={(event) => setVerifyCommand(event.currentTarget.value)}
          />
          <Select
            label="Permissions"
            description="Default for new threads here. Each thread can still choose its own."
            data={[
              { value: "", label: "Inherit instance default" },
              ...PERMISSION_MODES.map((mode) => ({
                value: mode,
                label: permissionModeManaged(mode, permissions.cap)
                  ? `${permissionModeLabel(mode)} · managed`
                  : permissionModeLabel(mode),
                disabled: permissionModeManaged(mode, permissions.cap),
              })),
            ]}
            value={permissionMode}
            onChange={(value) => setPermissionMode(value ?? "")}
            allowDeselect={false}
            comboboxProps={{ withinPortal: false }}
            title={permissionModeHint(
              (permissionMode || permissions.defaultMode) as PermissionMode,
            )}
          />
          {error ? (
            <Alert
              icon={<IconAlertCircle size={14} />}
              color="red"
              title="Cannot save project"
            >
              {error}
            </Alert>
          ) : null}
          <Group justify="flex-end" gap="xs" mt={4}>
            <Button variant="default" onClick={onClose} disabled={saving}>
              Cancel
            </Button>
            <Button
              type="submit"
              color="dark"
              loading={showSaving}
              disabled={saving || !canSave}
            >
              Save
            </Button>
          </Group>
        </Stack>
      </form>
    </Modal>
  );
}
