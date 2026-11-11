import {
  ActionIcon,
  Box,
  Button,
  Indicator,
  Kbd,
  Text,
  Tooltip,
} from "@mantine/core";
import {
  PanelLeft,
  PanelRight,
  Search,
  Share2,
  ShieldCheck,
} from "lucide-react";
import type { Participant } from "@/api/types/Participant";
import { AvatarStack } from "@/components/app";
import { modKeyCombo } from "@/lib/platform";

export interface TitleBarProps {
  title: string;
  subtitle?: string;
  participants?: Participant[];
  approvalsCount?: number;
  drawerOpen: boolean;
  sidebarOpen: boolean;
  shareDisabled?: boolean;
  onShare?: () => void;
  onToggleSidebar: () => void;
  onOpenSearch: () => void;
  onOpenApprovals: () => void;
  onToggleDrawer: () => void;
  onFollow?: (userId: string) => void;
  followingName?: string | null;
  onStopFollowing?: () => void;
}

export function TitleBar({
  title,
  participants = [],
  approvalsCount = 0,
  drawerOpen,
  sidebarOpen,
  shareDisabled = false,
  onShare,
  onToggleSidebar,
  onOpenSearch,
  onOpenApprovals,
  onToggleDrawer,
  onFollow,
  followingName,
  onStopFollowing,
}: TitleBarProps) {
  const people = participants.map((participant) => ({
    id: participant.user.id,
    displayName: participant.user.displayName,
    avatarUrl: participant.user.avatarUrl,
    color: participant.user.color,
    online: participant.online,
  }));

  const sidebarLabel = sidebarOpen ? "Hide sidebar" : "Show sidebar";
  const drawerLabel = drawerOpen ? "Hide right drawer" : "Show right drawer";

  return (
    <header
      className="drag-region"
      style={{
        position: "relative",
        zIndex: 30,
        display: "flex",
        height: 44,
        flexShrink: 0,
        alignItems: "center",
        gap: 8,
        borderBottom: "1px solid var(--mantine-color-default-border)",
        background: "var(--mantine-color-body)",
        paddingInline: 12,
      }}
    >
      <div
        className="no-drag"
        style={{ display: "flex", alignItems: "center", gap: 4 }}
      >
        <Tooltip label={sidebarLabel}>
          <ActionIcon
            variant="default"
            size="sm"
            aria-label={sidebarLabel}
            aria-expanded={sidebarOpen}
            onClick={onToggleSidebar}
          >
            <PanelLeft size={16} aria-hidden />
          </ActionIcon>
        </Tooltip>
        <Button
          type="button"
          variant="light"
          color="gray"
          size="sm"
          onClick={onOpenSearch}
          aria-label="Search workspace"
          leftSection={<Search size={14} aria-hidden />}
          style={{ fontWeight: 400 }}
        >
          <Kbd visibleFrom="md">{modKeyCombo("K")}</Kbd>
        </Button>
      </div>

      <div
        style={{
          display: "flex",
          minWidth: 0,
          flex: 1,
          alignItems: "center",
          justifyContent: "center",
          gap: 8,
          paddingInline: 8,
        }}
      >
        <div style={{ minWidth: 0, textAlign: "center" }}>
          <Text size="sm" fw={500} truncate>
            {title}
          </Text>
        </div>
        {people.length > 0 ? (
          <Box visibleFrom="sm">
            <AvatarStack
              people={people}
              max={4}
              size="sm"
              onSelect={
                onFollow
                  ? (person) => {
                      if (person.id !== "system") {
                        onFollow(person.id);
                      }
                    }
                  : undefined
              }
            />
          </Box>
        ) : null}
        {followingName ? (
          <Box visibleFrom="sm" style={{ display: "flex" }}>
            <Button
              type="button"
              variant="light"
              color="gray"
              size="compact-xs"
              onClick={onStopFollowing}
            >
              Following {followingName} · stop
            </Button>
          </Box>
        ) : null}
      </div>

      <div
        className="no-drag"
        style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 4 }}
      >
        {/* One primary action per page: pages without a thread (Projects,
            Inbox, Settings) do not render a dead Share button. */}
        {onShare ? (
          <Tooltip label="Copy link to this thread">
            <Button
              type="button"
              variant="subtle"
              size="sm"
              disabled={shareDisabled}
              onClick={onShare}
              leftSection={<Share2 size={14} aria-hidden />}
              style={{ fontWeight: 400 }}
            >
              <Box visibleFrom="sm" component="span">
                Share
              </Box>
            </Button>
          </Tooltip>
        ) : null}
        <Tooltip label="Approvals">
          <Indicator
            disabled={approvalsCount <= 0}
            label={approvalsCount > 99 ? "99+" : approvalsCount}
            size={18}
            color="yellow"
            position="top-end"
            offset={2}
            styles={{
              indicator: {
                color: "var(--mantine-color-black)",
                fontSize: "var(--mantine-font-size-xs)",
                fontWeight: 600,
                paddingInline: 2,
              },
            }}
          >
            <ActionIcon
              variant={approvalsCount > 0 ? "light" : "default"}
              color={approvalsCount > 0 ? "yellow" : undefined}
              size="md"
              aria-label="Approvals"
              onClick={onOpenApprovals}
            >
              <ShieldCheck size={16} aria-hidden />
            </ActionIcon>
          </Indicator>
        </Tooltip>
        <Tooltip label={drawerLabel}>
          <ActionIcon
            variant={drawerOpen ? "light" : "default"}
            color={drawerOpen ? "blue" : undefined}
            size="sm"
            aria-label={drawerLabel}
            aria-expanded={drawerOpen}
            aria-pressed={drawerOpen}
            onClick={onToggleDrawer}
          >
            <PanelRight size={16} aria-hidden />
          </ActionIcon>
        </Tooltip>
      </div>
    </header>
  );
}
