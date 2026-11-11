/** Inbox entry, settings entry and the account menu. */
import {
  Inbox,
  LogOut,
  Settings,
  Users,
} from "lucide-react";
import {
  ActionIcon,
  Avatar,
  Badge,
  Indicator,
  Kbd,
  Menu,
  Skeleton,
  UnstyledButton,
} from "@mantine/core";
import type { UserDto } from "@/api/types/UserDto";
import { ThemeToggle } from "@/components/shell/ThemeToggle";
import { modKeyLabel } from "@/lib/platform";
import { rowHover } from "./rowStyles";

export interface SidebarFooterProps {
  inboxCount: number;
  user?: UserDto | null;
  userLoading?: boolean;
  onOpenInbox: () => void;
  onOpenSettings: () => void;
  onLogout: () => void;
}

export function SidebarFooter({
  inboxCount,
  user,
  userLoading,
  onOpenInbox,
  onOpenSettings,
  onLogout,
}: SidebarFooterProps) {
  return (
    <div
      style={{
        flexShrink: 0,
        borderTop: "1px solid var(--mantine-color-default-border)",
        padding: 8,
        paddingBottom: "calc(8px + env(safe-area-inset-bottom))",
      }}
    >
      <UnstyledButton
        type="button"
        onClick={onOpenInbox}
        h={{ base: 44, md: 40 }}
        style={{
          marginBottom: 4,
          display: "flex",
          width: "100%",
          alignItems: "center",
          gap: 12,
          borderRadius: "var(--mantine-radius-md)",
          padding: "0 8px",
          textAlign: "left",
          fontSize: "var(--mantine-font-size-sm)",
          color: "var(--mantine-color-dimmed)",
        }}
        styles={{ root: rowHover }}
      >
        <Inbox size={14} aria-hidden />
        <span style={{ flex: 1 }}>Inbox</span>
        {inboxCount > 0 ? (
          <Badge color="blue" variant="light" size="xs">
            {inboxCount > 99 ? "99+" : inboxCount}
          </Badge>
        ) : null}
        <Kbd visibleFrom="md">{modKeyLabel()}⇧J</Kbd>
      </UnstyledButton>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          borderRadius: "var(--mantine-radius-md)",
          padding: "4px 6px",
        }}
      >
        {userLoading ? (
          <Skeleton height={24} width={24} circle />
        ) : user ? (
          <Indicator
            inline
            position="bottom-end"
            size={8}
            color="green"
            withBorder
          >
            <Avatar
              src={user.avatarUrl ?? null}
              alt={user.displayName}
              name={user.displayName}
              color="initials"
              size={24}
              radius="xl"
            />
          </Indicator>
        ) : (
          <span
            style={{
              display: "flex",
              width: 24,
              height: 24,
              alignItems: "center",
              justifyContent: "center",
              borderRadius: "50%",
              backgroundColor: "var(--mantine-color-gray-1)",
              color: "var(--mantine-color-dimmed)",
            }}
          >
            <Users size={14} aria-hidden />
          </span>
        )}
        <div style={{ minWidth: 0, flex: 1 }}>
          <p
            style={{
              margin: 0,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              fontSize: "var(--mantine-font-size-md)",
              fontWeight: 500,
              color: "var(--mantine-color-text)",
            }}
          >
            {user?.displayName ?? (userLoading ? "Loading…" : "Signed out")}
          </p>
          <p
            style={{
              margin: 0,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              fontSize: "var(--mantine-font-size-xs)",
              color: "var(--mantine-color-dimmed)",
            }}
          >
            {user ? `@${user.username}` : "No active session"}
          </p>
        </div>
        <ThemeToggle size="md" hitArea="40" />
        <Menu position="top-end" width={200}>
          <Menu.Target>
            <ActionIcon
              variant="default"
              size="md"
              className="tb-hit-40"
              aria-label="Account menu"
            >
              <Settings size={14} />
            </ActionIcon>
          </Menu.Target>
          <Menu.Dropdown>
            <Menu.Label>
              {user ? `@${user.username}` : "Account"}
            </Menu.Label>
            <Menu.Divider />
            <Menu.Item leftSection={<Inbox size={14} />} onClick={onOpenInbox}>
              Inbox
            </Menu.Item>
            <Menu.Item
              leftSection={<Settings size={14} />}
              onClick={onOpenSettings}
            >
              Settings
            </Menu.Item>
            <Menu.Divider />
            <Menu.Item
              leftSection={<LogOut size={14} />}
              onClick={onLogout}
            >
              Reload workspace
            </Menu.Item>
          </Menu.Dropdown>
        </Menu>
      </div>
    </div>
  );
}
