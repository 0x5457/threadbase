import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";
import { createComment } from "@/api/endpoints";
import { queryKeys } from "@/api/queries";
import type { CommentAnchor } from "@/api/types/CommentAnchor";
import {
  Button,
  Dialog,
  DialogContent,
  Textarea,
} from "@/components/ui";

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
      toast.success("Comment saved");
    },
    onError: (error) => toast.error(String(error)),
  });
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title="Add comment"
        description="Comments are attached to this anchor and delivered with the next message, or sent immediately."
        footer={
          <>
            <Button size="sm" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button
              size="sm"
              variant="secondary"
              disabled={!body.trim()}
              loading={create.isPending}
              onClick={() => create.mutate(false)}
            >
              Save pending
            </Button>
            <Button
              size="sm"
              variant="primary"
              disabled={!body.trim()}
              loading={create.isPending}
              onClick={() => create.mutate(true)}
            >
              Send now
            </Button>
          </>
        }
      >
        <Textarea
          autoFocus
          value={body}
          onChange={(event) => setBody(event.target.value)}
          placeholder="What should the agent know about this?"
          rows={4}
        />
      </DialogContent>
    </Dialog>
  );
}
