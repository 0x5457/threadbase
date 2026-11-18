import {
  ActionIcon,
  AppShell,
  Avatar,
  Box,
  Group,
  NavLink,
  ScrollArea,
  Text,
} from "@mantine/core";
import { useDisclosure } from "@mantine/hooks";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  IconFolderPlus,
  IconInbox,
  IconLayoutList,
  IconLogout,
  IconPlus,
  IconSettings,
} from "@tabler/icons-react";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import {
  getMe,
  logout,
  listAllThreads,
  listInbox,
  listProjects,
  type Project,
  type ThreadSummary,
} from "../api";
import { AddProjectModal } from "./AddProjectModal";
import { NewThreadModal } from "./NewThreadModal";
import { ProjectSettingsModal } from "./ProjectSettingsModal";
import { initials } from "../lib/initials";
import { ProjectNode } from "./sidebar/ProjectNode";
import { SidebarThreadRow } from "./sidebar/ThreadRow";
import { ThreadActionSheet } from "./sidebar/ThreadActionSheet";

/** Stable empty list for projects without threads. */
const EMPTY_THREADS: ThreadSummary[] = [];

interface SidebarProps {
  /** Phone viewport: project rows expand in place instead of navigating. */
  phone: boolean;
  onNavigate: () => void;
  onOpenInbox: () => void;
  onOpenSessions: () => void;
  onOpenSettings: () => void;
  showSessions: boolean;
  activeView: "inbox" | "sessions" | "thread" | "settings" | "project";
  selectedThreadId: string | null;
  selectedSubagentId: string | null;
  activeProjectId: string | null;
  onSelectThread: (thread: ThreadSummary) => void;
  onSelectSubagent: (
    thread: ThreadSummary,
    subagent: ThreadSummary["subagents"][number],
  ) => void;
  onSelectProject: (projectId: string) => void;
}

export const Sidebar = memo(function Sidebar({
  phone,
  onNavigate,
  onOpenInbox,
  onOpenSessions,
  onOpenSettings,
  showSessions,
  activeView,
  selectedThreadId,
  selectedSubagentId,
  activeProjectId,
  onSelectThread,
  onSelectSubagent,
  onSelectProject,
}: SidebarProps) {
  const queryClient = useQueryClient();
  const [addOpen, add] = useDisclosure(false);
  const [added, setAdded] = useState<Project[]>([]);
  const [newThreadProject, setNewThreadProject] = useState<Project | null>(null);
  const [settingsProject, setSettingsProject] = useState<Project | null>(null);

  const me = useQuery({
    queryKey: ["me"],
    queryFn: getMe,
    staleTime: Infinity,
  });
  const currentUser = me.data?.user;
  const projectsQuery = useQuery({
    queryKey: ["projects"],
    queryFn: listProjects,
    retry: false,
    staleTime: 10_000,
  });
  const allThreads = useQuery({
    queryKey: ["threads-all"],
    queryFn: listAllThreads,
    retry: false,
    staleTime: 10_000,
  });
  // Shares the shell's cache entry and polling cadence: a second observer
  // here used to force the whole tree to re-render every five seconds.
  const inbox = useQuery({
    queryKey: ["inbox-unread"],
    queryFn: () => listInbox("unread", undefined, 1),
    retry: false,
    staleTime: 5_000,
  });

  const realProjects = useMemo(() => {
    const seen = new Set<string>();
    return [...(projectsQuery.data?.projects ?? []), ...added].filter(
      (project) => {
        if (seen.has(project.id)) return false;
        seen.add(project.id);
        return true;
      },
    );
  }, [projectsQuery.data, added]);

  // The server decides the row; pinned work is lifted out of its project so it is
  // not rendered twice. Per-project array identity is preserved so a status
  // change in one project does not re-render the others.
  const pinned = useMemo(
    () => (allThreads.data?.threads ?? []).filter((thread) => thread.section === "pinned"),
    [allThreads.data],
  );
  const groupedThreadsRef = useRef<Map<string, ThreadSummary[]>>(new Map());
  const projectThreads = useMemo(() => {
    const next = new Map<string, ThreadSummary[]>();
    for (const thread of allThreads.data?.threads ?? []) {
      if (thread.section === "pinned") continue;
      const list = next.get(thread.projectId);
      if (list) list.push(thread);
      else next.set(thread.projectId, [thread]);
    }
    for (const [id, list] of next) {
      const previous = groupedThreadsRef.current.get(id);
      if (
        previous &&
        previous.length === list.length &&
        previous.every((thread, index) => thread === list[index])
      ) {
        next.set(id, previous);
      }
    }
    return next;
  }, [allThreads.data]);
  // Persist the identity map after render; mutating a ref inside `useMemo` makes
  // render impure and was the last blocker in the React Doctor gate.
  useEffect(() => {
    groupedThreadsRef.current = projectThreads;
  }, [projectThreads]);
  const projectName = useMemo(() => {
    const names = new Map(
      (projectsQuery.data?.projects ?? []).map((project) => [project.id, project.name]),
    );
    return (projectId: string) => names.get(projectId) ?? "";
  }, [projectsQuery.data]);
  const [pinnedSheet, setPinnedSheet] = useState<ThreadSummary | null>(null);
  const unread = inbox.data?.unread ?? 0;

  return (
    <AppShell.Navbar p={0} className="sidebar">
      <ScrollArea className="sidebar-scroll" type="hover">
        <div className="sidebar-inner">
          <Group className="brand-row" justify="space-between" wrap="nowrap">
            <Group gap={7} wrap="nowrap">
              <span className="brand-mark">t</span>
              <Text size="sm" fw={500}>
                Threadbase
              </Text>
            </Group>
            <ActionIcon
              size={20}
              aria-label="Add project"
              onClick={add.open}
            >
              <IconPlus size={13} stroke={1.5} />
            </ActionIcon>
          </Group>

          <NavLink
            component="button"
            className="side-nav"
            label="Inbox"
            active={activeView === "inbox"}
            onClick={() => {
              onOpenInbox();
              onNavigate();
            }}
            leftSection={<IconInbox size={14} stroke={1.5} />}
            rightSection={
              unread ? (
                <Text size="xs" c="dimmed" className="inbox-count">
                  {unread}
                </Text>
              ) : null
            }
          />

          {showSessions ? (
            <NavLink
              component="button"
              className="side-nav"
              label="Sessions"
              active={activeView === "sessions"}
              onClick={() => {
                onOpenSessions();
                onNavigate();
              }}
              leftSection={<IconLayoutList size={14} stroke={1.5} />}
            />
          ) : null}

          {pinned.length ? (
            <div className="section">
              <Group className="section-head" justify="space-between">
                <Text size="sm" className="section-label">
                  Pinned
                </Text>
              </Group>
              {pinned.map((thread) => (
                <SidebarThreadRow
                  key={thread.id}
                  thread={thread}
                  selected={thread.id === selectedThreadId}
                  selectedSubagentId={
                    thread.id === selectedThreadId ? selectedSubagentId : null
                  }
                  projectName={projectName(thread.projectId)}
                  onOpen={(next) => {
                    onSelectThread(next);
                    onNavigate();
                  }}
                  onSelectSubagent={(next, subagent) => {
                    onSelectSubagent(next, subagent);
                    onNavigate();
                  }}
                  onLongPress={setPinnedSheet}
                />
              ))}
            </div>
          ) : null}

          <div className="section">
            <Group className="section-head" justify="space-between">
              <Text size="sm" className="section-label">
                Projects
              </Text>
            </Group>
            {realProjects.map((project) => (
              <ProjectNode
                key={project.id}
                phone={phone}
                project={project}
                threads={projectThreads.get(project.id) ?? EMPTY_THREADS}
                threadsLoading={allThreads.isLoading}
                selectedThreadId={selectedThreadId}
                selectedSubagentId={selectedSubagentId}
                activeProjectId={activeProjectId}
                onProjectOpen={onSelectProject}
                onNavigate={onNavigate}
                onSelectThread={onSelectThread}
                onSelectSubagent={onSelectSubagent}
                onNewThread={setNewThreadProject}
                onSettings={setSettingsProject}
              />
            ))}
            {!projectsQuery.isPending && realProjects.length === 0 ? (
              <Text size="xs" c="dimmed" pl={10} py={4}>
                No projects yet
              </Text>
            ) : null}
            <NavLink
              component="button"
              className="side-nav add-project"
              label="Add project"
              onClick={add.open}
              leftSection={<IconFolderPlus size={13} stroke={1.5} />}
            />
          </div>

        </div>
      </ScrollArea>

      <Group className="sidebar-footer" gap={7} wrap="nowrap">
        <Avatar
          size={24}
          radius="xl"
          color="brand"
          src={currentUser?.avatarUrl ?? undefined}
        >
          {initials(currentUser?.displayName ?? currentUser?.username ?? "You")}
        </Avatar>
        <Box className="meta">
          <Text size="sm" lh={1.2}>
            {currentUser?.displayName ?? "Account"}
          </Text>
          <Text size="xs" c="dimmed" lh={1.2}>
            @{currentUser?.username ?? "you"}
          </Text>
        </Box>
        <ActionIcon
          aria-label="Settings"
          onClick={() => {
            onOpenSettings();
            onNavigate();
          }}
        >
          <IconSettings size={14} stroke={1.5} />
        </ActionIcon>
        <ActionIcon
          aria-label="Sign out"
          onClick={() => {
            void logout().then(() => {
              queryClient.clear();
              window.location.href = "/";
            });
          }}
        >
          <IconLogout size={14} stroke={1.5} />
        </ActionIcon>
      </Group>

      <NewThreadModal
        opened={newThreadProject !== null}
        project={newThreadProject}
        onClose={() => setNewThreadProject(null)}
        onCreated={(thread) => {
          void queryClient.invalidateQueries({ queryKey: ["projects"] });
          void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
          onSelectThread(thread);
          onNavigate();
          setNewThreadProject(null);
        }}
      />

      <ProjectSettingsModal
        opened={settingsProject !== null}
        project={settingsProject}
        onClose={() => setSettingsProject(null)}
        onSaved={(project) => {
          setAdded((current) =>
            current.map((item) =>
              item.id === project.id ? project : item,
            ),
          );
          setSettingsProject(null);
        }}
      />

      <ThreadActionSheet
        thread={pinnedSheet}
        projectId={pinnedSheet?.projectId ?? ""}
        subtitle={pinnedSheet ? projectName(pinnedSheet.projectId) : undefined}
        opened={pinnedSheet !== null}
        onClose={() => setPinnedSheet(null)}
        onOpen={(thread) => {
          setPinnedSheet(null);
          onSelectThread(thread);
          onNavigate();
        }}
      />

      <AddProjectModal
        opened={addOpen}
        onClose={add.close}
        onAdded={(project) => setAdded((current) => [...current, project])}
      />
    </AppShell.Navbar>
  );
});
