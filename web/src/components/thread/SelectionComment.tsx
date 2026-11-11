import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CornerDownLeft } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button, Flex, Kbd, Paper, Textarea } from "@mantine/core";
import { createComment } from "@/api/endpoints";
import { queryKeys } from "@/api/queries";
import { modKeyLabel } from "@/lib/platform";
import { notifyError } from "@/lib/notify";

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
  const paperRef = useRef<HTMLDivElement>(null);
  const create = useMutation({
    onError: (error) => notifyError(error, "Cannot save comment"),
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
  // Clicking outside dismisses the popover; without this it lingered over the
  // transcript until the next selection.
  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (
        paperRef.current &&
        event.target instanceof Node &&
        !paperRef.current.contains(event.target)
      ) {
        onClose();
      }
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    return () =>
      document.removeEventListener("pointerdown", onPointerDown, true);
  }, [onClose]);

  return (
    <Paper
      ref={paperRef}
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
        radius="sm"
        rows={3}
      />
      <Flex
        align="center"
        justify="space-between"
        gap="xs"
        mt={4}
        style={{ fontSize: "var(--mantine-font-size-xs)", color: "var(--mantine-color-dimmed)" }}
      >
        <span>
          <Kbd>{modKeyLabel()}</Kbd> <Kbd>Enter</Kbd> pending comment
        </span>
        <Button
          type="button"
          variant="subtle"
          color="blue"
          size="compact-xs"
          radius="sm"
          className="tb-action-40"
          loading={create.isPending}
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
