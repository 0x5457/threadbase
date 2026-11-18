import {
  Alert,
  Button,
  Group,
  Modal,
  Stack,
  Text,
  Textarea,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { IconAlertTriangle } from "@tabler/icons-react";
import { useEffect, useState } from "react";
import type { ViewItem } from "../../lib/thread-events";
import { useDelayedFlag } from "../../lib/use-delayed-flag";
import { LoadError } from "../loading/LoadError";

/**
 * A prompt can be edited only by restarting from it: the adapter keeps its own
 * conversation state, so Threadbase discards the later log and opens a fresh
 * ACP session. The dialog says so before the destructive request is sent.
 */
export function EditMessageDialog({
  message,
  onClose,
  onSave,
}: {
  message: Extract<ViewItem, { kind: "message" }> | null;
  onClose: () => void;
  /** Rejects on failure; the caller owns the notification. */
  onSave: (text: string) => Promise<void>;
}) {
  const phone = useMediaQuery("(max-width: 47.99em)");
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const showBusy = useDelayedFlag(busy, 300, 500);

  useEffect(() => {
    if (message) setText(message.text.replace(/^\n+|\n+$/g, ""));
  }, [message]);

  const trimmed = text.trim();
  const unchanged = message !== null && trimmed === message.text.trim();
  const close = () => {
    if (!busy) onClose();
  };
  const save = async () => {
    if (!message || busy || trimmed.length === 0 || unchanged) return;
    setBusy(true);
    setError(null);
    try {
      await onSave(trimmed);
    } catch (cause) {
      // Keep the draft open so a transient failure does not throw the edit away.
      setError(
        cause instanceof Error ? cause.message : "Could not edit the message",
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      opened={message !== null}
      onClose={close}
      title="Edit message"
      size="lg"
      centered
      fullScreen={phone}
      closeOnClickOutside={!busy}
      closeOnEscape={!busy}
    >
      <Stack gap="sm">
        <Textarea
          value={text}
          onChange={(event) => setText(event.currentTarget.value)}
          autosize
          minRows={3}
          maxRows={10}
          aria-label="Message text"
          disabled={busy}
          data-autofocus
        />
        <Alert
          color="yellow"
          variant="light"
          icon={<IconAlertTriangle size={15} />}
          title="This rewinds the thread"
        >
          <Text size="sm">
            Saving deletes every message after this one, cancels queued input
            and starts the agent over from here in a new session. Earlier turns
            are no longer in its context, and files in the worktree are left as
            they are.
          </Text>
        </Alert>
        {showBusy ? (
          <Text size="xs" c="dimmed" role="status">
            Rewinding the transcript and restarting the agent…
          </Text>
        ) : null}
        {error ? (
          <LoadError
            title="Could not edit the message"
            message={error}
            compact
          />
        ) : null}
        <Group justify="flex-end" gap="xs">
          <Button variant="default" onClick={close} disabled={busy}>
            Cancel
          </Button>
          <Button
            onClick={() => void save()}
            loading={showBusy}
            disabled={busy || trimmed.length === 0 || unchanged}
          >
            Save &amp; restart
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
