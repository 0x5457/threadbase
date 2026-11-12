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
  IconLogout,
  IconMessage,
  IconPlus,
  IconSettings,
  IconTerminal2,
} from "@tabler/icons-react";
import { useMemo, useState } from "react";
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

interface SidebarProps {
  onNavigate: () => void;
  onOpenInbox: () => void;
  onOpenSettings: () => void;
  onOpenTerminal: () => void;
  canTerminal: boolean;
  activeView: "inbox" | "thread" | "settings" | "project";
  selectedThreadId: string | null;
  activeProjectId: string | null;
  onSelectThread: (thread: ThreadSummary) => void;
  onSelectProject: (projectId: string) => void;
}

export function Sidebar({
  onNavigate,
  onOpenInbox,
  onOpenSettings,
  onOpenTerminal,
  canTerminal,
  activeView,
  selectedThreadId,
  activeProjectId,
  onSelectThread,
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
    staleTime: 5_000,
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
  });
  const inbox = useQuery({
    queryKey: ["inbox", "unread"],
    queryFn: () => listInbox("unread"),
    retry: false,
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
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

  const pinned = (allThreads.data?.threads ?? []).filter(
    (thread) => thread.pinned,
  );
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

          {pinned.length ? (
            <div className="section">
              <Group className="section-head" justify="space-between">
                <Text size="sm" className="section-label">
                  Pinned
                </Text>
              </Group>
              {pinned.map((thread) => (
                <NavLink
                  key={thread.id}
                  component="button"
                  className="side-nav"
                  label={thread.title}
                  active={thread.id === selectedThreadId}
                  onClick={() => {
                    onSelectThread(thread);
                    onNavigate();
                  }}
                  leftSection={<IconMessage size={13} stroke={1.5} />}
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
                project={project}
                selectedThreadId={selectedThreadId}
                activeProjectId={activeProjectId}
                onProjectOpen={onSelectProject}
                onNavigate={onNavigate}
                onSelectThread={onSelectThread}
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
        {canTerminal ? (
          <ActionIcon
            aria-label="Terminal"
            title="Terminal (Ctrl+`)"
            onClick={() => {
              onOpenTerminal();
              onNavigate();
            }}
          >
            <IconTerminal2 size={14} stroke={1.5} />
          </ActionIcon>
        ) : null}
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
          void queryClient.invalidateQueries({
            queryKey: ["threads", thread.projectId],
          });
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

      <AddProjectModal
        opened={addOpen}
        onClose={add.close}
        onAdded={(project) => setAdded((current) => [...current, project])}
      />
    </AppShell.Navbar>
  );
}
