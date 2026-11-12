import { ActionIcon, Group, Menu, NavLink, Text } from "@mantine/core";
import { useQuery } from "@tanstack/react-query";
import {
  IconDots,
  IconFolder,
  IconMessage,
  IconPlus,
  IconSettings,
} from "@tabler/icons-react";
import { useState } from "react";
import { listThreads, type Project, type ThreadSummary } from "../../api";

export function ProjectNode({
  project,
  selectedThreadId,
  activeProjectId,
  onProjectOpen,
  onNavigate,
  onSelectThread,
  onNewThread,
  onSettings,
}: {
  project: Project;
  selectedThreadId: string | null;
  activeProjectId: string | null;
  onProjectOpen: (projectId: string) => void;
  onNavigate: () => void;
  onSelectThread: (thread: ThreadSummary) => void;
  onNewThread: (project: Project) => void;
  onSettings: (project: Project) => void;
}) {
  const active = activeProjectId === project.id;
  const [opened, setOpened] = useState(active);
  const threads = useQuery({
    queryKey: ["threads", project.id],
    queryFn: () => listThreads(project.id),
    enabled: opened,
    staleTime: 5_000,
    refetchInterval: opened ? 5_000 : false,
    refetchIntervalInBackground: false,
  });

  return (
    <div className="side-project">
      <NavLink
        component="button"
        className="side-nav"
        label={project.name}
        title={project.path}
        leftSection={<IconFolder size={13} stroke={1.5} />}
        opened={opened || active}
        onChange={(next) => {
          setOpened(next);
          if (next) onProjectOpen(project.id);
        }}
        childrenOffset={12}
      >
        {threads.isPending ? (
          <NavLink
            component="button"
            className="side-nav"
            label="Loading…"
            disabled
          />
        ) : null}
        {threads.data?.threads.map((thread) => (
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
            leftSection={<IconMessage size={12} stroke={1.5} />}
            rightSection={
              thread.pendingApprovals || thread.status === "running" ? (
                <Group gap={5} wrap="nowrap">
                  {thread.pendingApprovals ? (
                    <Text size="xs" className="approval-count">
                      {thread.pendingApprovals}
                    </Text>
                  ) : null}
                  {thread.status === "running" ? (
                    <span className="dot running" />
                  ) : null}
                </Group>
              ) : null
            }
          />
        ))}
        {threads.data && threads.data.threads.length === 0 ? (
          <Text size="xs" c="dimmed" pl={10} py={4}>
            No threads yet
          </Text>
        ) : null}
      </NavLink>
      <Menu position="bottom-end" width={180} shadow="md">
        <Menu.Target>
          <ActionIcon
            className="project-menu"
            size={20}
            aria-label={`${project.name} actions`}
          >
            <IconDots size={13} stroke={1.5} />
          </ActionIcon>
        </Menu.Target>
        <Menu.Dropdown>
          <Menu.Item
            leftSection={<IconPlus size={12} stroke={1.5} />}
            onClick={() => onNewThread(project)}
          >
            New thread
          </Menu.Item>
          <Menu.Item
            leftSection={<IconSettings size={12} stroke={1.5} />}
            onClick={() => onSettings(project)}
          >
            Project settings
          </Menu.Item>
        </Menu.Dropdown>
      </Menu>
    </div>
  );
}
