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
import { useQueryClient } from "@tanstack/react-query";
import {
  IconDots,
  IconFolder,
  IconPlus,
  IconSettings,
  IconTrash,
} from "@tabler/icons-react";
import { useNavigate } from "react-router";
import { memo, useCallback, useState } from "react";
import type { Project, Subagent, ThreadSummary } from "../../api";
import { deleteProject } from "../../api/projects";
import { SidebarThreadRow, ThreadShelf } from "./ThreadRow";
import { ThreadActionSheet } from "./ThreadActionSheet";
import { useLongPress } from "../../lib/use-long-press";
import { useOverlayBack } from "../../lib/overlay-back";
import { ActionSheet } from "../ActionSheet";


export const ProjectNode = memo(function ProjectNode({
  phone,
  project,
  threads,
  threadsLoading,
  selectedThreadId,
  selectedSubagentId,
  activeProjectId,
  onProjectOpen,
  onNavigate,
  onSelectThread,
  onSelectSubagent,
  onNewThread,
  onSettings,
}: {
  /** Phone viewport: expand the project in place instead of navigating. */
  phone: boolean;
  project: Project;
  /** Threads for this project, filtered from the shell's shared thread list. */
  threads: ThreadSummary[];
  threadsLoading: boolean;
  selectedThreadId: string | null;
  selectedSubagentId: string | null;
  activeProjectId: string | null;
  onProjectOpen: (projectId: string) => void;
  onNavigate: () => void;
  onSelectThread: (thread: ThreadSummary) => void;
  onSelectSubagent: (thread: ThreadSummary, subagent: Subagent) => void;
  onNewThread: (project: Project) => void;
  onSettings: (project: Project) => void;
}) {
  const active = activeProjectId === project.id;
  const [opened, setOpened] = useState(active);
  // The active project is always expanded so its threads explain the current
  // route; the query has to follow the same state or a deep link paints an
  // open project with no rows.
  const expanded = opened || active;
  const [removeOpen, setRemoveOpen] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [projectSheet, setProjectSheet] = useState(false);
  const [threadSheet, setThreadSheet] = useState<ThreadSummary | null>(null);
  const projectPress = useLongPress(() => setProjectSheet(true));
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  useOverlayBack(removeOpen, () => setRemoveOpen(false));

  // The server decides which row a thread belongs to; the tree only renders it.
  // Pinned and working threads live in the global groups, so they are not
  // repeated here. A collapsed project still has to say that someone is waiting.
  const needsYou = threads.filter(
    (thread) =>
      thread.pendingApprovals + thread.pendingElicitations > 0 ||
      thread.status === "requires_action" ||
      thread.status === "error" ||
      thread.status === "interrupted",
  ).length;
  const activeThreads = threads.filter((thread) => thread.section === "active");
  const snoozed = threads.filter((thread) => thread.section === "snoozed");
  const settled = threads.filter((thread) => thread.section === "settled");
  const row = (thread: ThreadSummary) => (
    <SidebarThreadRow
      key={thread.id}
      thread={thread}
      selected={thread.id === selectedThreadId}
      // A subagent can only be selected through its parent thread, so the
      // other rows keep the same prop and skip their render.
      selectedSubagentId={thread.id === selectedThreadId ? selectedSubagentId : null}
      onOpen={openThread}
      onSelectSubagent={openSubagent}
      onLongPress={setThreadSheet}
    />
  );

  const remove = async () => {
    setRemoving(true);
    try {
      await deleteProject(project.id);
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
      void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
      void queryClient.invalidateQueries({ queryKey: ["inbox"] });
      void queryClient.invalidateQueries({ queryKey: ["inbox-unread"] });
      setRemoveOpen(false);
      if (active) {
        navigate("/inbox");
        onNavigate();
      }
    } finally {
      setRemoving(false);
    }
  };
  const openThread = useCallback(
    (next: ThreadSummary) => {
      onSelectThread(next);
      onNavigate();
    },
    [onNavigate, onSelectThread],
  );
  const openSubagent = useCallback(
    (next: ThreadSummary, subagent: Subagent) => {
      onSelectSubagent(next, subagent);
      onNavigate();
    },
    [onNavigate, onSelectSubagent],
  );

  return (
    <div className="side-project">
      <div className="side-project-head">
        <NavLink
          component="button"
          className="side-nav"
          label={
            <span className="side-project-label">
              <span className="side-project-name">{project.name}</span>
              {needsYou ? (
                <span
                  className="side-needs"
                  title={`${needsYou} thread${needsYou === 1 ? "" : "s"} need you`}
                >
                  {needsYou}
                </span>
              ) : null}
            </span>
          }
          title={project.path}
          leftSection={<IconFolder size={13} stroke={1.5} />}
          opened={expanded}
          onChange={(next) => {
            setOpened(next);
            if (next && !phone) onProjectOpen(project.id);
          }}
          childrenOffset={12}
          {...projectPress}
        >
          {expanded ? (
            <>
              {threadsLoading && threads.length === 0 ? (
                <NavLink
                  component="button"
                  className="side-nav"
                  label="Loading…"
                  disabled
                />
              ) : null}
              {activeThreads.map(row)}
              <ThreadShelf
                shelfKey={`${project.id}:snoozed`}
                label="Snoozed"
                count={snoozed.length}
              >
                {snoozed.map(row)}
              </ThreadShelf>
              <ThreadShelf
                shelfKey={`${project.id}:settled`}
                label="Settled"
                count={settled.length}
              >
                {settled.map(row)}
              </ThreadShelf>
              {!threadsLoading && threads.length === 0 ? (
                <Text size="xs" c="dimmed" pl={10} py={4}>
                  No threads yet
                </Text>
              ) : null}
            </>
          ) : (
            // Mantine only treats the row as expandable when `children` is
            // present; keep a placeholder while collapsed, and mount the rows
            // only for the project the user actually opened.
            <span />
          )}
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

        <ThreadActionSheet
          thread={threadSheet}
          projectId={project.id}
          subtitle={project.name}
          opened={threadSheet !== null}
          onClose={() => setThreadSheet(null)}
          onOpen={(thread) => {
            setThreadSheet(null);
            openThread(thread);
          }}
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
            <Text size="sm" className="section-label">
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
});
