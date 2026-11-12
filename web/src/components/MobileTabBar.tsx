import { AppShell } from "@mantine/core";
import {
  IconFileDiff,
  IconFolders,
  IconInbox,
  IconSettings,
} from "@tabler/icons-react";

export type MobileTab = "inbox" | "thread" | "project" | "settings";

interface MobileTabBarProps {
  view: MobileTab;
  unreadCount: number;
  threadsOpened: boolean;
  changesOpened: boolean;
  changesEnabled: boolean;
  onInbox: () => void;
  onThreads: () => void;
  onChanges: () => void;
  onSettings: () => void;
}

/**
 * Thumb-reach primary navigation for phones. It replaces the status bar below
 * the phone breakpoint: the branch/agent/status metadata is secondary on a
 * phone and stays available in the Changes sheet, while Inbox and Changes are
 * the two destinations people reach for constantly.
 */
export function MobileTabBar({
  view,
  unreadCount,
  threadsOpened,
  changesOpened,
  changesEnabled,
  onInbox,
  onThreads,
  onChanges,
  onSettings,
}: MobileTabBarProps) {
  const inboxActive = view === "inbox";
  const threadsActive = threadsOpened || view === "thread" || view === "project";
  const settingsActive = view === "settings";

  return (
    <AppShell.Footer className="mobile-tabs">
      <button
        type="button"
        className={`mobile-tab${inboxActive ? " active" : ""}`}
        aria-label={
          unreadCount > 0 ? `Inbox, ${unreadCount} unread` : "Inbox"
        }
        aria-current={inboxActive ? "page" : undefined}
        onClick={onInbox}
      >
        <span className="mobile-tab-icon">
          <IconInbox size={20} stroke={1.6} />
          {unreadCount > 0 ? (
            <span className="mobile-tab-badge" aria-hidden="true">
              {unreadCount > 9 ? "9+" : unreadCount}
            </span>
          ) : null}
        </span>
        <span className="mobile-tab-label">Inbox</span>
      </button>

      <button
        type="button"
        className={`mobile-tab${threadsActive ? " active" : ""}`}
        aria-label="Threads and projects"
        aria-expanded={threadsOpened}
        onClick={onThreads}
      >
        <span className="mobile-tab-icon">
          <IconFolders size={20} stroke={1.6} />
        </span>
        <span className="mobile-tab-label">Threads</span>
      </button>

      <button
        type="button"
        className={`mobile-tab${changesOpened ? " active" : ""}`}
        aria-label="Changes"
        aria-disabled={!changesEnabled}
        disabled={!changesEnabled}
        onClick={onChanges}
      >
        <span className="mobile-tab-icon">
          <IconFileDiff size={20} stroke={1.6} />
        </span>
        <span className="mobile-tab-label">Changes</span>
      </button>

      <button
        type="button"
        className={`mobile-tab${settingsActive ? " active" : ""}`}
        aria-label="Settings"
        aria-current={settingsActive ? "page" : undefined}
        onClick={onSettings}
      >
        <span className="mobile-tab-icon">
          <IconSettings size={20} stroke={1.6} />
        </span>
        <span className="mobile-tab-label">Settings</span>
      </button>
    </AppShell.Footer>
  );
}
