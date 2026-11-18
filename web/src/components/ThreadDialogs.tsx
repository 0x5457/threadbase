import { Button, Group, Modal, Text, TextInput } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useEffect, useState } from "react";
import { useDelayedFlag } from "../lib/use-delayed-flag";
import { LoadError } from "./loading/LoadError";
import { useProgressJobs } from "./loading/Progress";
import {
  mergeThread,
  revertThread,
  updateThread,
  type MergeBackResponse,
  type ThreadSummary,
} from "../api";
import { isImeConfirming } from "../lib/keyboard";

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
  const [error, setError] = useState<string | null>(null);
  const showSaving = useDelayedFlag(saving, 300, 500);

  useEffect(() => {
    if (opened) {
      setTitle(thread?.title ?? "");
      setError(null);
    }
  }, [opened, thread?.title]);

  const submit = async () => {
    if (!thread || !title.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const { thread: updated } = await updateThread(thread.id, {
        title: title.trim(),
      });
      onRenamed(updated);
      onClose();
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Could not rename thread",
      );
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Rename thread"
      size="md"
      centered
      closeOnClickOutside={!saving}
      closeOnEscape={!saving}
      withCloseButton={!saving}
    >
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
      {error ? (
        <LoadError
          title="Could not rename thread"
          message={error}
          compact
          className="dialog-error"
        />
      ) : null}
      <Group justify="flex-end" gap="xs" mt="md">
        <Button variant="default" onClick={onClose}>
          Cancel
        </Button>
        <Button
          color="dark"
          loading={showSaving}
          disabled={saving || !title.trim()}
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
  const [error, setError] = useState<string | null>(null);
  const showBusy = useDelayedFlag(busy, 300, 500);

  useEffect(() => {
    if (opened) setError(null);
  }, [opened]);

  const submit = async () => {
    if (!thread) return;
    setBusy(true);
    setError(null);
    try {
      await revertThread(thread.id);
      notifications.show({ message: "Changes reverted", color: "teal" });
      onReverted();
      onClose();
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Could not revert changes",
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Revert changes"
      size="md"
      centered
      closeOnClickOutside={!busy}
      closeOnEscape={!busy}
      withCloseButton={!busy}
    >
      <Text size="sm" c="dimmed" mb="md">
        Revert the worktree of <strong>{thread?.title}</strong> to its base
        commit. The conversation stays.
      </Text>
      {showBusy ? (
        <Text size="xs" c="dimmed" mb="sm" role="status">
          Resetting the worktree…
        </Text>
      ) : null}
      {error ? (
        <LoadError
          title="Could not revert changes"
          message={error}
          compact
          className="dialog-error"
        />
      ) : null}
      <Group justify="flex-end" gap="xs">
        <Button variant="default" onClick={onClose} disabled={busy}>
          Cancel
        </Button>
        <Button
          color="red"
          loading={showBusy}
          disabled={busy}
          onClick={() => void submit()}
        >
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
  const [error, setError] = useState<string | null>(null);
  const showBusy = useDelayedFlag(busy, 300, 500);
  const {
    start: startJob,
    update: updateJob,
    finish: finishJob,
    fail: failJob,
  } = useProgressJobs();

  useEffect(() => {
    if (opened) setError(null);
  }, [opened]);

  const submit = async () => {
    if (!thread) return;
    setBusy(true);
    setError(null);
    const jobId = startJob({
      key: `merge:${thread.id}`,
      title: `Applying ${thread.title}`,
      detail: "Verifying the worktree, then applying and committing changes",
      stage: "The verify command can take several minutes.",
    });
    try {
      const result = await mergeThread(thread.id);
      finishJob(
        jobId,
        result.committed
          ? `Applied ${result.commit?.slice(0, 7) ?? ""} to ${result.branch}`
          : `Applied ${result.files.length} files to ${result.branch}`,
      );
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
      const message = cause instanceof Error ? cause.message : "Merge failed";
      setError(message);
      updateJob(jobId, { onRetry: () => void submit() });
      failJob(jobId, message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Apply changes"
      size="md"
      centered
      closeOnClickOutside={!busy}
      closeOnEscape={!busy}
      withCloseButton={!busy}
    >
      <Text size="sm" c="dimmed" mb="md">
        Apply the worktree changes from <strong>{thread?.title}</strong> to the
        project working tree on its <strong>base branch</strong> and commit them.
        The project must be clean; the project verify command (if set) runs in
        the thread worktree first. Nothing is pushed and branches are not
        switched.
      </Text>
      {showBusy ? (
        <Text size="xs" c="dimmed" mb="sm" role="status">
          Verifying and applying changes. This can take several minutes.
        </Text>
      ) : null}
      {error ? (
        <LoadError
          title="Merge failed"
          message={error}
          compact
          className="dialog-error"
        />
      ) : null}
      <Group justify="flex-end" gap="xs">
        <Button variant="default" onClick={onClose} disabled={busy}>
          Cancel
        </Button>
        <Button
          color="dark"
          loading={showBusy}
          disabled={busy}
          onClick={() => void submit()}
        >
          Apply
        </Button>
      </Group>
    </Modal>
  );
}
