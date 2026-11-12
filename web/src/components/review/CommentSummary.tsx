import { Avatar, Card, Group, Text } from "@mantine/core";
import { IconMessageCircle } from "@tabler/icons-react";
import type { Comment } from "../../api";
import { initials } from "../../lib/initials";
import { SectionHead } from "./SectionHead";

const COMMENT_LIMIT = 6;

export function CommentSummary({ comments }: { comments: Comment[] }) {
  const visible = comments.slice(-COMMENT_LIMIT);
  return (
    <div>
      <SectionHead
        icon={<IconMessageCircle size={13} stroke={1.5} />}
        label="Comments"
        count={comments.length}
        trailing={
          comments.length > visible.length ? (
            <Text size="xs" c="dimmed">
              latest {visible.length} of {comments.length}
            </Text>
          ) : null
        }
      />
      {visible.length === 0 ? (
        <Text size="sm" c="dimmed" px={10} py="sm">
          No comments yet
        </Text>
      ) : (
        <div>
          {visible.map((comment) => (
            <div key={comment.id} className="comment-row">
              <Card withBorder p={6} className="comment-card">
                <Group gap={6} align="flex-start" wrap="nowrap">
                  <Avatar
                    size={14}
                    radius="xl"
                    color="brand"
                    src={comment.author.avatarUrl ?? undefined}
                  >
                    {initials(comment.author.displayName)}
                  </Avatar>
                  <div className="comment-text">
                    <Text size="sm" fw={500} component="span">
                      {comment.author.displayName}
                    </Text>
                    <Text
                      size="sm"
                      mt={2}
                      style={{ whiteSpace: "pre-wrap" }}
                    >
                      {comment.body}
                    </Text>
                  </div>
                </Group>
              </Card>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
