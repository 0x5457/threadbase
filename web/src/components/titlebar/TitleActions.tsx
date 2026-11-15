import { Button, Group } from "@mantine/core";
import { IconGitMerge, IconListCheck } from "@tabler/icons-react";
import { ThreadMenu } from "./ThreadMenu";

interface TitleActionsProps {
  visible: boolean;
  canManage: boolean;
  hasWorktree: boolean;
  canRunSetup: boolean;
  isPinned: boolean;
  /** Thread-machine commands legal for the current state. */
  allowedActions: string[];
  onReviews: () => void;
  onMerge: () => void;
  onRename: () => void;
  onTogglePin: () => void;
  onFork: () => void;
  onRevert: () => void;
  onArchive: (archived: boolean) => void;
}

export function TitleActions({
  visible,
  canManage,
  hasWorktree,
  canRunSetup,
  isPinned,
  allowedActions,
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
      {hasWorktree ? (
        <>
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
            Apply changes
          </Button>
        </>
      ) : null}
      <ThreadMenu
        canManage={canManage}
        hasWorktree={hasWorktree}
        canRunSetup={canRunSetup}
        inlineActions
        isPinned={isPinned}
        allowedActions={allowedActions}
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
