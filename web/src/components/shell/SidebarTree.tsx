import {
  Bookmark,
  Bot,
  ChevronRight,
  Folder,
  FolderPlus,
  Inbox,
  LogOut,
  MessageSquare,
  Plus,
  Settings,
  Users,
} from "lucide-react";
import {
  useMemo,
  useRef,
  useState,
  type ComponentType,
  type Dispatch,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
  type SetStateAction,
} from "react";
import type { ProjectDto } from "@/api/types/ProjectDto";
import type { SubagentDto } from "@/api/types/SubagentDto";
import type { ThreadDto } from "@/api/types/ThreadDto";
import type { UserDto } from "@/api/types/UserDto";
import type { ThreadStatus } from "@/api/types/ThreadStatus";
import {
  Avatar,
  AvatarStack,
  Badge,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  EmptyState,
  ErrorState,
  IconButton,
  Kbd,
  Skeleton,
  Tooltip,
} from "@/components/ui";
import { ThemeToggle } from "@/components/shell/ThemeToggle";
import { modKeyLabel } from "@/lib/platform";
import { cn } from "@/lib/utils";

export interface SidebarTreeProps {
  projects: ProjectDto[];
  threads: ThreadDto[];
  subagents?: SubagentDto[];
  selectedProjectId?: string;
  selectedThreadId?: string;
  selectedSubagentId?: string;
  loading?: boolean;
  error?: unknown;
  onRetry?: () => void;
  inboxCount?: number;
  user?: UserDto | null;
  userLoading?: boolean;
  threadsPerGroup?: number;
  onSelectProject: (projectId: string) => void;
  onSelectThread: (thread: ThreadDto) => void;
  onSelectSubagent: (subagent: SubagentDto) => void;
  onNewProject: () => void;
  onNewThread: (projectId: string) => void;
  onLocateProject?: (projectId: string) => void;
  onOpenInbox: () => void;
  onOpenSettings: () => void;
  onLogout: () => void;
  onNavigate?: () => void;
}

function statusColor(status: ThreadStatus): string {
  switch (status) {
    case "running":
      return "bg-success";
    case "preparing":
      return "bg-info";
    case "requires_action":
      return "bg-warning";
    case "error":
      return "bg-danger";
    case "interrupted":
      return "bg-danger/60";
    case "archived":
      return "bg-border-strong";
    default:
      return "bg-border-strong";
  }
}

function statusLabel(status: ThreadStatus): string {
  switch (status) {
    case "running":
      return "Running";
    case "preparing":
      return "Preparing";
    case "requires_action":
      return "Waiting for approval";
    case "error":
      return "Error";
    case "interrupted":
      return "Interrupted";
    case "archived":
      return "Archived";
    default:
      return "Idle";
  }
}

function StatusDot({
  status,
  pulse,
  className,
}: {
  status: ThreadStatus;
  pulse?: boolean;
  className?: string;
}) {
  return (
    <span
      title={statusLabel(status)}
      className={cn(
        "inline-block size-1.5 shrink-0 rounded-full",
        statusColor(status),
        (pulse || status === "running" || status === "preparing") &&
          "animate-pulse",
        className,
      )}
    />
  );
}

function sortThreads(threads: ThreadDto[]): ThreadDto[] {
  return [...threads].sort((a, b) => {
    const aTime = Number(a.updatedAt ?? a.createdAt ?? 0);
    const bTime = Number(b.updatedAt ?? b.createdAt ?? 0);
    if (aTime !== bTime) {
      return bTime - aTime;
    }
    return a.title.localeCompare(b.title);
  });
}

function activateRow(
  event: KeyboardEvent<HTMLDivElement>,
  action: () => void,
): void {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    action();
  }
}

function visibleTreeRows(container: HTMLElement | null): HTMLElement[] {
  if (!container) {
    return [];
  }
  return Array.from(
    container.querySelectorAll<HTMLElement>("[data-tree-row]"),
  ).filter((element) => element.offsetParent !== null);
}

function moveTreeFocus(
  event: KeyboardEvent<HTMLElement>,
  container: HTMLElement | null,
): boolean {
  const rows = visibleTreeRows(container);
  const target = (event.target as HTMLElement).closest<HTMLElement>(
    "[data-tree-row]",
  );
  if (!target || rows.length === 0) {
    return false;
  }
  const index = rows.indexOf(target);
  if (index < 0) {
    return false;
  }

  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const offset = event.key === "ArrowDown" ? 1 : -1;
    const next = Math.min(rows.length - 1, Math.max(0, index + offset));
    rows[next]?.focus();
    return true;
  }
  if (event.key === "Home" || event.key === "End") {
    event.preventDefault();
    rows[event.key === "Home" ? 0 : rows.length - 1]?.focus();
    return true;
  }
  if (event.key === "ArrowRight") {
    const expanded = target.dataset.expanded;
    if (expanded === "false") {
      const chevron = target.querySelector<HTMLElement>("[data-tree-chevron]");
      if (chevron) {
        event.preventDefault();
        chevron.click();
        return true;
      }
    }
  }
  if (event.key === "ArrowLeft") {
    if (target.dataset.expanded === "true") {
      const chevron = target.querySelector<HTMLElement>("[data-tree-chevron]");
      if (chevron) {
        event.preventDefault();
        chevron.click();
        return true;
      }
    } else {
      const depth = Number(target.dataset.depth ?? 0);
      for (let cursor = index - 1; cursor >= 0; cursor -= 1) {
        if (Number(rows[cursor]?.dataset.depth ?? 0) < depth) {
          event.preventDefault();
          rows[cursor]?.focus();
          return true;
        }
      }
    }
  }
  return false;
}

function RowChevron({
  expanded,
  onClick,
  label,
}: {
  expanded: boolean;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      aria-label={expanded ? `Collapse ${label}` : `Expand ${label}`}
      data-tree-chevron
      onClick={onClick}
      className="flex size-4 shrink-0 items-center justify-center rounded text-faint hover:bg-hover hover:text-fg"
    >
      <ChevronRight
        className={cn("size-3 transition-transform", expanded && "rotate-90")}
        aria-hidden
      />
    </button>
  );
}

interface ThreadNodeProps {
  thread: ThreadDto;
  depth: number;
  projectName: string;
  showProjectName: boolean;
  childThreads: Map<string, ThreadDto[]>;
  subagentsByThread: Map<string, SubagentDto[]>;
  subagentChildren: Map<string, SubagentDto[]>;
  selectedThreadId?: string;
  selectedSubagentId?: string;
  threadsPerGroup: number;
  collapsedThreads: Set<string>;
  onToggleThread: (threadId: string) => void;
  onSelectThread: (thread: ThreadDto) => void;
  onSelectSubagent: (subagent: SubagentDto) => void;
}

function ThreadNode({
  thread,
  depth,
  projectName,
  showProjectName,
  childThreads,
  subagentsByThread,
  subagentChildren,
  selectedThreadId,
  selectedSubagentId,
  threadsPerGroup,
  collapsedThreads,
  onToggleThread,
  onSelectThread,
  onSelectSubagent,
}: ThreadNodeProps) {
  const [showAll, setShowAll] = useState(false);
  const children = childThreads.get(thread.id) ?? [];
  const subagentRoots = (subagentsByThread.get(thread.id) ?? []).filter(
    (subagent) =>
      !subagent.parentId ||
      !(subagentsByThread.get(thread.id) ?? []).some(
        (candidate) => candidate.id === subagent.parentId,
      ),
  );
  const childCount = children.length + subagentRoots.length;
  const expanded = !collapsedThreads.has(thread.id);
  const selected = selectedThreadId === thread.id && !selectedSubagentId;
  const limit = showAll ? Number.POSITIVE_INFINITY : threadsPerGroup;
  const visibleThreads = children.slice(0, limit);
  const visibleSubagents = subagentRoots.slice(
    0,
    Math.max(0, limit - visibleThreads.length),
  );
  const remaining =
    children.length +
    subagentRoots.length -
    visibleThreads.length -
    visibleSubagents.length;

  const people = thread.participants.map((participant) => ({
    id: participant.user.id,
    displayName: participant.user.displayName,
    avatarUrl: participant.user.avatarUrl,
    color: participant.user.color,
  }));

  const activate = () => onSelectThread(thread);

  return (
    <div role="none">
      <div
        role="treeitem"
        aria-level={depth + 1}
        aria-selected={selected}
        aria-expanded={childCount > 0 ? expanded : undefined}
        tabIndex={0}
        data-tree-row
        data-depth={depth}
        data-expanded={childCount > 0 ? expanded : undefined}
        onClick={activate}
        onKeyDown={(event) => activateRow(event, activate)}
        className={cn(
          "group flex h-[30px] cursor-default items-center gap-1 rounded-md pr-1.5 text-[12px] outline-none transition-colors",
          selected
            ? "bg-active text-fg"
            : "text-muted hover:bg-hover hover:text-fg",
        )}
        style={{ paddingLeft: `${4 + depth * 14}px` }}
      >
        {childCount > 0 ? (
          <RowChevron
            expanded={expanded}
            label={thread.title}
            onClick={(event) => {
              event.stopPropagation();
              onToggleThread(thread.id);
            }}
          />
        ) : (
          <span className="size-4 shrink-0" aria-hidden />
        )}
        <StatusDot
          status={thread.status}
          pulse={thread.status === "running"}
          className="ml-0.5"
        />
        <MessageSquare className="ml-0.5 size-3.5 shrink-0 text-faint" aria-hidden />
        <span
          className={cn(
            "min-w-0 flex-1 truncate",
            (selected || thread.unread) && "font-medium",
          )}
        >
          {thread.title}
        </span>
        {thread.pinned ? (
          <Bookmark className="size-3 shrink-0 text-faint" aria-label="Pinned" />
        ) : null}
        {thread.pendingApprovals > 0 ? (
          <Badge variant="warning" size="sm">
            {thread.pendingApprovals}
          </Badge>
        ) : null}
        {people.length > 0 ? (
          <AvatarStack
            people={people}
            max={2}
            size="xs"
            className="hidden shrink-0 group-hover:flex"
          />
        ) : null}
        {thread.unread ? (
          <span
            className="size-1.5 shrink-0 rounded-full bg-info"
            aria-label="Unread"
          />
        ) : null}
      </div>
      {showProjectName && depth === 0 ? (
        <p
          className="truncate pb-1 pl-9 pr-2 text-[10px] text-faint"
          aria-hidden
        >
          {projectName}
        </p>
      ) : null}
      {expanded && childCount > 0 ? (
        <div className="relative ml-3 border-l border-border/70 pl-1">
          {visibleThreads.map((child) => (
            <ThreadNode
              key={child.id}
              thread={child}
              depth={depth + 1}
              projectName={projectName}
              showProjectName={false}
              childThreads={childThreads}
              subagentsByThread={subagentsByThread}
              subagentChildren={subagentChildren}
              selectedThreadId={selectedThreadId}
              selectedSubagentId={selectedSubagentId}
              threadsPerGroup={threadsPerGroup}
              collapsedThreads={collapsedThreads}
              onToggleThread={onToggleThread}
              onSelectThread={onSelectThread}
              onSelectSubagent={onSelectSubagent}
            />
          ))}
          {visibleSubagents.map((subagent) => (
            <SubagentNode
              key={subagent.id}
              subagent={subagent}
              depth={depth + 1}
              childrenMap={subagentChildren}
              selectedSubagentId={selectedSubagentId}
              collapsedSubagents={collapsedThreads}
              onToggleSubagent={onToggleThread}
              onSelectSubagent={onSelectSubagent}
            />
          ))}
          {remaining > 0 ? (
            <button
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                setShowAll(true);
              }}
              className="flex h-7 w-full items-center rounded-md pl-6 pr-2 text-left text-[11px] text-faint hover:bg-hover hover:text-fg"
            >
              Show {remaining} more
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

interface SubagentNodeProps {
  subagent: SubagentDto;
  depth: number;
  childrenMap: Map<string, SubagentDto[]>;
  selectedSubagentId?: string;
  collapsedSubagents: Set<string>;
  onToggleSubagent: (subagentId: string) => void;
  onSelectSubagent: (subagent: SubagentDto) => void;
}

function SubagentNode({
  subagent,
  depth,
  childrenMap,
  selectedSubagentId,
  collapsedSubagents,
  onToggleSubagent,
  onSelectSubagent,
}: SubagentNodeProps) {
  const children = childrenMap.get(subagent.id) ?? [];
  const expanded = !collapsedSubagents.has(subagent.id);
  const selected = selectedSubagentId === subagent.id;
  const status: ThreadStatus =
    subagent.status === "running"
      ? "running"
      : subagent.status === "requires_action"
        ? "requires_action"
        : subagent.status === "error"
          ? "error"
          : "idle";
  const activate = () => onSelectSubagent(subagent);

  return (
    <div role="none">
      <div
        role="treeitem"
        aria-level={depth + 1}
        aria-selected={selected}
        aria-expanded={children.length > 0 ? expanded : undefined}
        tabIndex={0}
        data-tree-row
        data-depth={depth}
        data-expanded={children.length > 0 ? expanded : undefined}
        onClick={activate}
        onKeyDown={(event) => activateRow(event, activate)}
        className={cn(
          "flex h-[28px] cursor-default items-center gap-1 rounded-md pr-1.5 text-[11.5px] outline-none transition-colors",
          selected
            ? "bg-active text-fg"
            : "text-muted hover:bg-hover hover:text-fg",
        )}
        style={{ paddingLeft: `${4 + depth * 14}px` }}
      >
        {children.length > 0 ? (
          <RowChevron
            expanded={expanded}
            label={subagent.title}
            onClick={(event) => {
              event.stopPropagation();
              onToggleSubagent(subagent.id);
            }}
          />
        ) : (
          <span className="size-4 shrink-0" aria-hidden />
        )}
        <StatusDot status={status} pulse={status === "running"} />
        <Bot className="ml-0.5 size-3.5 shrink-0 text-faint" aria-hidden />
        <span className="min-w-0 flex-1 truncate">{subagent.title}</span>
      </div>
      {expanded && children.length > 0 ? (
        <div className="relative ml-3 border-l border-border/70 pl-1">
          {children.map((child) => (
            <SubagentNode
              key={child.id}
              subagent={child}
              depth={depth + 1}
              childrenMap={childrenMap}
              selectedSubagentId={selectedSubagentId}
              collapsedSubagents={collapsedSubagents}
              onToggleSubagent={onToggleSubagent}
              onSelectSubagent={onSelectSubagent}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

interface ProjectNodeProps {
  project: ProjectDto;
  rootThreads: ThreadDto[];
  childThreads: Map<string, ThreadDto[]>;
  subagentsByThread: Map<string, SubagentDto[]>;
  subagentChildren: Map<string, SubagentDto[]>;
  selectedProjectId?: string;
  selectedThreadId?: string;
  selectedSubagentId?: string;
  threadsPerGroup: number;
  collapsedProjects: Set<string>;
  collapsedThreads: Set<string>;
  onToggleProject: (projectId: string) => void;
  onToggleThread: (threadId: string) => void;
  onSelectProject: (projectId: string) => void;
  onSelectThread: (thread: ThreadDto) => void;
  onSelectSubagent: (subagent: SubagentDto) => void;
  onNewThread: (projectId: string) => void;
  onLocateProject?: (projectId: string) => void;
}

function ProjectNode({
  project,
  rootThreads,
  childThreads,
  subagentsByThread,
  subagentChildren,
  selectedProjectId,
  selectedThreadId,
  selectedSubagentId,
  threadsPerGroup,
  collapsedProjects,
  collapsedThreads,
  onToggleProject,
  onToggleThread,
  onSelectProject,
  onSelectThread,
  onSelectSubagent,
  onNewThread,
  onLocateProject,
}: ProjectNodeProps) {
  const [showAll, setShowAll] = useState(false);
  const expanded = !collapsedProjects.has(project.id);
  const addThread = (
    <button
      type="button"
      onClick={(event) => {
        event.stopPropagation();
        onNewThread(project.id);
      }}
      className="flex h-7 w-full items-center gap-2 rounded-md pl-6 pr-2 text-left text-[11px] text-faint transition-colors hover:bg-hover hover:text-fg"
    >
      <Plus className="size-3" aria-hidden />
      New Thread
    </button>
  );

  if (rootThreads.length === 0) {
    return (
      <div role="none">
        <ProjectRow
          project={project}
          expanded={expanded}
          selected={selectedProjectId === project.id && !selectedThreadId}
          onToggle={() => onToggleProject(project.id)}
          onSelect={() => onSelectProject(project.id)}
          onLocate={() => onLocateProject?.(project.id)}
        />
        {expanded ? (
          <div className="relative ml-3 border-l border-border/70 pl-1">
            <p className="px-2 py-1 text-[11px] text-faint">No threads yet</p>
            {addThread}
          </div>
        ) : null}
      </div>
    );
  }

  const limit = showAll ? Number.POSITIVE_INFINITY : threadsPerGroup;
  const visible = rootThreads.slice(0, limit);
  const remaining = rootThreads.length - visible.length;

  return (
    <div role="none">
      <ProjectRow
        project={project}
        expanded={expanded}
        selected={selectedProjectId === project.id && !selectedThreadId}
        onToggle={() => onToggleProject(project.id)}
        onSelect={() => onSelectProject(project.id)}
        onLocate={() => onLocateProject?.(project.id)}
      />
      {expanded ? (
        <div className="relative ml-3 border-l border-border/70 pl-1">
          {addThread}
          {visible.map((thread) => (
            <ThreadNode
              key={thread.id}
              thread={thread}
              depth={1}
              projectName={project.name}
              showProjectName={false}
              childThreads={childThreads}
              subagentsByThread={subagentsByThread}
              subagentChildren={subagentChildren}
              selectedThreadId={selectedThreadId}
              selectedSubagentId={selectedSubagentId}
              threadsPerGroup={threadsPerGroup}
              collapsedThreads={collapsedThreads}
              onToggleThread={onToggleThread}
              onSelectThread={onSelectThread}
              onSelectSubagent={onSelectSubagent}
            />
          ))}
          {remaining > 0 ? (
            <button
              type="button"
              onClick={() => setShowAll(true)}
              className="flex h-7 w-full items-center rounded-md pl-6 pr-2 text-left text-[11px] text-faint hover:bg-hover hover:text-fg"
            >
              Show {remaining} more
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function ProjectRow({
  project,
  expanded,
  selected,
  onToggle,
  onSelect,
  onLocate,
}: {
  project: ProjectDto;
  expanded: boolean;
  selected: boolean;
  onToggle: () => void;
  onSelect: () => void;
  onLocate?: () => void;
}) {
  return (
    <div
      role="treeitem"
      aria-level={1}
      aria-selected={selected}
      aria-expanded={expanded}
      tabIndex={0}
      data-tree-row
      data-depth={0}
      data-expanded={expanded}
      onClick={onSelect}
      onKeyDown={(event) => activateRow(event, onSelect)}
      className={cn(
        "flex h-[30px] cursor-default items-center gap-1 rounded-md pr-1.5 text-[12px] outline-none transition-colors",
        selected ? "bg-active text-fg" : "text-muted hover:bg-hover hover:text-fg",
      )}
      style={{ paddingLeft: "4px" }}
    >
      <RowChevron
        expanded={expanded}
        label={project.name}
        onClick={(event) => {
          event.stopPropagation();
          onToggle();
        }}
      />
      <Folder className="size-3.5 shrink-0 text-faint" aria-hidden />
      <span className="min-w-0 flex-1 truncate font-medium">{project.name}</span>
      {project.waitingApprovals > 0 ? (
        <Badge variant="warning" size="sm">
          {project.waitingApprovals}
        </Badge>
      ) : null}
      {!project.available ? (
        <button
          type="button"
          className="shrink-0 rounded px-1 text-[10px] text-warning hover:bg-hover hover:text-warning"
          title="Project path is unavailable"
          onClick={(event) => {
            event.stopPropagation();
            onLocate?.();
          }}
        >
          Locate…
        </button>
      ) : null}
      {project.activeThreads > 0 ? (
        <span className="text-[10px] tabular-nums text-faint">
          {project.activeThreads}
        </span>
      ) : null}
    </div>
  );
}

export function SidebarTree({
  projects,
  threads,
  subagents = [],
  selectedProjectId,
  selectedThreadId,
  selectedSubagentId,
  loading = false,
  error,
  onRetry,
  inboxCount = 0,
  user,
  userLoading = false,
  threadsPerGroup = 10,
  onSelectProject,
  onSelectThread,
  onSelectSubagent,
  onNewProject,
  onNewThread,
  onLocateProject,
  onOpenInbox,
  onOpenSettings,
  onLogout,
  onNavigate,
}: SidebarTreeProps) {
  const treeRef = useRef<HTMLDivElement>(null);
  const [collapsedProjects, setCollapsedProjects] = useState<Set<string>>(
    new Set(),
  );
  const [collapsedThreads, setCollapsedThreads] = useState<Set<string>>(
    new Set(),
  );

  const projectById = useMemo(
    () => new Map(projects.map((project) => [project.id, project])),
    [projects],
  );

  const childThreads = useMemo(() => {
    const map = new Map<string, ThreadDto[]>();
    for (const thread of threads) {
      if (!thread.parentThreadId) {
        continue;
      }
      const list = map.get(thread.parentThreadId) ?? [];
      list.push(thread);
      map.set(thread.parentThreadId, list);
    }
    for (const [key, list] of map) {
      map.set(key, sortThreads(list));
    }
    return map;
  }, [threads]);

  const rootThreadsByProject = useMemo(() => {
    const map = new Map<string, ThreadDto[]>();
    for (const project of projects) {
      map.set(project.id, []);
    }
    for (const thread of threads) {
      if (
        thread.parentThreadId &&
        threads.some((candidate) => candidate.id === thread.parentThreadId)
      ) {
        continue;
      }
      const list = map.get(thread.projectId);
      if (list) {
        list.push(thread);
      }
    }
    for (const [key, list] of map) {
      map.set(key, sortThreads(list));
    }
    return map;
  }, [projects, threads]);

  const pinnedThreads = useMemo(() => {
    return sortThreads(
      threads.filter(
        (thread) =>
          thread.pinned &&
          (!thread.parentThreadId ||
            !threads.some((candidate) => candidate.id === thread.parentThreadId)),
      ),
    );
  }, [threads]);

  const orphanThreads = useMemo(() => {
    return sortThreads(
      threads.filter(
        (thread) =>
          !thread.pinned &&
          !projectById.has(thread.projectId) &&
          (!thread.parentThreadId ||
            !threads.some((candidate) => candidate.id === thread.parentThreadId)),
      ),
    );
  }, [threads, projectById]);

  const subagentsByThread = useMemo(() => {
    const map = new Map<string, SubagentDto[]>();
    for (const subagent of subagents) {
      const list = map.get(subagent.threadId) ?? [];
      list.push(subagent);
      map.set(subagent.threadId, list);
    }
    for (const list of map.values()) {
      list.sort((a, b) => Number(a.createdAt) - Number(b.createdAt));
    }
    return map;
  }, [subagents]);

  const subagentChildren = useMemo(() => {
    const map = new Map<string, SubagentDto[]>();
    for (const subagent of subagents) {
      if (!subagent.parentId) {
        continue;
      }
      const list = map.get(subagent.parentId) ?? [];
      list.push(subagent);
      map.set(subagent.parentId, list);
    }
    for (const list of map.values()) {
      list.sort((a, b) => Number(a.createdAt) - Number(b.createdAt));
    }
    return map;
  }, [subagents]);

  const toggleInSet = (
    setter: Dispatch<SetStateAction<Set<string>>>,
    id: string,
  ) => {
    setter((previous) => {
      const next = new Set(previous);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const selectProject = (projectId: string) => {
    onSelectProject(projectId);
    onNavigate?.();
  };
  const selectThread = (thread: ThreadDto) => {
    onSelectThread(thread);
    onNavigate?.();
  };
  const selectSubagent = (subagent: SubagentDto) => {
    onSelectSubagent(subagent);
    onNavigate?.();
  };
  const openInbox = () => {
    onOpenInbox();
    onNavigate?.();
  };
  const openSettings = () => {
    onOpenSettings();
    onNavigate?.();
  };
  const signOut = () => {
    onLogout();
    onNavigate?.();
  };

  return (
    <div className="flex h-full min-h-0 flex-col bg-panel">
      <div className="flex h-9 shrink-0 items-center justify-between border-b border-border px-2.5">
        <span className="text-[12px] font-medium text-fg">Threadbase</span>
        <Tooltip content={`New project (${modKeyLabel()}⇧A)`}>
          <IconButton
            label="New project"
            size="sm"
            onClick={onNewProject}
          >
            <FolderPlus />
          </IconButton>
        </Tooltip>
      </div>

      <div
        ref={treeRef}
        role="tree"
        aria-label="Projects and threads"
        className="min-h-0 flex-1 overflow-y-auto px-1.5 py-1.5"
        onKeyDown={(event) => {
          moveTreeFocus(event, treeRef.current);
        }}
      >
        {loading ? (
          <div className="space-y-1.5 px-1 pt-1">
            {Array.from({ length: 6 }).map((_, index) => (
              <Skeleton
                key={index}
                className={cn("h-6", index % 3 === 0 ? "w-11/12" : "w-4/5")}
              />
            ))}
          </div>
        ) : error ? (
          <ErrorState
            error={error}
            title="Cannot load sidebar"
            onRetry={onRetry}
            compact
          />
        ) : projects.length === 0 && threads.length === 0 ? (
          <EmptyState
            icon={FolderPlus}
            title="No projects yet"
            description="Add a local repository to start a thread."
            compact
            action={
              <button
                type="button"
                onClick={onNewProject}
                className="rounded-md border border-border bg-surface px-2 py-1 text-[11px] text-fg hover:bg-hover"
              >
                Add project
              </button>
            }
          />
        ) : (
          <div className="space-y-2.5">
            {pinnedThreads.length > 0 ? (
              <section aria-label="Pinned">
                <SidebarGroupLabel icon={Bookmark}>Pinned</SidebarGroupLabel>
                <div role="group">
                  {pinnedThreads.map((thread) => (
                    <ThreadNode
                      key={thread.id}
                      thread={thread}
                      depth={0}
                      projectName={
                        projectById.get(thread.projectId)?.name ?? "Unknown project"
                      }
                      showProjectName
                      childThreads={childThreads}
                      subagentsByThread={subagentsByThread}
                      subagentChildren={subagentChildren}
                      selectedThreadId={selectedThreadId}
                      selectedSubagentId={selectedSubagentId}
                      threadsPerGroup={threadsPerGroup}
                      collapsedThreads={collapsedThreads}
                      onToggleThread={(id) =>
                        toggleInSet(setCollapsedThreads, id)
                      }
                      onSelectThread={selectThread}
                      onSelectSubagent={selectSubagent}
                    />
                  ))}
                </div>
              </section>
            ) : null}

            <section aria-label="Projects">
              <SidebarGroupLabel icon={Folder}>Projects</SidebarGroupLabel>
              <div role="group">
                {projects.map((project) => (
                  <ProjectNode
                    key={project.id}
                    project={project}
                    rootThreads={rootThreadsByProject.get(project.id) ?? []}
                    childThreads={childThreads}
                    subagentsByThread={subagentsByThread}
                    subagentChildren={subagentChildren}
                    selectedProjectId={selectedProjectId}
                    selectedThreadId={selectedThreadId}
                    selectedSubagentId={selectedSubagentId}
                    threadsPerGroup={threadsPerGroup}
                    collapsedProjects={collapsedProjects}
                    collapsedThreads={collapsedThreads}
                    onToggleProject={(id) =>
                      toggleInSet(setCollapsedProjects, id)
                    }
                    onToggleThread={(id) => toggleInSet(setCollapsedThreads, id)}
                    onSelectProject={selectProject}
                    onSelectThread={selectThread}
                    onSelectSubagent={selectSubagent}
                    onNewThread={(projectId) => {
                      onNewThread(projectId);
                      onNavigate?.();
                    }}
                    onLocateProject={(projectId) => onLocateProject?.(projectId)}
                  />
                ))}
              </div>
            </section>

            {orphanThreads.length > 0 ? (
              <section aria-label="Threads">
                <SidebarGroupLabel icon={MessageSquare}>
                  Threads
                </SidebarGroupLabel>
                <div role="group">
                  {orphanThreads.map((thread) => (
                    <ThreadNode
                      key={thread.id}
                      thread={thread}
                      depth={0}
                      projectName={thread.projectId}
                      showProjectName
                      childThreads={childThreads}
                      subagentsByThread={subagentsByThread}
                      subagentChildren={subagentChildren}
                      selectedThreadId={selectedThreadId}
                      selectedSubagentId={selectedSubagentId}
                      threadsPerGroup={threadsPerGroup}
                      collapsedThreads={collapsedThreads}
                      onToggleThread={(id) =>
                        toggleInSet(setCollapsedThreads, id)
                      }
                      onSelectThread={selectThread}
                      onSelectSubagent={selectSubagent}
                    />
                  ))}
                </div>
              </section>
            ) : null}
          </div>
        )}
      </div>

      <div className="shrink-0 border-t border-border p-1.5">
        <button
          type="button"
          onClick={openInbox}
          className="mb-1 flex h-7 w-full items-center gap-2 rounded-md px-2 text-left text-[11.5px] text-muted transition-colors hover:bg-hover hover:text-fg"
        >
          <Inbox className="size-3.5" aria-hidden />
          <span className="flex-1">Inbox</span>
          {inboxCount > 0 ? (
            <Badge variant="info" size="sm">
              {inboxCount > 99 ? "99+" : inboxCount}
            </Badge>
          ) : null}
          <Kbd>{modKeyLabel()}⇧J</Kbd>
        </button>
        <div className="flex items-center gap-2 rounded-md px-1.5 py-1">
          {userLoading ? (
            <Skeleton className="size-6 rounded-full" />
          ) : user ? (
            <Avatar
              person={{
                id: user.id,
                displayName: user.displayName,
                avatarUrl: user.avatarUrl,
              }}
              size="md"
              online
            />
          ) : (
            <span className="flex size-6 items-center justify-center rounded-full bg-raised text-faint">
              <Users className="size-3.5" aria-hidden />
            </span>
          )}
          <div className="min-w-0 flex-1">
            <p className="truncate text-[11.5px] font-medium text-fg">
              {user?.displayName ?? (userLoading ? "Loading…" : "Signed out")}
            </p>
            <p className="truncate text-[10px] text-faint">
              {user ? `@${user.username}` : "No active session"}
            </p>
          </div>
          <ThemeToggle />
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <IconButton label="Account menu" size="sm">
                <Settings />
              </IconButton>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" side="top">
              <DropdownMenuLabel>
                {user ? `@${user.username}` : "Account"}
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={openInbox}>
                <Inbox />
                Inbox
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={openSettings}>
                <Settings />
                Settings
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem destructive onSelect={signOut}>
                <LogOut />
                Sign out
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </div>
  );
}

function SidebarGroupLabel({
  children,
  icon: Icon,
}: {
  children: ReactNode;
  icon: ComponentType<{ className?: string }>;
}) {
  return (
    <div className="flex items-center gap-1.5 px-1.5 pb-1 pt-1.5 text-[10px] font-semibold uppercase tracking-[0.15em] text-faint">
      <Icon className="size-3" />
      {children}
    </div>
  );
}
