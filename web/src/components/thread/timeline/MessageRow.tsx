import { MessageSquare, MessageSquareText } from "lucide-react";
import { ActionIcon, Avatar, Box, Flex, Text } from "@mantine/core";
import type { CommentAnchor } from "@/api/types/CommentAnchor";
import { formatTimestamp, initials } from "@/lib/format";
import type { MessageItem } from "./types";

export interface MessageRowProps {
  message: MessageItem;
  onSelect?: () => void;
  onComment?: (anchor: CommentAnchor) => void;
}

/**
 * A user or agent message: 28px avatar gutter (shared with tool rows and
 * notices so every timeline row starts on the same text column), author line,
 * then the body block.
 */
export function MessageRow({ message, onSelect, onComment }: MessageRowProps) {
  const author = message.author;
  const person = author
    ? {
        id: author.id,
        displayName: author.displayName,
        avatarUrl: author.avatarUrl,
        color: author.color,
      }
    : { id: "agent", displayName: "Agent" };

  return (
    <Box>
      {message.seq ? <span id={`event-${message.seq}`} aria-hidden /> : null}
      <Flex
        gap="md"
        id={`message-${message.messageId}`}
        data-message-id={message.messageId}
        onClick={onSelect}
      >
        <Flex w={28} justify="center" pt={2}>
          <Avatar
            src={person.avatarUrl ?? null}
            alt={person.displayName}
            name={person.displayName}
            color={person.color ?? "initials"}
            size={24}
            radius="xl"
          >
            {initials(person.displayName)}
          </Avatar>
        </Flex>
        <Box flex={1} miw={0}>
          <Flex align="center" gap="xs" mb="xs">
            <Text size="sm" fw={500} c="dimmed">
              {author?.displayName ??
                (message.role === "agent" ? "Agent" : "User")}
            </Text>
            {message.authors.length > 1 ? (
              <Text size="sm" c="dimmed">
                +{message.authors.length - 1}
              </Text>
            ) : null}
            {message.createdAt ? (
              <Text
                size="sm"
                c="dimmed"
                title={formatTimestamp(message.createdAt)}
              >
                {formatTimestamp(message.createdAt)}
              </Text>
            ) : null}
            <Flex align="center" gap={16} ml="auto">
              <ActionIcon
                variant="subtle"
                color="gray"
                size={24}
                className="tb-hit-40"
                aria-label="View comments"
                onClick={() =>
                  window.dispatchEvent(
                    new CustomEvent("threadbase:open-comments"),
                  )
                }
              >
                <MessageSquareText size={12} aria-hidden />
              </ActionIcon>
              {onComment ? (
                <ActionIcon
                  variant="subtle"
                  color="gray"
                  size={24}
                  className="tb-hit-40"
                  aria-label="Comment on message"
                  onClick={() =>
                    onComment({
                      kind: "message",
                      messageId: message.messageId,
                    })
                  }
                >
                  <MessageSquare size={12} aria-hidden />
                </ActionIcon>
              ) : null}
            </Flex>
          </Flex>
          <Box
            px="sm"
            py="xs"
            style={{
              whiteSpace: "pre-wrap",
              overflowWrap: "break-word",
              fontSize: "var(--mantine-font-size-md)",
              lineHeight: 1.6,
              borderRadius: "var(--mantine-radius-md)",
              ...(message.role === "user"
                ? { background: "var(--mantine-color-default-hover)" }
                : {}),
            }}
          >
            {message.text}
          </Box>
        </Box>
      </Flex>
    </Box>
  );
}
