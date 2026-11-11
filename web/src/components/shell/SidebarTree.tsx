import {
  Bookmark,
  FolderGit2,
  FolderPlus,
  MessageSquareText,
} from "lucide-react";
import { Box, Skeleton, Stack, UnstyledButton } from "@mantine/core";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import type { ProjectDto } from "@/api/types/ProjectDto";
import type { SubagentDto } from "@/api/types/SubagentDto";
import type { ThreadDto } from "@/api/types/ThreadDto";
import type { UserDto } from "@/api/types/UserDto";
import { EmptyState, ErrorState } from "@/components/app";
import { ProjectNode } from "./sidebar/ProjectNode";
import { SidebarFooter } from "./sidebar/SidebarFooter";
import { SidebarHeader } from "./sidebar/SidebarHeader";
import { ThreadNode } from "./sidebar/ThreadNode";
import { SidebarGroupLabel } from "./sidebar/SidebarGroupLabel";
import { moveTreeFocus, sortThreads } from "./sidebar/treeNav";
import { rowHover } from "./sidebar/rowStyles";

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

  /**
   * Roving tabindex: the tree is one Tab stop, arrows move between rows.
   * Arrows already focus the next row; this keeps exactly one row tabbable
   * and promotes whichever row currently owns focus after a re-render.
   */
  const syncRovingRow = useCallback(() => {
    const container = treeRef.current;
    if (!container) {
      return;
    }
    const rows = Array.from(
      container.querySelectorAll<HTMLElement>("[data-tree-row]"),
    ).filter((element) => element.offsetParent !== null);
    if (rows.length === 0) {
      return;
    }
    const active =
      rows.find((row) => row === document.activeElement) ??
      rows.find((row) => row.tabIndex === 0) ??
      rows[0];
    for (const row of rows) {
      row.tabIndex = row === active ? 0 : -1;
    }
  }, []);

  useEffect(() => {
    syncRovingRow();
  }, [syncRovingRow, projects, threads, subagents, collapsedProjects, collapsedThreads]);

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
      <SidebarHeader onNewProject={onNewProject} onClose={onClose} />

      <div
        ref={treeRef}
        role="tree"
        aria-label="Projects and threads"
        onFocusCapture={syncRovingRow}
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
                  fontSize: "var(--mantine-font-size-md)",
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

      <SidebarFooter
        inboxCount={inboxCount ?? 0}
        user={user}
        userLoading={userLoading}
        onOpenInbox={openInbox}
        onOpenSettings={openSettings}
        onLogout={signOut}
      />
    </div>
  );
}
