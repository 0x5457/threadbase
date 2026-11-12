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
          if (event.key === "Enter") void submit();
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
          ? `Merged ${result.commit?.slice(0, 7) ?? ""}`
          : `Merged ${result.files.length} files`,
        color: "teal",
      });
      onMerged(result);
      onClose();
    } catch (cause) {
      failure(cause, "Merge failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal opened={opened} onClose={onClose} title="Merge thread" size="md" centered>
      <Text size="sm" c="dimmed" mb="md">
        Merge <strong>{thread?.title}</strong> back into{" "}
        <strong>{thread?.baseBranch ?? "main"}</strong> and commit the worktree
        changes.
      </Text>
      <Group justify="flex-end" gap="xs">
        <Button variant="default" onClick={onClose}>
          Cancel
        </Button>
        <Button color="dark" loading={busy} onClick={() => void submit()}>
          Merge
        </Button>
      </Group>
    </Modal>
  );
}
