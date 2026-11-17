import { Button, Group, Modal, Text, TextInput } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useEffect, useState } from "react";
import {
  mergeThread,
  revertThread,
  updateThread,
  type MergeBackResponse,
  type ThreadSummary,
} from "../api";
import { isImeConfirming } from "../lib/keyboard";

function failure(cause: unknown, fallback: string) {
  notifications.show({
    message: cause instanceof Error ? cause.message : fallback,
    color: "red",
  });
}

export function RenameThreadDialog({
  opened,
  thread,
  onClose,
  onRenamed,
}: {
  opened: boolean;
  thread: ThreadSummary | null;
  onClose: () => void;
  onRenamed: (thread: ThreadSummary) => void;
}) {
  const [title, setTitle] = useState(thread?.title ?? "");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (opened) setTitle(thread?.title ?? "");
  }, [opened, thread?.title]);

  const submit = async () => {
    if (!thread || !title.trim()) return;
    setSaving(true);
    try {
      const { thread: updated } = await updateThread(thread.id, {
        title: title.trim(),
      });
      onRenamed(updated);
      onClose();
    } catch (cause) {
      failure(cause, "Could not rename thread");
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal opened={opened} onClose={onClose} title="Rename thread" size="md" centered>
      <TextInput
        label="Title"
        value={title}
        onChange={(event) => setTitle(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key !== "Enter" || isImeConfirming(event)) return;
          event.preventDefault();
          void submit();
        }}
        autoFocus
      />
      <Group justify="flex-end" gap="xs" mt="md">
        <Button variant="default" onClick={onClose}>
          Cancel
        </Button>
        <Button
          color="dark"
          loading={saving}
          disabled={!title.trim()}
          onClick={() => void submit()}
        >
          Save
        </Button>
      </Group>
    </Modal>
  );
}

export function RevertThreadDialog({
  opened,
  thread,
  onClose,
  onReverted,
}: {
  opened: boolean;
  thread: ThreadSummary | null;
  onClose: () => void;
  onReverted: () => void;
}) {
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (!thread) return;
    setBusy(true);
    try {
      await revertThread(thread.id);
      notifications.show({ message: "Changes reverted", color: "teal" });
      onReverted();
      onClose();
    } catch (cause) {
      failure(cause, "Could not revert changes");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal opened={opened} onClose={onClose} title="Revert changes" size="md" centered>
      <Text size="sm" c="dimmed" mb="md">
        Revert the worktree of <strong>{thread?.title}</strong> to its base
        commit. The conversation stays.
      </Text>
      <Group justify="flex-end" gap="xs">
        <Button variant="default" onClick={onClose}>
          Cancel
        </Button>
        <Button color="red" loading={busy} onClick={() => void submit()}>
          Revert
        </Button>
      </Group>
    </Modal>
  );
}

export function MergeThreadDialog({
  opened,
  thread,
  onClose,
  onMerged,
}: {
  opened: boolean;
  thread: ThreadSummary | null;
  onClose: () => void;
  onMerged: (result: MergeBackResponse) => void;
}) {
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (!thread) return;
    setBusy(true);
    try {
      const result = await mergeThread(thread.id);
      notifications.show({
        message: result.committed
          ? `Applied ${result.commit?.slice(0, 7) ?? ""} to ${result.branch}`
          : `Applied ${result.files.length} files to ${result.branch}`,
        color: "teal",
      });
      if (result.baseDrift) {
        notifications.show({
          message:
            "Applied, but the base branch moved since this thread started. Review the merged result.",
          color: "yellow",
        });
      }
      onMerged(result);
      onClose();
    } catch (cause) {
      failure(cause, "Merge failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal opened={opened} onClose={onClose} title="Apply changes" size="md" centered>
      <Text size="sm" c="dimmed" mb="md">
        Apply the worktree changes from <strong>{thread?.title}</strong> to the
        project working tree on its <strong>base branch</strong> and commit them.
        The project must be clean; the project verify command (if set) runs in
        the thread worktree first. Nothing is pushed and branches are not
        switched.
      </Text>
      <Group justify="flex-end" gap="xs">
        <Button variant="default" onClick={onClose}>
          Cancel
        </Button>
        <Button color="dark" loading={busy} onClick={() => void submit()}>
          Apply
        </Button>
      </Group>
    </Modal>
  );
}
