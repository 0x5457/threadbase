import {
  IconChevronDown,
  IconChevronRight,
  IconGitBranch,
  IconPencil,
} from "@tabler/icons-react";
import { Fragment, memo, useCallback, useState } from "react";
import type { Subagent, ThreadSummary } from "../../api";
import { relativeTime } from "../thread/format";
import { statusTone } from "../thread/StatusDot";
import { useLongPress } from "../../lib/use-long-press";

/** One-line label for a delegation. Agents usually open with the repository
 *  boilerplate (`Repo: /path. ...`), which every sibling repeats; dropping it
 *  leaves the part that actually tells the subagents apart. */
function taskLabel(subagent: Subagent): string {
  const text = (subagent.description ?? subagent.title).trim();
  return text.replace(/^repo(?: worktree)?\b[^.]*\.\s+/i, "") || text;
}

/** Depth-first order for the delegation tree; roots are subagents whose
 *  parent is not part of the list (workflow containers are not stored). */
function orderedSubagents(
  subagents: Subagent[],
): Array<{ subagent: Subagent; depth: number }> {
  const ids = new Set(subagents.map((subagent) => String(subagent.id)));
  const children = new Map<string, Subagent[]>();
  const roots: Subagent[] = [];
  for (const subagent of subagents) {
    const parent = subagent.parentId ? String(subagent.parentId) : null;
    if (parent && ids.has(parent)) {
      const list = children.get(parent) ?? [];
      list.push(subagent);
      children.set(parent, list);
    } else {
      roots.push(subagent);
    }
  }
  const ordered: Array<{ subagent: Subagent; depth: number }> = [];
  const seen = new Set<string>();
  const walk = (items: Subagent[], depth: number) => {
    for (const subagent of items) {
      const id = String(subagent.id);
      if (seen.has(id)) continue;
      seen.add(id);
      ordered.push({ subagent, depth });
      const nested = children.get(id);
      if (nested?.length) walk(nested, depth + 1);
    }
  };
  walk(roots, 0);
  return ordered;
}

/** The right-hand gutter: one signal per row, the most urgent first. */
function ThreadSignal({ thread }: { thread: ThreadSummary }) {
  const needs = thread.pendingApprovals + thread.pendingElicitations;
  if (needs > 0) {
    return (
      <span
        className="side-needs"
        title={`${needs} request${needs === 1 ? "" : "s"} waiting for a person`}
      >
        {needs}
      </span>
    );
  }
  if (thread.status === "running" || thread.status === "preparing") {
    return <span className="dot running" title="Agent working" />;
  }
  if (thread.status === "error" || thread.status === "interrupted") {
    return (
      <span
        className={`dot ${statusTone(thread.status)}`}
        title={thread.status === "error" ? "Agent failed" : "Agent stopped"}
      />
    );
  }
  if (thread.hasDraft) {
    return (
      <span className="thread-draft-mark" title="Draft saved" aria-label="Draft saved">
        <IconPencil size={11} stroke={1.5} />
      </span>
    );
  }
  return null;
}

/** One thread row: title, then the context that tells two threads apart. */
export const SidebarThreadRow = memo(function SidebarThreadRow({
  thread,
  selected,
  selectedSubagentId,
  projectName,
  onOpen,
  onSelectSubagent,
  onLongPress,
}: {
  thread: ThreadSummary;
  selected: boolean;
  selectedSubagentId: string | null;
  /** Shown when the row is outside its project tree (pinned shelf). */
  projectName?: string;
  onOpen: (thread: ThreadSummary) => void;
  onSelectSubagent?: (thread: ThreadSummary, subagent: Subagent) => void;
  onLongPress: (thread: ThreadSummary) => void;
}) {
  const press = useLongPress(() => onLongPress(thread));
  // `null` follows the route: selecting a subagent (including a deep link)
  // opens the tree, while an explicit click can still collapse it.
  const [open, setOpen] = useState<boolean | null>(null);
  const subagents = thread.subagents ?? [];
  const rows = orderedSubagents(subagents);
  const hasSelected = rows.some(
    ({ subagent }) => String(subagent.id) === selectedSubagentId,
  );
  const expanded = open ?? hasSelected;
  const context = projectName ?? thread.model ?? thread.agent;

  return (
    <Fragment>
      <button
        type="button"
        className="side-thread"
        data-active={selected ? "true" : undefined}
        aria-current={selected ? "true" : undefined}
        onClick={() => onOpen(thread)}
        {...press}
      >
        <span className="side-thread-body">
          <span className="side-thread-title">{thread.title}</span>
          <span className="side-thread-meta">
            <span className="side-thread-context">{context}</span>
            <span className="side-thread-time">{relativeTime(thread.updatedAt)}</span>
          </span>
        </span>
        <span className="side-thread-status">
          <ThreadSignal thread={thread} />
        </span>
      </button>
      {subagents.length && onSelectSubagent ? (
        <>
          <button
            type="button"
            className="side-subagent-toggle"
            aria-expanded={expanded}
            onClick={() => setOpen(!expanded)}
          >
            {expanded ? (
              <IconChevronDown size={11} stroke={1.5} />
            ) : (
              <IconChevronRight size={11} stroke={1.5} />
            )}
            <span>
              {subagents.length} subagent{subagents.length === 1 ? "" : "s"}
            </span>
          </button>
          {expanded ? (
            <div className="side-subagents">
              {rows.map(({ subagent, depth }) => {
                const active = String(subagent.id) === selectedSubagentId;
                const label = taskLabel(subagent);
                return (
                  <button
                    type="button"
                    key={String(subagent.id)}
                    className={`side-subagent${active ? " selected" : ""}`}
                    style={{ paddingLeft: 10 + depth * 12 }}
                    title={`${subagent.title} · ${label}`}
                    aria-current={active ? "true" : undefined}
                    onClick={() => onSelectSubagent(thread, subagent)}
                  >
                    <IconGitBranch
                      size={11}
                      stroke={1.5}
                      className="side-subagent-icon"
                    />
                    <span className="side-subagent-title">{label}</span>
                    <span className={`dot ${statusTone(subagent.status)}`} />
                  </button>
                );
              })}
            </div>
          ) : null}
        </>
      ) : null}
    </Fragment>
  );
});

/** Collapse state is a per-device preference, like the shelf in a mail client. */
const SHELF_KEY = "threadbase:shelves";

function readShelf(key: string, fallback: boolean): boolean {
  try {
    const stored = window.localStorage.getItem(`${SHELF_KEY}:${key}`);
    return stored === null ? fallback : stored === "open";
  } catch {
    return fallback;
  }
}

function writeShelf(key: string, open: boolean) {
  try {
    window.localStorage.setItem(`${SHELF_KEY}:${key}`, open ? "open" : "closed");
  } catch {
    // A blocked storage area must not break navigation.
  }
}

/**
 * A folded row of parked threads. Parked work stays one click away instead of
 * competing with the thread a person is actually working on.
 */
export function ThreadShelf({
  shelfKey,
  label,
  count,
  defaultOpen = false,
  forceOpen = false,
  children,
}: {
  shelfKey: string;
  label: string;
  count: number;
  defaultOpen?: boolean;
  /** The open thread lives here, so the shelf cannot stay folded. */
  forceOpen?: boolean;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(() => readShelf(shelfKey, defaultOpen));
  const toggle = useCallback(() => {
    setOpen((current) => {
      writeShelf(shelfKey, !current);
      return !current;
    });
  }, [shelfKey]);
  const expanded = open || forceOpen;
  if (count === 0) return null;
  return (
    <>
      <button
        type="button"
        className="side-shelf"
        aria-expanded={expanded}
        onClick={toggle}
      >
        {expanded ? (
          <IconChevronDown size={11} stroke={1.5} />
        ) : (
          <IconChevronRight size={11} stroke={1.5} />
        )}
        <span className="side-shelf-label">{label}</span>
        <span className="side-shelf-count">{count}</span>
      </button>
      {expanded ? children : null}
    </>
  );
}
