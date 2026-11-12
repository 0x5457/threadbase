import { Box, Text } from "@mantine/core";
import type { DraftPendingBlock } from "../../lib/draft";

export function PendingBlocks({
  blocks,
  authorName,
  authorAccent,
}: {
  blocks: DraftPendingBlock[];
  authorName: (authorId: string) => string;
  authorAccent: (authorId: string) => string;
}) {
  if (!blocks.length) return null;

  return (
    <Box mb={6}>
      {blocks.slice(0, 2).map((block) => (
        <Box
          key={block.id}
          mb={3}
          pl={6}
          py={2}
          style={{
            minWidth: 0,
            borderRadius: 3,
            borderLeft: `2px solid ${authorAccent(block.author)}`,
            background: "var(--raised)",
          }}
        >
          <Text size="xs" fw={600} style={{ color: authorAccent(block.author) }}>
            {authorName(block.author)}
          </Text>
          <Text
            size="xs"
            c="dimmed"
            style={{
              display: "-webkit-box",
              WebkitLineClamp: 2,
              WebkitBoxOrient: "vertical",
              overflow: "hidden",
              whiteSpace: "pre-wrap",
              overflowWrap: "anywhere",
            }}
          >
            {block.text}
          </Text>
        </Box>
      ))}
      {blocks.length > 2 ? (
        <Text size="xs" c="dimmed">
          +{blocks.length - 2} more pending
        </Text>
      ) : null}
    </Box>
  );
}
