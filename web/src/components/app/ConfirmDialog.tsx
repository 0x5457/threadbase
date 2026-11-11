import { Button, Group, Modal, Text } from "@mantine/core";

export interface ConfirmRequest {
  title: string;
  description: string;
  confirmLabel: string;
  cancelLabel?: string;
  /** `red` marks irreversible actions; approvals stay neutral. */
  confirmColor?: string;
  onConfirm: () => void;
}

/**
 * One confirmation surface for destructive actions. Replaces `window.confirm`
 * so the dialog follows the theme, traps focus and can be read by assistive
 * technology like every other modal in the app.
 */
export function ConfirmDialog({
  request,
  onClose,
}: {
  request: ConfirmRequest | null;
  onClose: () => void;
}) {
  return (
    <Modal
      opened={Boolean(request)}
      onClose={onClose}
      title={request?.title}
      size="sm"
    >
      <Text size="sm" c="dimmed">
        {request?.description}
      </Text>
      <Group justify="flex-end" gap="xs" mt="md">
        <Button
          variant="default"
          size="sm"
          className="tb-action-40"
          onClick={onClose}
        >
          {request?.cancelLabel ?? "Cancel"}
        </Button>
        <Button
          color={request?.confirmColor ?? "red"}
          variant={request?.confirmColor === "gray" ? "default" : "filled"}
          size="sm"
          className="tb-action-40"
          onClick={() => {
            request?.onConfirm();
            onClose();
          }}
        >
          {request?.confirmLabel}
        </Button>
      </Group>
    </Modal>
  );
}
