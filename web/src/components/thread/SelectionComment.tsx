import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CornerDownLeft } from "lucide-react";
import { useState } from "react";
import { createComment } from "@/api/endpoints";
import { queryKeys } from "@/api/queries";
import { Kbd, Textarea } from "@/components/ui";

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
    <div
      className="fixed z-50 w-72 rounded-lg border border-border bg-panel p-2 shadow-2xl"
      style={{
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
        className="text-[11px]"
      />
      <div className="mt-1 flex items-center justify-between text-[9.5px] text-faint">
        <span>
          <Kbd>⌘</Kbd> <Kbd>Enter</Kbd> pending comment
        </span>
        <button
          type="button"
          className="flex items-center gap-1 text-accent disabled:opacity-40"
          disabled={!body.trim() || create.isPending}
          onClick={() => create.mutate()}
        >
          <CornerDownLeft className="size-3" aria-hidden />
          Add
        </button>
      </div>
    </div>
  );
}
