import { ActionIcon, Tooltip } from "@mantine/core";
import {
  IconChevronRight,
  IconMessage,
  IconPencil,
  IconPin,
  IconPlus,
  IconTerminal2,
} from "@tabler/icons-react";
import { stateLabel } from "../../lib/machine";
import { fullTime, timeAgo } from "../../lib/sessions";
import type { SessionSummary, TmuxSessionSummary } from "../../api";
import { statusTone } from "../thread/StatusDot";
import { ViewerAvatars } from "../ViewerAvatars";

function plural(count: number, word: string) {
  return `${count} ${word}${count === 1 ? "" : "s"}`;
}

function ThreadDetail({ session }: { session: Extract<SessionSummary, { kind: "thread" }> }) {
  return (
    <>
      <span className="session-subtitle">
        {session.projectName} · {session.agent}
      </span>
      <span className="session-detail">
        <span className="session-state">
          <i className={`dot ${statusTone(session.status)}`} />
          {stateLabel("thread", session.status)}
        </span>
        {session.hasDraft ? (
          <span className="session-chip" title="Draft saved">
            <IconPencil size={11} stroke={1.5} />
            draft
          </span>
        ) : null}
        {session.pendingApprovals > 0 ? (
          <span className="session-chip session-chip-warn">
            {session.pendingApprovals} waiting
          </span>
        ) : null}
        {session.terminal ? (
          <>
            <span className="session-chip">
              <IconTerminal2 size={11} stroke={1.5} />
              shell
              {session.terminal.canDrive ? "" : " · watch"}
            </span>
            <ViewerAvatars users={session.terminal.viewers} size={15} />
            {session.terminal.controller ? (
              <span className="session-chip session-chip-live">
                {session.terminal.controller.displayName || "someone"} driving
              </span>
            ) : null}
          </>
        ) : null}
      </span>
    </>
  );
}

function TmuxDetail({ session }: { session: Extract<SessionSummary, { kind: "tmux" }> }) {
  return (
    <>
      <span className="session-subtitle">
        {session.socket} · {session.command ?? "shell"} ·{" "}
        {plural(session.windows, "window")}
      </span>
      <span className="session-detail">
        {session.projectName ? (
          <span className="session-chip">{session.projectName}</span>
        ) : null}
        <span className="session-chip" title="Every member can attach; one browser drives">
          shared
        </span>
        {session.link?.threadId ? (
          <span
            className="session-chip"
            title={session.link.threadTitle ?? "Linked thread"}
          >
            <IconMessage size={11} stroke={1.5} />
            thread
          </span>
        ) : null}
        <ViewerAvatars users={session.viewers} size={15} />
        {session.controller ? (
          <span className="session-chip session-chip-live">
            {session.controller.displayName || "someone"} driving
          </span>
        ) : null}
        {session.attached > 0 ? (
          <span
            className="session-chip"
            title="tmux clients, including local terminals"
          >
            {session.attached} attached
          </span>
        ) : null}
        <span className="session-cwd" title={session.cwd ?? undefined}>
          {session.cwd ?? "no working directory"}
        </span>
      </span>
    </>
  );
}

/**
 * One row in the Sessions list and switcher. The main button is the whole row;
 * the shell action is a sibling so there are no nested buttons.
 */
export function SessionRow({
  session,
  compact = false,
  active = false,
  onOpen,
  onOpenShell,
  onOpenLinkedThread,
  onStartThread,
  onHover,
}: {
  session: SessionSummary;
  compact?: boolean;
  /** Highlighted by the switcher's keyboard cursor. */
  active?: boolean;
  onOpen: (session: SessionSummary) => void;
  onOpenShell?: (threadId: string) => void;
  onOpenLinkedThread?: (threadId: string) => void;
  onStartThread?: (session: TmuxSessionSummary) => void;
  onHover?: () => void;
}) {
  const title = session.kind === "thread" ? session.title : session.name;
  const activity =
    session.kind === "thread" ? session.lastActivityAt : session.activityAt;
  const pinned = session.kind === "thread" && session.pinned;
  const shellAction =
    session.kind === "thread" && session.terminal && onOpenShell
      ? onOpenShell
      : null;
  const tmuxAction = (() => {
    if (session.kind !== "tmux") return null;
    if (session.link?.threadId && onOpenLinkedThread) {
      const threadId = session.link.threadId;
      return {
        label: "Open linked thread",
        icon: <IconMessage size={14} stroke={1.5} />,
        onClick: () => onOpenLinkedThread(threadId),
      };
    }
    if (session.projectId && onStartThread) {
      return {
        label: "Start a thread from this session",
        icon: <IconPlus size={14} stroke={1.5} />,
        onClick: () => onStartThread(session),
      };
    }
    return null;
  })();

  return (
    <div
      className="session-row"
      data-kind={session.kind}
      data-compact={compact || undefined}
      data-active={active || undefined}
      onMouseEnter={onHover}
    >
      <button
        type="button"
        className="session-row-main"
        aria-label={`Open ${session.kind === "thread" ? "thread" : "tmux session"} ${title}`}
        onClick={() => onOpen(session)}
      >
        <span className="session-kind" aria-hidden>
          {session.kind === "thread" ? (
            <IconMessage size={15} stroke={1.5} />
          ) : (
            <IconTerminal2 size={15} stroke={1.5} />
          )}
        </span>
        <span className="session-body">
          <span className="session-title">
            {pinned ? (
              <IconPin
                size={12}
                stroke={1.5}
                className="session-pin"
                aria-label="Pinned"
              />
            ) : null}
            <span className="session-name">{title}</span>
          </span>
          {session.kind === "thread" ? (
            <ThreadDetail session={session} />
          ) : (
            <TmuxDetail session={session} />
          )}
        </span>
        <span className="session-time" title={fullTime(activity)}>
          {timeAgo(activity)}
        </span>
        <IconChevronRight
          size={15}
          stroke={1.5}
          className="session-chevron"
          aria-hidden
        />
      </button>
      {shellAction ? (
        <Tooltip
          label={
            session.kind === "thread" && session.terminal?.canDrive
              ? "Open this thread's terminal"
              : "Watch this thread's terminal"
          }
        >
          <ActionIcon
            className="session-shell-action"
            aria-label={`Open terminal for ${title}`}
            onClick={() => shellAction(session.id)}
          >
            <IconTerminal2 size={14} stroke={1.5} />
          </ActionIcon>
        </Tooltip>
      ) : tmuxAction ? (
        <Tooltip label={tmuxAction.label}>
          <ActionIcon
            className="session-shell-action"
            aria-label={tmuxAction.label}
            onClick={tmuxAction.onClick}
          >
            {tmuxAction.icon}
          </ActionIcon>
        </Tooltip>
      ) : null}
    </div>
  );
}
