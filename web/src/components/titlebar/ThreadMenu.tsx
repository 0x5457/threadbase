import { ActionIcon, Menu } from "@mantine/core";
import {
  IconArchive,
  IconArrowBackUp,
  IconDots,
  IconGitFork,
  IconGitMerge,
  IconListCheck,
  IconPencil,
  IconPin,
  IconShare,
} from "@tabler/icons-react";

export function ThreadMenu({
  canManage,
  isPinned,
  onShare,
  onMerge,
  onReviews,
  onRename,
  onTogglePin,
  onFork,
  onRevert,
  onArchive,
}: {
  canManage: boolean;
  isPinned: boolean;
  onShare: () => void;
  onMerge: () => void;
  onReviews: () => void;
  onRename: () => void;
  onTogglePin: () => void;
  onFork: () => void;
  onRevert: () => void;
  onArchive: () => void;
}) {
  if (!canManage) return null;
  return (
    <Menu position="bottom-end" width={190} shadow="md">
      <Menu.Target>
        <ActionIcon aria-label="Thread actions">
          <IconDots size={15} stroke={1.5} />
        </ActionIcon>
      </Menu.Target>
      <Menu.Dropdown>
        <Menu.Item leftSection={<IconShare size={13} />} onClick={onShare}>
          Share
        </Menu.Item>
        <Menu.Item leftSection={<IconGitMerge size={13} />} onClick={onMerge}>
          Merge
        </Menu.Item>
        <Menu.Item
          leftSection={<IconListCheck size={13} />}
          onClick={onReviews}
        >
          Reviews
        </Menu.Item>
        <Menu.Divider />
        <Menu.Item leftSection={<IconPencil size={13} />} onClick={onRename}>
          Rename
        </Menu.Item>
        <Menu.Item leftSection={<IconPin size={13} />} onClick={onTogglePin}>
          {isPinned ? "Unpin" : "Pin"}
        </Menu.Item>
        <Menu.Divider />
        <Menu.Item leftSection={<IconGitFork size={13} />} onClick={onFork}>
          Fork thread
        </Menu.Item>
        <Menu.Item
          leftSection={<IconArrowBackUp size={13} />}
          onClick={onRevert}
        >
          Revert changes
        </Menu.Item>
        <Menu.Divider />
        <Menu.Item
          color="red"
          leftSection={<IconArchive size={13} />}
          onClick={onArchive}
        >
          Archive thread
        </Menu.Item>
      </Menu.Dropdown>
    </Menu>
  );
}
