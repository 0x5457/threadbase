import { useMutation, useQueryClient } from "@tanstack/react-query";
import { MessageSquareText, Send, Trash2 } from "lucide-react";
import { useState } from "react";
import { notifications } from "@mantine/notifications";
import {
  ActionIcon,
  Anchor,
  Badge,
  Box,
  Button,
  Divider,
  Flex,
  Paper,
  Skeleton,
  Stack,
  Text,
  Textarea,
} from "@mantine/core";
import {
  createComment,
  deleteComment,
  sendComment,
  updateComment,
} from "@/api/endpoints";
import { queryKeys, useComments } from "@/api/queries";
import type { CommentDto } from "@/api/types/CommentDto";
import { EmptyState, ErrorState } from "@/components/app";

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
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
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
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
  });
  const send = useMutation({
    mutationFn: (comment: CommentDto) => sendComment(comment.id),
    onSuccess: refresh,
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
  });
  const resolve = useMutation({
    mutationFn: (comment: CommentDto) =>
      updateComment(comment.id, { body: null, status: "resolved" }),
    onSuccess: refresh,
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
  });
  const remove = useMutation({
    mutationFn: (comment: CommentDto) => deleteComment(comment.id),
    onSuccess: refresh,
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
  });

  if (comments.isLoading) {
    return (
      <Stack gap="sm" p="sm">
        <Skeleton height={56} />
        <Skeleton height={80} />
      </Stack>
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
    <Stack gap={0} style={{ minHeight: 0 }}>
      <Box p="sm">
        {canWrite ? (
          <Stack gap="xs">
            <Textarea
              value={body}
              onChange={(event) => setBody(event.target.value)}
              placeholder="Leave a comment for the thread…"
              rows={2}
            />
            <Flex justify="flex-end" gap="xs">
              <Button
                variant="subtle"
                color="dark"
                size="xs"
                disabled={!body.trim()}
                loading={create.isPending}
                onClick={() => create.mutate(false)}
              >
                Save pending
              </Button>
              <Button
                size="xs"
                disabled={!body.trim()}
                loading={create.isPending}
                onClick={() => create.mutate(true)}
              >
                <Send size={12} aria-hidden />
                Send now
              </Button>
            </Flex>
          </Stack>
        ) : (
          <Text size="xs" c="dimmed">
            Comments are visible to thread members.
          </Text>
        )}
      </Box>
      <Divider />
      {topLevel.length === 0 ? (
        <EmptyState
          icon={MessageSquareText}
          title="No comments yet"
          compact
        />
      ) : (
        <Stack gap="sm" p="sm">
          {topLevel.map((comment) => (
            <Paper key={comment.id} withBorder p="sm" radius="md">
              <Stack gap={4}>
                <Flex align="center" gap="xs" wrap="wrap">
                  <Text size="sm" fw={500}>
                    {comment.author.displayName}
                  </Text>
                  <Text size="sm" c="dimmed">
                    @{comment.author.username}
                  </Text>
                  <Badge
                    variant="light"
                    color="gray"
                    size="xs"
                    style={{ marginLeft: "auto" }}
                  >
                    {comment.status}
                  </Badge>
                </Flex>
                <Text
                  size="sm"
                  style={{ whiteSpace: "pre-wrap", lineHeight: 1.6 }}
                >
                  {comment.body}
                </Text>
                <Flex gap="md">
                  {comment.anchor.kind === "message" ? (
                    <Anchor
                      href={`#message-${comment.anchor.messageId}`}
                      size="xs"
                      c="blue.6"
                    >
                      jump to message
                    </Anchor>
                  ) : null}
                  {comment.deliveredSeq ? (
                    <Anchor
                      href={`#event-${comment.deliveredSeq}`}
                      size="xs"
                      c="blue.6"
                    >
                      delivered at #{String(comment.deliveredSeq)}
                    </Anchor>
                  ) : null}
                </Flex>
                {comment.anchorChanged ? (
                  <Text size="xs" c="yellow.7">
                    The original diff position changed; this comment is anchored
                    to the file instead.
                  </Text>
                ) : null}
                <Flex
                  align="center"
                  gap="xs"
                  mt="xs"
                  pt="xs"
                  style={{
                    borderTop:
                      "1px solid var(--mantine-color-default-border)",
                  }}
                >
                  {comment.status !== "delivered" &&
                  comment.status !== "resolved" &&
                  canWrite ? (
                    <Button
                      variant="subtle"
                      color="dark"
                      size="xs"
                      loading={send.isPending}
                      onClick={() => send.mutate(comment)}
                    >
                      Send now
                    </Button>
                  ) : null}
                  {comment.status !== "resolved" && canWrite ? (
                    <Button
                      variant="subtle"
                      color="dark"
                      size="xs"
                      onClick={() => resolve.mutate(comment)}
                    >
                      Resolve
                    </Button>
                  ) : null}
                  <Button
                    variant="subtle"
                    color="dark"
                    size="xs"
                    onClick={() => setReplyTo(comment.id)}
                  >
                    Reply
                  </Button>
                  <ActionIcon
                    variant="subtle"
                    color="gray"
                    size="xs"
                    aria-label="Delete comment"
                    style={{ marginLeft: "auto" }}
                    onClick={() => remove.mutate(comment)}
                  >
                    <Trash2 size={12} aria-hidden />
                  </ActionIcon>
                </Flex>
              </Stack>
              {repliesFor(comment.id).map((replyComment) => (
                <Box
                  key={replyComment.id}
                  pl="sm"
                  mt="xs"
                  style={{
                    borderLeft:
                      "2px solid var(--mantine-color-default-border)",
                  }}
                >
                  <Text size="xs" c="dimmed">
                    {replyComment.author.displayName} · {replyComment.status}
                  </Text>
                  <Text size="sm" style={{ whiteSpace: "pre-wrap" }}>
                    {replyComment.body}
                  </Text>
                </Box>
              ))}
              {replyTo === comment.id ? (
                <Stack gap="xs" mt="xs">
                  <Textarea
                    value={replyBody}
                    onChange={(event) => setReplyBody(event.target.value)}
                    placeholder="Reply to this comment…"
                    rows={2}
                  />
                  <Flex justify="flex-end" gap="xs">
                    <Button
                      variant="subtle"
                      color="dark"
                      size="xs"
                      onClick={() => setReplyTo(null)}
                    >
                      Cancel
                    </Button>
                    <Button
                      size="xs"
                      disabled={!replyBody.trim()}
                      loading={reply.isPending}
                      onClick={() => reply.mutate(comment)}
                    >
                      Reply
                    </Button>
                  </Flex>
                </Stack>
              ) : null}
            </Paper>
          ))}
        </Stack>
      )}
    </Stack>
  );
}
