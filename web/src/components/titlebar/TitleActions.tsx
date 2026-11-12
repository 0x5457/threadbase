import { Button, Group } from "@mantine/core";
import { IconGitMerge, IconListCheck } from "@tabler/icons-react";
import { ThreadMenu } from "./ThreadMenu";

interface TitleActionsProps {
  visible: boolean;
  canManage: boolean;
  isPinned: boolean;
  onReviews: () => void;
  onMerge: () => void;
  onRename: () => void;
  onTogglePin: () => void;
  onFork: () => void;
  onRevert: () => void;
  onArchive: () => void;
}

export function TitleActions({
  visible,
  canManage,
  isPinned,
  onReviews,
  onMerge,
  onRename,
  onTogglePin,
  onFork,
  onRevert,
  onArchive,
}: TitleActionsProps) {
  return (
    <Group
      gap={2}
      ml="auto"
      wrap="nowrap"
      visibleFrom="md"
      display={visible ? undefined : "none"}
    >
      <Button
        variant="subtle"
        size="compact-xs"
        color="gray"
        leftSection={<IconListCheck size={13} stroke={1.5} />}
        onClick={onReviews}
      >
        Reviews
      </Button>
      <Button
        variant="subtle"
        size="compact-xs"
        color="gray"
        leftSection={<IconGitMerge size={13} stroke={1.5} />}
        onClick={onMerge}
      >
        Merge
      </Button>
      <ThreadMenu
        canManage={canManage}
        isPinned={isPinned}
        onMerge={onMerge}
        onReviews={onReviews}
        onRename={onRename}
        onTogglePin={onTogglePin}
        onFork={onFork}
        onRevert={onRevert}
        onArchive={onArchive}
      />
    </Group>
  );
}
