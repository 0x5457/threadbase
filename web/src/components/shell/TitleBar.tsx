import {
  CheckCheck,
  PanelLeft,
  PanelRight,
  Search,
  Share2,
} from "lucide-react";
import type { Participant } from "@/api/types/Participant";
import { AvatarStack, Button, IconButton, Kbd, Tooltip } from "@/components/ui";
import { modKeyLabel } from "@/lib/platform";

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

  return (
    <header className="drag-region relative z-30 flex h-11 shrink-0 items-center gap-2 border-b border-border/60 bg-background px-3">
      <div className="no-drag flex items-center gap-1">
        <Tooltip content={sidebarOpen ? "Hide sidebar" : "Show sidebar"}>
          <IconButton
            label={sidebarOpen ? "Hide sidebar" : "Show sidebar"}
            aria-expanded={sidebarOpen}
            onClick={onToggleSidebar}
          >
            <PanelLeft />
          </IconButton>
        </Tooltip>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={onOpenSearch}
          className="no-drag h-8 gap-2 bg-muted/50 px-2 text-muted-foreground"
          aria-label="Search workspace"
        >
          <Search className="size-3.5" aria-hidden />
          <Kbd className="hidden md:inline-flex">{modKeyLabel()}K</Kbd>
        </Button>
      </div>

      <div className="flex min-w-0 flex-1 items-center justify-center gap-2 px-2">
        <div className="min-w-0 text-center">
          <p className="truncate text-[13px] font-medium leading-tight text-foreground">
            {title}
          </p>
        </div>
        {people.length > 0 ? (
          <AvatarStack
            people={people}
            max={4}
            size="sm"
            className="hidden sm:flex"
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
        ) : null}
        {followingName ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onStopFollowing}
            className="no-drag hidden h-auto rounded-full border border-info/30 bg-info/10 px-2 py-0.5 text-[10px] text-info sm:flex"
          >
            Following {followingName} · stop
          </Button>
        ) : null}
      </div>

      <div className="no-drag flex items-center justify-end gap-1">
        <Tooltip
          content={
            shareDisabled
              ? "Open a thread to copy its share link"
              : "Copy link to this thread"
          }
        >
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={shareDisabled || !onShare}
            onClick={onShare}
            className="gap-1.5 px-2 text-[12px] text-muted-foreground"
          >
            <Share2 className="size-3.5" aria-hidden />
            <span className="hidden sm:inline">Share</span>
          </Button>
        </Tooltip>
        <Tooltip content="Approvals">
          <span className="relative">
            <IconButton
              label="Approvals"
              variant={approvalsCount > 0 ? "subtle" : "ghost"}
              onClick={onOpenApprovals}
            >
              <CheckCheck />
            </IconButton>
            {approvalsCount > 0 ? (
              <span className="pointer-events-none absolute -right-0.5 -top-0.5 flex h-3.5 min-w-3.5 items-center justify-center rounded-full border border-panel bg-warning px-0.5 text-[9px] font-semibold leading-none text-canvas">
                {approvalsCount > 99 ? "99+" : approvalsCount}
              </span>
            ) : null}
          </span>
        </Tooltip>
        <Tooltip content={drawerOpen ? "Hide right drawer" : "Show right drawer"}>
          <IconButton
            label={drawerOpen ? "Hide right drawer" : "Show right drawer"}
            aria-expanded={drawerOpen}
            pressed={drawerOpen}
            onClick={onToggleDrawer}
          >
            <PanelRight />
          </IconButton>
        </Tooltip>
      </div>
    </header>
  );
}
