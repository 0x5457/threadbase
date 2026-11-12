import {
  ActionIcon,
  Button,
  Group,
  Menu,
  Modal,
  NavLink,
  Stack,
  Text,
} from "@mantine/core";
import { useQuery } from "@tanstack/react-query";
import {
  IconDots,
  IconFolder,
  IconMessage,
  IconPlus,
  IconSettings,
  IconTrash,
} from "@tabler/icons-react";
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { listThreads, type Project, type ThreadSummary } from "../../api";
import { deleteProject } from "../../api/projects";
import { navigate } from "../../lib/router";

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
  const [removeOpen, setRemoveOpen] = useState(false);
  const [removing, setRemoving] = useState(false);
  const queryClient = useQueryClient();
  const remove = async () => {
    setRemoving(true);
    try {
      await deleteProject(project.id);
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
      void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
      void queryClient.invalidateQueries({ queryKey: ["inbox"] });
      setRemoveOpen(false);
      if (active) navigate("/inbox");
    } finally {
      setRemoving(false);
    }
  };
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
      <div className="side-project-head">
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
            <Menu.Divider />
            <Menu.Item
              color="red"
              leftSection={<IconTrash size={12} stroke={1.5} />}
              onClick={() => setRemoveOpen(true)}
            >
              Remove project
            </Menu.Item>
          </Menu.Dropdown>
        </Menu>

        <Modal
          opened={removeOpen}
          onClose={() => setRemoveOpen(false)}
          title={`Remove ${project.name}?`}
          size="md"
          centered
        >
          <Stack gap="sm">
            <Text size="sm">
              This removes the project from Threadbase together with its
              threads, their worktrees and their branches.
            </Text>
            <Text size="sm" c="dimmed">
              The repository folder itself stays where it is:{" "}
              <Text component="span" ff="monospace" size="xs">
                {project.path}
              </Text>
            </Text>
            <Group justify="flex-end" gap="xs" mt={4}>
              <Button variant="default" onClick={() => setRemoveOpen(false)}>
                Cancel
              </Button>
              <Button
                color="red"
                loading={removing}
                onClick={() => void remove()}
              >
                Remove project
              </Button>
            </Group>
          </Stack>
        </Modal>
      </div>
    </div>
  );
}
