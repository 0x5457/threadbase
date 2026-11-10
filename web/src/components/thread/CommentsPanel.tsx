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
  const [replyTo, setReplyTo] = useState<string | null>(null);
  const [replyBody, setReplyBody] = useState("");
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
  const reply = useMutation({
    mutationFn: (parent: CommentDto) =>
      createComment(threadId, {
        body: replyBody.trim(),
        anchor: parent.anchor,
        parentId: parent.id,
        send: false,
      }),
    onSuccess: () => {
      setReplyBody("");
      setReplyTo(null);
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
  const topLevel = list.filter((comment) => !comment.parentId);
  const repliesFor = (id: string) =>
    list.filter((comment) => comment.parentId === id);

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
              className="text-[13px]"
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
          <p className="text-[13px] text-faint">
            Comments are visible to thread members.
          </p>
        )}
      </div>
      {topLevel.length === 0 ? (
        <EmptyState
          icon={MessageSquare}
          title="No comments yet"
          compact
        />
      ) : (
        <ul className="space-y-2 p-2.5">
          {topLevel.map((comment) => (
            <li
              key={comment.id}
              className="rounded-lg border border-border bg-surface p-2.5"
            >
              <div className="flex items-center gap-1.5 text-[13px]">
                <span className="text-fg">{comment.author.displayName}</span>
                <span className="text-faint">@{comment.author.username}</span>
                <Badge size="sm" variant="neutral" className="ml-auto">
                  {comment.status}
                </Badge>
              </div>
              <p className="mt-1 whitespace-pre-wrap text-[13px] leading-relaxed text-fg">
                {comment.body}
              </p>
              <div className="mt-1 flex flex-wrap gap-2 text-[9.5px]">
                {comment.anchor.kind === "message" ? (
                  <a
                    href={`#message-${comment.anchor.messageId}`}
                    className="text-primary hover:underline"
                  >
                    jump to message
                  </a>
                ) : null}
                {comment.deliveredSeq ? (
                  <a
                    href={`#event-${comment.deliveredSeq}`}
                    className="text-primary hover:underline"
                  >
                    delivered at #{String(comment.deliveredSeq)}
                  </a>
                ) : null}
              </div>
              {comment.anchorChanged ? (
                <p className="mt-1 text-[13px] text-warning">
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
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setReplyTo(comment.id)}
                >
                  Reply
                </Button>
                <button
                  type="button"
                  className="ml-auto text-faint hover:text-danger"
                  aria-label="Delete comment"
                  onClick={() => remove.mutate(comment)}
                >
                  <Trash2 className="size-3" aria-hidden />
                </button>
              </div>
              {repliesFor(comment.id).map((replyComment) => (
                <div
                  key={replyComment.id}
                  className="mt-1.5 border-l-2 border-border pl-2 text-[13px]"
                >
                  <p className="text-faint">
                    {replyComment.author.displayName} · {replyComment.status}
                  </p>
                  <p className="whitespace-pre-wrap text-fg">{replyComment.body}</p>
                </div>
              ))}
              {replyTo === comment.id ? (
                <div className="mt-1.5 space-y-1">
                  <Textarea
                    value={replyBody}
                    onChange={(event) => setReplyBody(event.target.value)}
                    placeholder="Reply to this comment…"
                    rows={2}
                    className="text-[13px]"
                  />
                  <div className="flex justify-end gap-1">
                    <Button size="sm" variant="ghost" onClick={() => setReplyTo(null)}>
                      Cancel
                    </Button>
                    <Button
                      size="sm"
                      variant="primary"
                      disabled={!replyBody.trim()}
                      loading={reply.isPending}
                      onClick={() => reply.mutate(comment)}
                    >
                      Reply
                    </Button>
                  </div>
                </div>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
