import {
  Bookmark,
  Bot,
  ChevronRight,
  FolderGit2,
  FolderPlus,
  Inbox,
  LogOut,
  MessageSquareText,
  Plus,
  Settings,
  Users,
  X,
} from "lucide-react";
import {
  ActionIcon,
  Avatar,
  Badge,
  Box,
  Indicator,
  Kbd,
  Menu,
  Skeleton,
  Stack,
  Tooltip,
  UnstyledButton,
} from "@mantine/core";
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
import type { SubagentStatus } from "@/api/types/SubagentStatus";
import type { ThreadDto } from "@/api/types/ThreadDto";
import type { UserDto } from "@/api/types/UserDto";
import type { ThreadStatus } from "@/api/types/ThreadStatus";
import { AvatarStack, EmptyState, ErrorState } from "@/components/app";
import { ThemeToggle } from "@/components/shell/ThemeToggle";
import { modKeyLabel } from "@/lib/platform";

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
  /** Renders a close button in the header when the tree is an overlay sheet. */
  onClose?: () => void;
}

type AnyStatus = ThreadStatus | SubagentStatus;

const rowTransition = "background-color 120ms ease, color 120ms ease";
const rowHover = {
  ":hover": {
    backgroundColor: "var(--mantine-color-default-hover)",
    color: "var(--mantine-color-text)",
  },
} as const;

function statusColor(status: AnyStatus): string {
  switch (status) {
    case "running":
      return "var(--mantine-color-teal-filled)";
    case "preparing":
      return "var(--mantine-color-blue-filled)";
    case "requires_action":
      return "var(--mantine-color-yellow-filled)";
    case "error":
      return "var(--mantine-color-red-filled)";
    case "interrupted":
      return "var(--mantine-color-red-filled)";
    case "archived":
      return "var(--mantine-color-gray-6)";
    default:
      return "var(--mantine-color-gray-6)";
  }
}

function statusLabel(status: AnyStatus): string {
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
    case "cancelled":
      return "Cancelled";
    default:
      return "Idle";
  }
}

function StatusDot({
  status,
  pulse,
}: {
  status: AnyStatus;
  pulse?: boolean;
}) {
  return (
    <span
      title={statusLabel(status)}
      style={{
        display: "inline-block",
        width: 6,
        height: 6,
        flexShrink: 0,
        borderRadius: "50%",
        backgroundColor: statusColor(status),
        ...(pulse || status === "running" || status === "preparing"
          ? { animation: "sidebar-pulse 1.6s ease-in-out infinite" }
          : {}),
      }}
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

function TreeRow({
  level,
  depth,
  selected,
  expanded,
  onSelect,
  children,
}: {
  level: number;
  depth: number;
  selected: boolean;
  expanded?: boolean;
  onSelect: () => void;
  children: ReactNode;
}) {
  return (
    <UnstyledButton
      component="div"
      role="treeitem"
      aria-level={level}
      aria-selected={selected}
      aria-expanded={expanded}
      tabIndex={0}
      data-tree-row
      data-depth={depth}
      data-expanded={expanded}
      onClick={onSelect}
      onKeyDown={(event) => activateRow(event, onSelect)}
      h={{ base: 40, md: 34 }}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 4,
        borderRadius: "var(--mantine-radius-md)",
        paddingRight: 4,
        paddingLeft: 4 + depth * 16,
        fontSize: 13,
        cursor: "default",
        outline: "none",
        transition: rowTransition,
        backgroundColor: selected
          ? "var(--mantine-color-default-hover)"
          : undefined,
        color: selected
          ? "var(--mantine-color-text)"
          : "var(--mantine-color-dimmed)",
      }}
      styles={{ root: rowHover }}
    >
      {children}
    </UnstyledButton>
  );
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
    <UnstyledButton
      type="button"
      aria-label={expanded ? `Collapse ${label}` : `Expand ${label}`}
      data-tree-chevron
      onClick={onClick}
      w={{ base: 28, md: 16 }}
      h={{ base: 28, md: 16 }}
      style={{
        display: "flex",
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "center",
        borderRadius: "var(--mantine-radius-md)",
        color: "var(--mantine-color-dimmed)",
      }}
      styles={{ root: rowHover }}
    >
      <ChevronRight
        size={12}
        aria-hidden
        style={{
          transform: expanded ? "rotate(90deg)" : "none",
          transition: "transform 120ms ease",
        }}
      />
    </UnstyledButton>
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
      <TreeRow
        level={depth + 1}
        depth={depth}
        selected={selected}
        expanded={childCount > 0 ? expanded : undefined}
        onSelect={activate}
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
          <span style={{ width: 16, flexShrink: 0 }} aria-hidden />
        )}
        <StatusDot status={thread.status} pulse={thread.status === "running"} />
        <MessageSquareText
          size={14}
          aria-hidden
          style={{
            marginLeft: 2,
            flexShrink: 0,
            color: "var(--mantine-color-dimmed)",
          }}
        />
        <span
          style={{
            minWidth: 0,
            flex: 1,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            ...(selected || thread.unread ? { fontWeight: 500 } : {}),
          }}
        >
          {thread.title}
        </span>
        {thread.pinned ? (
          <Bookmark
            size={12}
            aria-label="Pinned"
            style={{ flexShrink: 0, color: "var(--mantine-color-dimmed)" }}
          />
        ) : null}
        {thread.pendingApprovals > 0 ? (
          <Badge color="yellow" variant="light" size="xs">
            {thread.pendingApprovals}
          </Badge>
        ) : null}
        {people.length > 0 ? (
          <Box style={{ flexShrink: 0 }}>
            <AvatarStack people={people} max={2} size="xs" />
          </Box>
        ) : null}
        {thread.unread ? (
          <span
            style={{
              width: 6,
              height: 6,
              flexShrink: 0,
              borderRadius: "50%",
              backgroundColor: "var(--mantine-color-blue-filled)",
            }}
            aria-label="Unread"
          />
        ) : null}
      </TreeRow>
      {showProjectName && depth === 0 ? (
        <p
          aria-hidden
          style={{
            margin: 0,
            padding: "0 8px 4px 36px",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            fontSize: 13,
            color: "var(--mantine-color-dimmed)",
          }}
        >
          {projectName}
        </p>
      ) : null}
      {expanded && childCount > 0 ? (
        <div
          style={{
            position: "relative",
            marginLeft: 12,
            borderLeft: "1px solid var(--mantine-color-default-border)",
            paddingLeft: 4,
          }}
        >
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
            <UnstyledButton
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                setShowAll(true);
              }}
              h={{ base: 36, md: 28 }}
              style={{
                display: "flex",
                width: "100%",
                alignItems: "center",
                borderRadius: "var(--mantine-radius-md)",
                paddingLeft: 24,
                paddingRight: 8,
                textAlign: "left",
                fontSize: 13,
                color: "var(--mantine-color-dimmed)",
              }}
              styles={{ root: rowHover }}
            >
              Show {remaining} more
            </UnstyledButton>
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
  const activate = () => onSelectSubagent(subagent);

  return (
    <div role="none">
      <TreeRow
        level={depth + 1}
        depth={depth}
        selected={selected}
        expanded={children.length > 0 ? expanded : undefined}
        onSelect={activate}
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
          <span style={{ width: 16, flexShrink: 0 }} aria-hidden />
        )}
        <StatusDot
          status={subagent.status}
          pulse={subagent.status === "running"}
        />
        <Bot
          size={14}
          aria-hidden
          style={{
            marginLeft: 2,
            flexShrink: 0,
            color: "var(--mantine-color-dimmed)",
          }}
        />
        <span
          style={{
            minWidth: 0,
            flex: 1,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {subagent.title}
        </span>
      </TreeRow>
      {expanded && children.length > 0 ? (
        <div
          style={{
            position: "relative",
            marginLeft: 12,
            borderLeft: "1px solid var(--mantine-color-default-border)",
            paddingLeft: 4,
          }}
        >
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
    <UnstyledButton
      type="button"
      onClick={(event) => {
        event.stopPropagation();
        onNewThread(project.id);
      }}
      h={{ base: 36, md: 28 }}
      style={{
        display: "flex",
        width: "100%",
        alignItems: "center",
        gap: 8,
        borderRadius: "var(--mantine-radius-md)",
        paddingLeft: 24,
        paddingRight: 8,
        textAlign: "left",
        fontSize: 13,
        color: "var(--mantine-color-dimmed)",
      }}
      styles={{ root: rowHover }}
    >
      <Plus size={12} aria-hidden />
      New Thread
    </UnstyledButton>
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
          <div
            style={{
              position: "relative",
              marginLeft: 12,
              borderLeft: "1px solid var(--mantine-color-default-border)",
              paddingLeft: 4,
            }}
          >
            <p
              style={{
                margin: 0,
                padding: "4px 8px",
                fontSize: 13,
                color: "var(--mantine-color-dimmed)",
              }}
            >
              No threads yet
            </p>
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
        <div
          style={{
            position: "relative",
            marginLeft: 12,
            borderLeft: "1px solid var(--mantine-color-default-border)",
            paddingLeft: 4,
          }}
        >
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
            <UnstyledButton
              type="button"
              onClick={() => setShowAll(true)}
              h={{ base: 36, md: 28 }}
              style={{
                display: "flex",
                width: "100%",
                alignItems: "center",
                borderRadius: "var(--mantine-radius-md)",
                paddingLeft: 24,
                paddingRight: 8,
                textAlign: "left",
                fontSize: 13,
                color: "var(--mantine-color-dimmed)",
              }}
              styles={{ root: rowHover }}
            >
              Show {remaining} more
            </UnstyledButton>
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
    <TreeRow
      level={1}
      depth={0}
      selected={selected}
      expanded={expanded}
      onSelect={onSelect}
    >
      <RowChevron
        expanded={expanded}
        label={project.name}
        onClick={(event) => {
          event.stopPropagation();
          onToggle();
        }}
      />
      <FolderGit2
        size={14}
        aria-hidden
        style={{ flexShrink: 0, color: "var(--mantine-color-dimmed)" }}
      />
      <span
        style={{
          minWidth: 0,
          flex: 1,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          fontWeight: 500,
        }}
      >
        {project.name}
      </span>
      {project.waitingApprovals > 0 ? (
        <Badge color="yellow" variant="light" size="xs">
          {project.waitingApprovals}
        </Badge>
      ) : null}
      {!project.available ? (
        <UnstyledButton
          type="button"
          title="Project path is unavailable"
          onClick={(event) => {
            event.stopPropagation();
            onLocate?.();
          }}
          style={{
            flexShrink: 0,
            borderRadius: "var(--mantine-radius-md)",
            padding: "0 4px",
            fontSize: 13,
            color: "var(--mantine-color-yellow-6)",
          }}
          styles={{
            root: {
              ":hover": {
                backgroundColor: "var(--mantine-color-default-hover)",
                color: "var(--mantine-color-yellow-7)",
              },
            },
          }}
        >
          Locate…
        </UnstyledButton>
      ) : null}
      {project.activeThreads > 0 ? (
        <span
          style={{
            fontSize: 13,
            fontVariantNumeric: "tabular-nums",
            color: "var(--mantine-color-dimmed)",
          }}
        >
          {project.activeThreads}
        </span>
      ) : null}
    </TreeRow>
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
  onClose,
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
    <div
      style={{
        display: "flex",
        height: "100%",
        minHeight: 0,
        flexDirection: "column",
        backgroundColor: "var(--mantine-color-body)",
      }}
    >
      <div
        style={{
          display: "flex",
          height: 44,
          flexShrink: 0,
          alignItems: "center",
          justifyContent: "space-between",
          padding: "0 12px",
        }}
      >
        <span
          style={{
            fontSize: 13,
            fontWeight: 600,
            letterSpacing: "-0.01em",
            color: "var(--mantine-color-text)",
          }}
        >
          Threadbase
        </span>
        <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
          <Tooltip label={`New project (${modKeyLabel()}⇧A)`}>
            <ActionIcon
              variant="default"
              size="lg"
              aria-label="New project"
              onClick={onNewProject}
            >
              <FolderPlus size={16} />
            </ActionIcon>
          </Tooltip>
          {onClose ? (
            <ActionIcon
              variant="subtle"
              color="gray"
              size="lg"
              aria-label="Close sidebar"
              onClick={onClose}
            >
              <X size={16} />
            </ActionIcon>
          ) : null}
        </div>
      </div>

      <div
        ref={treeRef}
        role="tree"
        aria-label="Projects and threads"
        style={{
          minHeight: 0,
          flex: 1,
          overflowY: "auto",
          padding: "8px 8px",
        }}
        onKeyDown={(event) => {
          moveTreeFocus(event, treeRef.current);
        }}
      >
        {loading ? (
          <Box px={4} pt={4}>
            <Stack gap={6}>
              {Array.from({ length: 6 }).map((_, index) => (
                <Skeleton
                  key={index}
                  height={24}
                  width={index % 3 === 0 ? "92%" : "80%"}
                  radius="md"
                />
              ))}
            </Stack>
          </Box>
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
              <UnstyledButton
                type="button"
                onClick={onNewProject}
                style={{
                  borderRadius: "var(--mantine-radius-md)",
                  border: "1px solid var(--mantine-color-default-border)",
                  backgroundColor: "var(--mantine-color-default)",
                  padding: "4px 8px",
                  fontSize: 13,
                  color: "var(--mantine-color-text)",
                }}
                styles={{ root: rowHover }}
              >
                Add project
              </UnstyledButton>
            }
          />
        ) : (
          <Stack gap={10}>
            {pinnedThreads.length > 0 ? (
              <section aria-label="Pinned">
                <SidebarGroupLabel icon={Bookmark}>Pinned</SidebarGroupLabel>
                <Box role="group">
                  {pinnedThreads.map((thread) => (
                    <ThreadNode
                      key={thread.id}
                      thread={thread}
                      depth={0}
                      projectName={
                        projectById.get(thread.projectId)?.name ??
                        "Unknown project"
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
                </Box>
              </section>
            ) : null}

            <section aria-label="Projects">
              <SidebarGroupLabel icon={FolderGit2}>Projects</SidebarGroupLabel>
              <Box role="group">
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
                    onLocateProject={(projectId) =>
                      onLocateProject?.(projectId)
                    }
                  />
                ))}
              </Box>
            </section>

            {orphanThreads.length > 0 ? (
              <section aria-label="Threads">
                <SidebarGroupLabel icon={MessageSquareText}>Threads</SidebarGroupLabel>
                <Box role="group">
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
                </Box>
              </section>
            ) : null}
          </Stack>
        )}
      </div>

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
          onClick={openInbox}
          h={{ base: 40, md: 32 }}
          style={{
            marginBottom: 4,
            display: "flex",
            width: "100%",
            alignItems: "center",
            gap: 8,
            borderRadius: "var(--mantine-radius-md)",
            padding: "0 8px",
            textAlign: "left",
            fontSize: 12,
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
            gap: 8,
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
                fontSize: 13,
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
                fontSize: 11,
                color: "var(--mantine-color-dimmed)",
              }}
            >
              {user ? `@${user.username}` : "No active session"}
            </p>
          </div>
          <ThemeToggle />
          <Menu position="top-end" width={200}>
            <Menu.Target>
              <ActionIcon
                variant="default"
                size="sm"
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
              <Menu.Item leftSection={<Inbox size={14} />} onClick={openInbox}>
                Inbox
              </Menu.Item>
              <Menu.Item
                leftSection={<Settings size={14} />}
                onClick={openSettings}
              >
                Settings
              </Menu.Item>
              <Menu.Divider />
              <Menu.Item
                leftSection={<LogOut size={14} />}
                onClick={signOut}
              >
                Reload workspace
              </Menu.Item>
            </Menu.Dropdown>
          </Menu>
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
  icon: ComponentType<{ className?: string; size?: number }>;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "8px 8px 4px",
        fontSize: 11,
        fontWeight: 500,
        textTransform: "uppercase",
        letterSpacing: "0.14em",
        color: "var(--mantine-color-dimmed)",
      }}
    >
      <Icon size={12} />
      {children}
    </div>
  );
}
