import { useMutation, useQueryClient } from "@tanstack/react-query";
import { MessageSquare, Send, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import {
  createComment,
  deleteComment,
  sendComment,
  updateComment,
} from "@/api/endpoints";
import { queryKeys, useComments } from "@/api/queries";
import type { CommentDto } from "@/api/types/CommentDto";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Skeleton,
  Textarea,
} from "@/components/ui";

export function CommentsPanel({
  threadId,
  canWrite,
}: {
  threadId: string;
  canWrite: boolean;
}) {
  const queryClient = useQueryClient();
  const comments = useComments(threadId);
  const [body, setBody] = useState("");
  const refresh = () => {
    void queryClient.invalidateQueries({
      queryKey: queryKeys.comments(threadId),
    });
    void queryClient.invalidateQueries({
      queryKey: queryKeys.threadBundle(threadId),
    });
    void queryClient.invalidateQueries({
      queryKey: queryKeys.threadEvents(threadId),
    });
  };
  const create = useMutation({
    mutationFn: (send: boolean) =>
      createComment(threadId, {
        body: body.trim(),
        anchor: { kind: "thread" },
        parentId: null,
        send,
      }),
    onSuccess: () => {
      setBody("");
      refresh();
    },
    onError: (error) => toast.error(String(error)),
  });
  const send = useMutation({
    mutationFn: (comment: CommentDto) => sendComment(comment.id),
    onSuccess: refresh,
    onError: (error) => toast.error(String(error)),
  });
  const resolve = useMutation({
    mutationFn: (comment: CommentDto) =>
      updateComment(comment.id, { body: null, status: "resolved" }),
    onSuccess: refresh,
    onError: (error) => toast.error(String(error)),
  });
  const remove = useMutation({
    mutationFn: (comment: CommentDto) => deleteComment(comment.id),
    onSuccess: refresh,
    onError: (error) => toast.error(String(error)),
  });

  if (comments.isLoading) {
    return (
      <div className="space-y-2 p-3">
        <Skeleton className="h-14 w-full" />
        <Skeleton className="h-20 w-full" />
      </div>
    );
  }
  if (comments.isError) {
    return (
      <ErrorState
        error={comments.error}
        title="Cannot load comments"
        onRetry={() => void comments.refetch()}
        compact
      />
    );
  }
  const list = comments.data?.comments ?? [];

  return (
    <div className="flex min-h-0 flex-col">
      <div className="border-b border-border p-2.5">
        {canWrite ? (
          <div className="space-y-1.5">
            <Textarea
              value={body}
              onChange={(event) => setBody(event.target.value)}
              placeholder="Leave a comment for the thread…"
              rows={2}
              className="text-[11.5px]"
            />
            <div className="flex justify-end gap-1.5">
              <Button
                size="sm"
                variant="ghost"
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
                <Send aria-hidden />
                Send now
              </Button>
            </div>
          </div>
        ) : (
          <p className="text-[10.5px] text-faint">
            Comments are visible to thread members.
          </p>
        )}
      </div>
      {list.length === 0 ? (
        <EmptyState
          icon={MessageSquare}
          title="No comments yet"
          description="Thread comments are collected here. Pending comments are delivered with the next message."
          compact
        />
      ) : (
        <ul className="space-y-2 p-2.5">
          {list.map((comment) => (
            <li
              key={comment.id}
              className="rounded-lg border border-border bg-surface p-2.5"
            >
              <div className="flex items-center gap-1.5 text-[10.5px]">
                <span className="text-fg">{comment.author.displayName}</span>
                <span className="text-faint">@{comment.author.username}</span>
                <Badge size="sm" variant="neutral" className="ml-auto">
                  {comment.status}
                </Badge>
              </div>
              <p className="mt-1 whitespace-pre-wrap text-[11.5px] leading-relaxed text-fg">
                {comment.body}
              </p>
              {comment.anchorChanged ? (
                <p className="mt-1 text-[10px] text-warning">
                  The original diff position changed; this comment is anchored to
                  the file instead.
                </p>
              ) : null}
              <div className="mt-1.5 flex items-center gap-1.5 border-t border-border/70 pt-1.5">
                {comment.status !== "delivered" &&
                comment.status !== "resolved" &&
                canWrite ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    loading={send.isPending}
                    onClick={() => send.mutate(comment)}
                  >
                    Send now
                  </Button>
                ) : null}
                {comment.status !== "resolved" && canWrite ? (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => resolve.mutate(comment)}
                  >
                    Resolve
                  </Button>
                ) : null}
                <button
                  type="button"
                  className="ml-auto text-faint hover:text-danger"
                  aria-label="Delete comment"
                  onClick={() => remove.mutate(comment)}
                >
                  <Trash2 className="size-3" aria-hidden />
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
