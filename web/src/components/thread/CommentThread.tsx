import { UnstyledButton } from "@mantine/core";
import { IconChevronDown } from "@tabler/icons-react";
import type { ReactNode } from "react";
import type { Comment } from "../../api";

/**
 * Delta-style comment thread: one inline `N comments` trigger under the block,
 * cards only when expanded. Keeping the cards unmounted also keeps a long
 * transcript cheap.
 */
export function CommentThread({
  comments,
  threadKey,
  expanded,
  onToggle,
  renderCard,
  editing,
  editor,
}: {
  comments: Comment[];
  threadKey: string;
  expanded: boolean;
  onToggle: (key: string) => void;
  renderCard: (comment: Comment) => ReactNode;
  editing: boolean;
  editor: ReactNode;
}) {
  if (comments.length === 0 && !editing) return null;
  const count = comments.length;
  const label =
    count === 0
      ? "Add a comment"
      : count === 1
        ? "1 comment"
        : `${count} comments`;

  return (
    <div className="comment-thread">
      <UnstyledButton
        className="comment-thread-trigger"
        aria-expanded={expanded || editing}
        onClick={() => onToggle(threadKey)}
      >
        <IconChevronDown size={11} stroke={1.5} />
        {label}
      </UnstyledButton>
      {expanded || editing ? (
        <div className="comment-thread-body">
          {comments.map(renderCard)}
          {editing ? editor : null}
        </div>
      ) : null}
    </div>
  );
}
