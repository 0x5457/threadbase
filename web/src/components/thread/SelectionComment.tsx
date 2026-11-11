import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CornerDownLeft } from "lucide-react";
import { useState } from "react";
import { Button, Flex, Kbd, Paper, Textarea } from "@mantine/core";
import { createComment } from "@/api/endpoints";
import { queryKeys } from "@/api/queries";

export interface SelectionTarget {
  messageId: string;
  x: number;
  y: number;
}

export function SelectionComment({
  threadId,
  target,
  onClose,
}: {
  threadId: string;
  target: SelectionTarget;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [body, setBody] = useState("");
  const create = useMutation({
    mutationFn: () =>
      createComment(threadId, {
        body: body.trim(),
        anchor: { kind: "message", messageId: target.messageId },
        parentId: null,
        send: false,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.comments(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadEvents(threadId),
      });
      onClose();
    },
  });
  return (
    <Paper
      withBorder
      p="xs"
      shadow="xl"
      radius="md"
      style={{
        position: "fixed",
        zIndex: 50,
        width: 288,
        left: Math.min(target.x, window.innerWidth - 300),
        top: Math.min(target.y + 10, window.innerHeight - 150),
      }}
      onMouseDown={(event) => event.stopPropagation()}
    >
      <Textarea
        autoFocus
        value={body}
        onChange={(event) => setBody(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            onClose();
          }
          if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
            event.preventDefault();
            if (body.trim()) {
              create.mutate();
            }
          }
        }}
        placeholder="Comment on selection…"
        rows={3}
      />
      <Flex
        align="center"
        justify="space-between"
        gap="xs"
        mt={4}
        style={{ fontSize: 11, color: "var(--mantine-color-dimmed)" }}
      >
        <span>
          <Kbd>⌘</Kbd> <Kbd>Enter</Kbd> pending comment
        </span>
        <Button
          type="button"
          variant="subtle"
          color="blue"
          size="compact-xs"
          disabled={!body.trim() || create.isPending}
          onClick={() => create.mutate()}
        >
          <CornerDownLeft size={12} aria-hidden />
          Add
        </Button>
      </Flex>
    </Paper>
  );
}
