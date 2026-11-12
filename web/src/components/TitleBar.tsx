import {
  ActionIcon,
  AppShell,
  Box,
  Burger,
  Text,
  Tooltip,
  useMantineColorScheme,
} from "@mantine/core";
import {
  IconInbox,
  IconLayoutSidebarRightExpand,
  IconMoon,
  IconSearch,
  IconSun,
} from "@tabler/icons-react";
import type { ThreadSummary } from "../api";
import { Participants } from "./titlebar/Participants";
import { ThreadMenu } from "./titlebar/ThreadMenu";
import { TitleActions } from "./titlebar/TitleActions";

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
        <Participants
          participants={participants}
          onlineUserIds={onlineUserIds}
        />
        <TitleActions
          visible={canOpenChanges}
          canManage={canManage}
          isPinned={isPinned}
          onReviews={onReviews}
          onMerge={onMerge}
          onShare={onShare}
          onRename={onRename}
          onTogglePin={onTogglePin}
          onFork={onFork}
          onRevert={onRevert}
          onArchive={onArchive}
        />
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
