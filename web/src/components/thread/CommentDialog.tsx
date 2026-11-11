import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { notifications } from "@mantine/notifications";
import { Button, Group, Modal, Text, Textarea } from "@mantine/core";
import { createComment } from "@/api/endpoints";
import { queryKeys } from "@/api/queries";
import type { CommentAnchor } from "@/api/types/CommentAnchor";

export function CommentDialog({
  open,
  onOpenChange,
  threadId,
  anchor,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  threadId: string;
  anchor: CommentAnchor | null;
}) {
  const queryClient = useQueryClient();
  const [body, setBody] = useState("");
  const create = useMutation({
    mutationFn: (send: boolean) =>
      createComment(threadId, {
        body: body.trim(),
        anchor: anchor ?? { kind: "thread" },
        parentId: null,
        send,
      }),
    onSuccess: () => {
      setBody("");
      onOpenChange(false);
      void queryClient.invalidateQueries({
        queryKey: queryKeys.comments(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadEvents(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadBundle(threadId),
      });
      notifications.show({ message: "Comment saved", color: "green" });
    },
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
  });
  return (
    <Modal
      opened={open}
      onClose={() => onOpenChange(false)}
      title="Add comment"
      size="md"
    >
      <Text size="sm" c="dimmed" mb="sm">
        Comments are attached to this anchor and delivered with the next
        message, or sent immediately.
      </Text>
      <Textarea
        autoFocus
        value={body}
        onChange={(event) => setBody(event.target.value)}
        placeholder="What should the agent know about this?"
        rows={4}
        onKeyDown={(event) => {
          if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
            event.preventDefault();
            if (body.trim() && !create.isPending) {
              create.mutate(false);
            }
          }
        }}
      />
      <Group justify="flex-end" gap="xs" mt="sm">
        <Button
          variant="subtle"
          color="dark"
          className="tb-action-40"
          onClick={() => onOpenChange(false)}
        >
          Cancel
        </Button>
        <Button
          variant="outline"
          className="tb-action-40"
          disabled={!body.trim()}
          loading={create.isPending}
          onClick={() => create.mutate(false)}
        >
          Save pending
        </Button>
        <Button
          className="tb-action-40"
          disabled={!body.trim()}
          loading={create.isPending}
          onClick={() => create.mutate(true)}
        >
          Send now
        </Button>
      </Group>
    </Modal>
  );
}
