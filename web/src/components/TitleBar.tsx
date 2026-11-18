import {
  ActionIcon,
  AppShell,
  Badge,
  Box,
  Burger,
  Text,
  Tooltip,
  useMantineColorScheme,
} from "@mantine/core";
import {
  IconFolder,
  IconGitBranch,
  IconInbox,
  IconLayoutSidebarLeftCollapse,
  IconLayoutSidebarLeftExpand,
  IconLayoutSidebarRightCollapse,
  IconLayoutSidebarRightExpand,
  IconLayoutList,
  IconMoon,
  IconSearch,
  IconSun,
  IconTerminal2,
} from "@tabler/icons-react";
import { useQuery } from "@tanstack/react-query";
import { getMe, type ThreadSummary } from "../api";
import { Participants } from "./titlebar/Participants";
import { ThreadMenu } from "./titlebar/ThreadMenu";
import { TitleActions } from "./titlebar/TitleActions";

interface TitleBarProps {
  title: string;
  /** A live agent turn is in progress for the open thread. */
  running?: boolean;
  participants: ThreadSummary["participants"];
  /** Authoritative online author ids from the live presence socket. */
  onlineUserIds?: string[];
  /** Unread inbox items; the red dot is rendered only when this is non-zero. */
  unreadCount: number;
  canOpenChanges: boolean;
  /** Thread has its own git worktree; the badge and merge actions need it. */
  hasWorktree: boolean;
  /** A thread is open; the worktree/project-tree badge is only meaningful then. */
  threadOpen: boolean;
  /** The project behind this thread has a setup command to (re)run. */
  canRunSetup: boolean;
  branch: string;
  baseBranch: string;
  /** Desktop changes panel is hidden; the toggle reveals it again. */
  asideCollapsed: boolean;
  onToggleAside: () => void;
  navOpened: boolean;
  navCollapsed: boolean;
  onToggleNav: () => void;
  /** The browser terminal exists on this server and this account may drive it. */
  canTerminal?: boolean;
  terminalOpen?: boolean;
  onToggleTerminal?: () => void;
  /** The Sessions surface is available; its entry point is shown. */
  canSessions?: boolean;
  sessionsActive?: boolean;
  onOpenSessions?: () => void;
  onToggleNavCollapsed: () => void;
  onOpenPalette: () => void;
  onOpenInbox: () => void;
  onOpenChanges: () => void;
  onOpenHistory: () => void;
  onReviews: () => void;
  onMerge: () => void;
  canManage: boolean;
  isPinned: boolean;
  /** Thread-machine commands legal for the current state. */
  allowedActions?: string[];
  onRename: () => void;
  onTogglePin: () => void;
  onFork: () => void;
  onRevert: () => void;
  onArchive: (archived: boolean) => void;
  /** Move the thread between shelves; ids come from the placement machine. */
  onPlacement: (action: string) => void;
}

export function TitleBar({
  title,
  running = false,
  participants,
  onlineUserIds,
  unreadCount,
  canOpenChanges,
  hasWorktree,
  threadOpen,
  canRunSetup,
  branch,
  baseBranch,
  asideCollapsed,
  onToggleAside,
  navOpened,
  navCollapsed,
  onToggleNav,
  onToggleNavCollapsed,
  canTerminal = false,
  terminalOpen = false,
  onToggleTerminal,
  canSessions = false,
  sessionsActive = false,
  onOpenSessions,
  onOpenPalette,
  onOpenInbox,
  onOpenChanges,
  onOpenHistory,
  onReviews,
  onMerge,
  canManage,
  isPinned,
  allowedActions,
  onRename,
  onTogglePin,
  onFork,
  onRevert,
  onArchive,
  onPlacement,
}: TitleBarProps) {
  const { colorScheme, toggleColorScheme } = useMantineColorScheme();
  const dark = colorScheme === "dark";
  const me = useQuery({
    queryKey: ["me"],
    queryFn: getMe,
    staleTime: 60_000,
  });
  const readOnly = me.data?.user.access === "read_only";

  return (
    <AppShell.Header className="header">
      <div className="header-left">
        <span className="window-lights" aria-hidden="true" title="Window">
          <span />
          <span />
          <span />
        </span>
        <Box className="burger-slot" hiddenFrom="sm">
          <Burger
            opened={navOpened}
            onClick={onToggleNav}
            size="sm"
            aria-label={
              unreadCount > 0
                ? `Toggle navigation, ${unreadCount} unread`
                : "Toggle navigation"
            }
          />
          {unreadCount > 0 ? <span className="icon-dot burger-dot" /> : null}
        </Box>
        <Tooltip
          label={navCollapsed ? "Show navigation" : "Hide navigation"}
        >
          <ActionIcon
            aria-label={navCollapsed ? "Show navigation" : "Hide navigation"}
            visibleFrom="sm"
            hiddenFrom="lg"
            onClick={onToggleNavCollapsed}
          >
            {navCollapsed ? (
              <IconLayoutSidebarLeftExpand size={15} stroke={1.5} />
            ) : (
              <IconLayoutSidebarLeftCollapse size={15} stroke={1.5} />
            )}
          </ActionIcon>
        </Tooltip>
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
        {running ? (
          <span
            className="dot running header-running"
            role="status"
            aria-label="Agent is running"
            title="Agent is running"
          />
        ) : null}
        <Text size="md" fw={500} className="doc-title">
          {title}
        </Text>
        {readOnly ? (
          <Badge variant="light" color="gray" style={{ textTransform: "none" }}>
            Read-only
          </Badge>
        ) : null}
        {hasWorktree ? (
          <Tooltip
            label={`Isolated worktree · ${branch} (base ${baseBranch})`}
          >
            <Badge
              variant="light"
              color="gray"
              className="scope-badge"
              leftSection={<IconGitBranch size={9} stroke={1.5} />}
              style={{ textTransform: "none" }}
            >
              <span className="scope-badge-text">worktree</span>
            </Badge>
          </Tooltip>
        ) : threadOpen ? (
          <Tooltip label="Runs directly in the project working tree; changes are live">
            <Badge
              variant="light"
              color="gray"
              className="scope-badge"
              leftSection={<IconFolder size={9} stroke={1.5} />}
              style={{ textTransform: "none" }}
            >
              <span className="scope-badge-text">project tree</span>
            </Badge>
          </Tooltip>
        ) : null}
        <Box visibleFrom="sm">
          <Participants
            participants={participants}
            onlineUserIds={onlineUserIds}
          />
        </Box>
        <TitleActions
          visible={canManage && !navOpened}
          canManage={canManage}
          hasWorktree={hasWorktree}
          canRunSetup={canRunSetup}
          isPinned={isPinned}
          allowedActions={allowedActions ?? []}
          onReviews={onReviews}
          onMerge={onMerge}
          onRename={onRename}
          onTogglePin={onTogglePin}
          onFork={onFork}
          onRevert={onRevert}
          onArchive={onArchive}
          onPlacement={onPlacement}
          onHistory={onOpenHistory}
        />
      </div>

      <div className="header-right">
        {canOpenChanges && !navOpened ? (
          <>
            <Tooltip
              label={asideCollapsed ? "Show changes  ⌘⇧B" : "Hide changes  ⌘⇧B"}
            >
              <ActionIcon
                aria-label={asideCollapsed ? "Show changes" : "Hide changes"}
                visibleFrom="lg"
                onClick={onToggleAside}
              >
                {asideCollapsed ? (
                  <IconLayoutSidebarRightExpand size={16} stroke={1.5} />
                ) : (
                  <IconLayoutSidebarRightCollapse size={16} stroke={1.5} />
                )}
              </ActionIcon>
            </Tooltip>
            <Tooltip label="Changes">
              <ActionIcon
                aria-label="Open changes"
                hiddenFrom="lg"
                onClick={onOpenChanges}
              >
                <IconLayoutSidebarRightExpand size={16} stroke={1.5} />
              </ActionIcon>
            </Tooltip>
          </>
        ) : null}
        {!navOpened ? (
          <Box hiddenFrom="md">
            <ThreadMenu
              canManage={canManage}
              hasWorktree={hasWorktree}
              canRunSetup={canRunSetup}
              isPinned={isPinned}
              allowedActions={allowedActions ?? []}
                  onMerge={onMerge}
              onReviews={onReviews}
              onRename={onRename}
              onTogglePin={onTogglePin}
              onFork={onFork}
              onRevert={onRevert}
              onArchive={onArchive}
              onPlacement={onPlacement}
              onHistory={onOpenHistory}
            />
          </Box>
        ) : null}
        {canTerminal && onToggleTerminal ? (
          <Tooltip label="Terminal (Ctrl+`)">
            <ActionIcon
              aria-label="Terminal"
              aria-pressed={terminalOpen}
              className="header-terminal"
              data-active={terminalOpen ? "true" : undefined}
              onClick={onToggleTerminal}
            >
              <IconTerminal2 size={15} stroke={1.5} />
            </ActionIcon>
          </Tooltip>
        ) : null}
        {canSessions && onOpenSessions ? (
          <Tooltip label="Sessions">
            <ActionIcon
              aria-label="Sessions"
              aria-pressed={sessionsActive}
              className="header-terminal"
              data-active={sessionsActive ? "true" : undefined}
              onClick={onOpenSessions}
            >
              <IconLayoutList size={15} stroke={1.5} />
            </ActionIcon>
          </Tooltip>
        ) : null}
        <Tooltip label={dark ? "Light mode" : "Dark mode"}>
          <ActionIcon
            aria-label="Toggle theme"
            visibleFrom="sm"
            onClick={toggleColorScheme}
          >
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
