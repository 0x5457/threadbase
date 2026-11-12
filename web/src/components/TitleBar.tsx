import {
  ActionIcon,
  AppShell,
  Avatar,
  Burger,
  Box,
  Button,
  Group,
  Indicator,
  Menu,
  Text,
  Tooltip,
  useMantineColorScheme,
} from "@mantine/core";
import {
  IconArchive,
  IconArrowBackUp,
  IconDots,
  IconGitFork,
  IconGitMerge,
  IconInbox,
  IconLayoutSidebarRightExpand,
  IconListCheck,
  IconMoon,
  IconPencil,
  IconPin,
  IconSearch,
  IconShare,
  IconSun,
} from "@tabler/icons-react";
import type { ThreadSummary } from "../api";

interface TitleBarProps {
  title: string;
  participants: ThreadSummary["participants"];
  /** Authoritative online author ids from the live presence socket. */
  onlineUserIds?: string[];
  /** Unread inbox items; the red dot is rendered only when this is non-zero. */
  unreadCount: number;
  canOpenChanges: boolean;
  navOpened: boolean;
  onToggleNav: () => void;
  onOpenPalette: () => void;
  onOpenInbox: () => void;
  onOpenChanges: () => void;
  onReviews: () => void;
  onMerge: () => void;
  onShare: () => void;
  canManage: boolean;
  isPinned: boolean;
  onRename: () => void;
  onTogglePin: () => void;
  onFork: () => void;
  onRevert: () => void;
  onArchive: () => void;
}

function ThreadMenu({
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

function initials(name: string) {
  return name
    .split(/\s+/)
    .map((part) => part[0])
    .slice(0, 2)
    .join("")
    .toUpperCase();
}

export function TitleBar({
  title,
  participants,
  onlineUserIds,
  unreadCount,
  canOpenChanges,
  navOpened,
  onToggleNav,
  onOpenPalette,
  onOpenInbox,
  onOpenChanges,
  onReviews,
  onMerge,
  onShare,
  canManage,
  isPinned,
  onRename,
  onTogglePin,
  onFork,
  onRevert,
  onArchive,
}: TitleBarProps) {
  const { colorScheme, toggleColorScheme } = useMantineColorScheme();
  const dark = colorScheme === "dark";

  return (
    <AppShell.Header className="header">
      <div className="header-left">
        <Burger
          opened={navOpened}
          onClick={onToggleNav}
          size="sm"
          hiddenFrom="sm"
          aria-label="Toggle navigation"
        />
        <Tooltip label="Search  ⌘K">
          <ActionIcon aria-label="Search" onClick={onOpenPalette}>
            <IconSearch size={15} stroke={1.5} />
          </ActionIcon>
        </Tooltip>
        <Tooltip
          label={unreadCount > 0 ? `Inbox — ${unreadCount} unread` : "Inbox  ⌘J"}
        >
          <ActionIcon
            aria-label="Inbox"
            className="inbox-button"
            visibleFrom="sm"
            onClick={onOpenInbox}
          >
            <IconInbox size={15} stroke={1.5} />
            {unreadCount > 0 ? <span className="icon-dot" /> : null}
          </ActionIcon>
        </Tooltip>
      </div>

      <div className="header-center">
        <Text size="md" fw={500} className="doc-title">
          {title}
        </Text>
        {participants.length ? (
          <Avatar.Group spacing={-4} visibleFrom="xs">
            {participants.slice(0, 3).map((participant, index) => (
              <Indicator
                key={participant.user.id}
                color="green"
                size={5}
                offset={1}
                position="bottom-end"
                disabled={
                  onlineUserIds
                    ? !onlineUserIds.includes(participant.user.id)
                    : !participant.online
                }
                withBorder={false}
              >
                <Avatar
                  size={18}
                  radius="xl"
                  color={index === 0 ? "brand" : "blue"}
                  title={participant.user.displayName}
                  src={participant.user.avatarUrl ?? undefined}
                >
                  {initials(participant.user.displayName)}
                </Avatar>
              </Indicator>
            ))}
          </Avatar.Group>
        ) : null}
        <Group
          gap={2}
          ml="auto"
          wrap="nowrap"
          visibleFrom="md"
          display={canOpenChanges ? undefined : "none"}
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
          <Button
            variant="subtle"
            size="compact-xs"
            color="gray"
            leftSection={<IconShare size={13} stroke={1.5} />}
            onClick={onShare}
          >
            Share
          </Button>
          <ThreadMenu
            canManage={canManage}
            isPinned={isPinned}
            onShare={onShare}
            onMerge={onMerge}
            onReviews={onReviews}
            onRename={onRename}
            onTogglePin={onTogglePin}
            onFork={onFork}
            onRevert={onRevert}
            onArchive={onArchive}
          />
        </Group>
      </div>

      <div className="header-right">
        {canOpenChanges ? (
          <Tooltip label="Changes">
            <ActionIcon
              aria-label="Open changes"
              hiddenFrom="lg"
              onClick={onOpenChanges}
            >
              <IconLayoutSidebarRightExpand size={16} stroke={1.5} />
            </ActionIcon>
          </Tooltip>
        ) : null}
        <Box hiddenFrom="md">
          <ThreadMenu
            canManage={canManage}
            isPinned={isPinned}
            onShare={onShare}
            onMerge={onMerge}
            onReviews={onReviews}
            onRename={onRename}
            onTogglePin={onTogglePin}
            onFork={onFork}
            onRevert={onRevert}
            onArchive={onArchive}
          />
        </Box>
        <Tooltip label={dark ? "Light mode" : "Dark mode"}>
          <ActionIcon aria-label="Toggle theme" onClick={toggleColorScheme}>
            {dark ? (
              <IconSun size={15} stroke={1.5} />
            ) : (
              <IconMoon size={15} stroke={1.5} />
            )}
          </ActionIcon>
        </Tooltip>
      </div>
    </AppShell.Header>
  );
}
