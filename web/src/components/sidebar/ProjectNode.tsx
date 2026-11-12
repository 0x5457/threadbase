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
import { notifications } from "@mantine/notifications";
import { useMediaQuery } from "@mantine/hooks";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  IconArrowUpRight,
  IconCopy,
  IconDots,
  IconFolder,
  IconMessage,
  IconPin,
  IconPinFilled,
  IconPlus,
  IconSettings,
  IconTrash,
} from "@tabler/icons-react";
import { useNavigate } from "react-router";
import { useState } from "react";
import {
  listThreads,
  updateThread,
  type Project,
  type ThreadSummary,
} from "../../api";
import { deleteProject } from "../../api/projects";
import { pathFor } from "../../lib/router";
import { useLongPress } from "../../lib/use-long-press";
import { useOverlayBack } from "../../lib/overlay-back";
import { ActionSheet } from "../ActionSheet";


function ThreadRow({
  thread,
  selected,
  onOpen,
  onLongPress,
}: {
  thread: ThreadSummary;
  selected: boolean;
  onOpen: (thread: ThreadSummary) => void;
  onLongPress: (thread: ThreadSummary) => void;
}) {
  const press = useLongPress(() => onLongPress(thread));

  return (
    <NavLink
      component="button"
      className="side-nav"
      label={thread.title}
      active={selected}
      onClick={() => onOpen(thread)}
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
      {...press}
    />
  );
}

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
  // On a phone the nav is the thread browser, not a route picker: expanding a
  // project must not navigate away from the current screen, otherwise the
  // drawer's history entry points at the old route.
  const phone = useMediaQuery("(max-width: 47.99em)");
  const [opened, setOpened] = useState(active);
  const [removeOpen, setRemoveOpen] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [projectSheet, setProjectSheet] = useState(false);
  const [threadSheet, setThreadSheet] = useState<ThreadSummary | null>(null);
  const projectPress = useLongPress(() => setProjectSheet(true));
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  useOverlayBack(removeOpen, () => setRemoveOpen(false));

  const pinThread = async (thread: ThreadSummary) => {
    try {
      await updateThread(thread.id, { pinned: !thread.pinned });
      void queryClient.invalidateQueries({ queryKey: ["threads", project.id] });
      void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
      notifications.show({
        message: thread.pinned ? "Thread unpinned" : "Thread pinned",
        color: "teal",
      });
    } catch (cause) {
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not update thread",
        color: "red",
      });
    }
  };

  const copyThreadLink = (thread: ThreadSummary) => {
    const url = `${window.location.origin}${pathFor({
      kind: "thread",
      projectId: project.id,
      threadId: thread.id,
    })}`;
    void navigator.clipboard
      ?.writeText(url)
      .then(() => notifications.show({ message: "Link copied", color: "teal" }))
      .catch(() =>
        notifications.show({ message: "Could not copy link", color: "red" }),
      );
  };
  const remove = async () => {
    setRemoving(true);
    try {
      await deleteProject(project.id);
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
      void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
      void queryClient.invalidateQueries({ queryKey: ["inbox"] });
      setRemoveOpen(false);
      if (active) {
        navigate("/inbox");
        onNavigate();
      }
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
            if (next && !phone) onProjectOpen(project.id);
          }}
          childrenOffset={12}
          {...projectPress}
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
            <ThreadRow
              key={thread.id}
              thread={thread}
              selected={thread.id === selectedThreadId}
              onOpen={(next) => {
                onSelectThread(next);
                onNavigate();
              }}
              onLongPress={setThreadSheet}
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

        <ActionSheet
          opened={projectSheet}
          onClose={() => setProjectSheet(false)}
          title={project.name}
          subtitle={project.path}
          actions={[
            {
              label: "New thread",
              icon: <IconPlus size={17} stroke={1.5} />,
              onSelect: () => onNewThread(project),
            },
            {
              label: "Project settings",
              icon: <IconSettings size={17} stroke={1.5} />,
              onSelect: () => onSettings(project),
            },
            {
              label: "Remove project",
              icon: <IconTrash size={17} stroke={1.5} />,
              destructive: true,
              onSelect: () => setRemoveOpen(true),
            },
          ]}
        />

        <ActionSheet
          opened={threadSheet !== null}
          onClose={() => setThreadSheet(null)}
          title={threadSheet?.title}
          subtitle={project.name}
          actions={
            threadSheet
              ? [
                  {
                    label: "Open thread",
                    icon: <IconArrowUpRight size={17} stroke={1.5} />,
                    onSelect: () => {
                      onSelectThread(threadSheet);
                      onNavigate();
                    },
                  },
                  {
                    label: threadSheet.pinned ? "Unpin thread" : "Pin thread",
                    icon: threadSheet.pinned ? (
                      <IconPinFilled size={17} stroke={1.5} />
                    ) : (
                      <IconPin size={17} stroke={1.5} />
                    ),
                    onSelect: () => void pinThread(threadSheet),
                  },
                  {
                    label: "Copy link",
                    icon: <IconCopy size={17} stroke={1.5} />,
                    onSelect: () => copyThreadLink(threadSheet),
                  },
                ]
              : []
          }
        />

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
