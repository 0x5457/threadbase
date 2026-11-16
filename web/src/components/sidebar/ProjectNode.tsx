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
import { useQueryClient } from "@tanstack/react-query";
import {
  IconArrowUpRight,
  IconChevronDown,
  IconChevronRight,
  IconCopy,
  IconDots,
  IconFolder,
  IconGitBranch,
  IconMessage,
  IconPencil,
  IconPin,
  IconPinFilled,
  IconPlus,
  IconSettings,
  IconTrash,
} from "@tabler/icons-react";
import { useNavigate } from "react-router";
import { Fragment, memo, useCallback, useState } from "react";
import {
  updateThread,
  type Project,
  type Subagent,
  type ThreadSummary,
} from "../../api";
import { statusTone } from "../thread/StatusDot";
import { deleteProject } from "../../api/projects";
import { pathFor } from "../../lib/router";
import { useLongPress } from "../../lib/use-long-press";
import { useOverlayBack } from "../../lib/overlay-back";
import { ActionSheet } from "../ActionSheet";


/** One-line label for a delegation. Agents usually open with the repository
 *  boilerplate (`Repo: /path. ...`), which every sibling repeats; dropping it
 *  leaves the part that actually tells the subagents apart. */
function taskLabel(subagent: Subagent): string {
  const text = (subagent.description ?? subagent.title).trim();
  return text.replace(/^repo(?: worktree)?\b[^.]*\.\s+/i, "") || text;
}

/** Depth-first order for the delegation tree; roots are subagents whose
 * parent is not part of the list (workflow containers are not stored). */
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

const ThreadRow = memo(function ThreadRow({
  thread,
  selected,
  selectedSubagentId,
  onOpen,
  onSelectSubagent,
  onLongPress,
}: {
  thread: ThreadSummary;
  selected: boolean;
  selectedSubagentId: string | null;
  onOpen: (thread: ThreadSummary) => void;
  onSelectSubagent: (thread: ThreadSummary, subagent: Subagent) => void;
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

  return (
    <Fragment>
      <NavLink
        component="button"
        className="side-nav"
        label={thread.title}
        active={selected}
        onClick={() => onOpen(thread)}
        leftSection={<IconMessage size={12} stroke={1.5} />}
        rightSection={
          thread.pendingApprovals ||
          thread.status === "running" ||
          thread.worktreePath ||
          thread.hasDraft ||
          subagents.length ? (
            <Group gap={5} wrap="nowrap">
              {thread.hasDraft ? (
                <span
                  className="thread-draft-mark"
                  title="Draft saved"
                  aria-label="Draft saved"
                >
                  <IconPencil size={11} stroke={1.5} />
                </span>
              ) : null}
              {thread.worktreePath ? (
                <span
                  className="thread-worktree-mark"
                  title="Isolated worktree"
                  aria-label="Isolated worktree"
                >
                  <IconGitBranch size={11} stroke={1.5} />
                </span>
              ) : null}
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
      {subagents.length ? (
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
      ) : null}
      {expanded && rows.length ? (
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
                <IconGitBranch size={11} stroke={1.5} className="side-subagent-icon" />
                <span className="side-subagent-title">{label}</span>
                <span className={`dot ${statusTone(subagent.status)}`} />
              </button>
            );
          })}
        </div>
      ) : null}
    </Fragment>
  );
});

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

  const pinThread = async (thread: ThreadSummary) => {
    try {
      await updateThread(thread.id, { pinned: !thread.pinned });
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
          label={project.name}
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
              {threads.map((thread) => (
                <ThreadRow
                  key={thread.id}
                  thread={thread}
                  selected={thread.id === selectedThreadId}
                  // A subagent can only be selected through its parent thread,
                  // so the other rows keep the same prop and skip their render.
                  selectedSubagentId={
                    thread.id === selectedThreadId ? selectedSubagentId : null
                  }
                  onOpen={openThread}
                  onSelectSubagent={openSubagent}
                  onLongPress={setThreadSheet}
                />
              ))}
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
