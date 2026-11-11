import {
  AlertCircle,
  Archive,
  ArrowLeft,
  Bot,
  ChevronRight,
  CircleHelp,
  MoreHorizontal,
  ShieldCheck,
  WifiOff,
} from "lucide-react";
import {
  ActionIcon,
  Anchor,
  Badge,
  Box,
  Button,
  Group,
  Menu,
  Modal,
  ScrollArea,
  Skeleton,
  Stack,
  Text,
  TextInput,
  VisuallyHidden,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import {
  cancelSubagent,
  decideApproval,
  deleteThread,
  forkThread,
  markThreadRead,
  markThreadUnread,
  restartAgent,
  requestAccess,
  respondElicitation,
  revertThread,
  updateThread,
} from "@/api/endpoints";
import {
  queryKeys,
  useMe,
  useThreadBundle,
  useThreadEvents,
  useThreadState,
} from "@/api/queries";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { CommentAnchor } from "@/api/types/CommentAnchor";
import type { ElicitationAction } from "@/api/types/ElicitationAction";
import { CommentDialog } from "@/components/thread/CommentDialog";
import {
  SelectionComment,
  type SelectionTarget,
} from "@/components/thread/SelectionComment";
import { Composer } from "@/components/thread/Composer";
import { ThreadStatusBadge } from "@/components/thread/ThreadStatusBadge";
import { SubagentView } from "@/components/thread/SubagentView";
import { ThreadTimeline } from "@/components/thread/ThreadTimeline";
import { ErrorState } from "@/components/app";
import { useOnlineStatus } from "@/lib/hooks";
import { useLiveStore, useThreadLiveEvents } from "@/lib/live";

export function ThreadPage() {
  const { projectId, threadId, subagentId } = useParams();
  const online = useOnlineStatus();
  const me = useMe();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const bundle = useThreadBundle(threadId);
  const events = useThreadEvents(threadId);
  const threadState = useThreadState(threadId);
  useThreadLiveEvents(threadId);
  const liveConnected = useLiveStore((state) => state.connected);
  const elicitation = useMutation({
    mutationFn: ({
      elicitationId,
      action,
      values,
    }: {
      elicitationId: string;
      action: ElicitationAction;
      values: Record<string, unknown>;
    }) => respondElicitation(elicitationId, action, values),
    onSuccess: () => {
      if (!threadId) {
        return;
      }
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadEvents(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadBundle(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.inbox("unread"),
      });
    },
  });
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      if (
        modifier &&
        event.altKey &&
        event.key.toLowerCase() === "u" &&
        subagentId &&
        projectId &&
        threadId
      ) {
        event.preventDefault();
        navigate(`/p/${projectId}/t/${threadId}`);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navigate, projectId, subagentId, threadId]);

  const [commentAnchor, setCommentAnchor] = useState<CommentAnchor | null>(null);
  const cancelSubagentMutation = useMutation({
    mutationFn: (subagentId: string) =>
      cancelSubagent(threadId ?? "", subagentId),
    onSuccess: () => {
      if (threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadEvents(threadId),
        });
      }
    },
  });
  const [commentOpen, setCommentOpen] = useState(false);
  const [selectionTarget, setSelectionTarget] = useState<SelectionTarget | null>(
    null,
  );
  useEffect(() => {
    const onMouseUp = () => {
      const selection = window.getSelection();
      if (!selection || selection.isCollapsed) {
        return;
      }
      const text = selection.toString().trim();
      if (!text) {
        return;
      }
      const node = selection.anchorNode;
      const element =
        node instanceof Element ? node : node?.parentElement ?? null;
      const message = element?.closest("[data-message-id]");
      const messageId = message?.getAttribute("data-message-id");
      if (!messageId) {
        return;
      }
      const rect = selection.getRangeAt(0).getBoundingClientRect();
      setSelectionTarget({ messageId, x: rect.left, y: rect.bottom });
    };
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      if (modifier && event.altKey && event.key.toLowerCase() === "m") {
        event.preventDefault();
        const node = window.getSelection()?.anchorNode;
        const element =
          node instanceof Element ? node : node?.parentElement ?? null;
        const message = element?.closest("[data-message-id]");
        const messageId = message?.getAttribute("data-message-id");
        if (messageId) {
          setSelectionTarget({
            messageId,
            x: window.innerWidth / 2,
            y: window.innerHeight / 2,
          });
        } else {
          setCommentAnchor({ kind: "thread" });
          setCommentOpen(true);
        }
      }
    };
    document.addEventListener("mouseup", onMouseUp);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mouseup", onMouseUp);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, []);

  useEffect(() => {
    if (!threadId) {
      return;
    }
    void markThreadRead(threadId)
      .then(() => {
        void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
      })
      .catch(() => undefined);
  }, [threadId, queryClient]);

  const archive = useMutation({
    mutationFn: (archived: boolean) =>
      updateThread(threadId ?? "", {
        title: null,
        pinned: null,
        archived,
        shareLevel: null,
        model: null,
        effort: null,
      }),
    onSuccess: (response) => {
      if (!threadId) {
        return;
      }
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadBundle(threadId),
      });
      void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectThreads(response.thread.projectId, false),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectThreads(response.thread.projectId, true),
      });
      if (response.thread.status === "archived") {
        navigate(`/p/${response.thread.projectId}`);
      }
    },
  });

  useEffect(() => {
    if (!threadId) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      const status =
        threadState.data?.status ?? bundle.data?.bundle.thread.status ?? "idle";
      const members = bundle.data?.bundle.members ?? [];
      const role = members.find(
        (member) => member.user.id === me.data?.user.id,
      )?.role;
      const canWrite =
        Boolean(me.data?.user.isAdmin) ||
        role === "owner" ||
        role === "editor";
      if (
        modifier &&
        !event.shiftKey &&
        !event.altKey &&
        event.key.toLowerCase() === "w"
      ) {
        event.preventDefault();
        if (canWrite && !archive.isPending) {
          archive.mutate(status !== "archived");
        }
        return;
      }
      if (
        modifier &&
        event.shiftKey &&
        !event.altKey &&
        event.key.toLowerCase() === "u"
      ) {
        event.preventDefault();
        const unread = bundle.data?.bundle.thread.unread ?? false;
        const mutation = unread
          ? markThreadRead(threadId)
          : markThreadUnread(threadId);
        void mutation
          .then(() => {
            void queryClient.invalidateQueries({
              queryKey: queryKeys.threads,
              exact: true,
            });
            void queryClient.invalidateQueries({
              queryKey: queryKeys.threadBundle(threadId),
            });
          })
          .catch(() => undefined);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [threadId, archive, bundle.data, threadState.data, me.data, queryClient]);

  const [divergedDismissed, setDivergedDismissed] = useState(false);
  const [renameOpen, setRenameOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [renameValue, setRenameValue] = useState("");
  const rename = useMutation({
    mutationFn: () =>
      updateThread(threadId ?? "", {
        title: renameValue.trim(),
        pinned: null,
        archived: null,
        shareLevel: null,
        model: null,
        effort: null,
      }),
    onSuccess: () => {
      setRenameOpen(false);
      if (threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
        void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
        if (projectId) {
          void queryClient.invalidateQueries({
            queryKey: queryKeys.projectThreads(projectId, false),
          });
          void queryClient.invalidateQueries({
            queryKey: queryKeys.projectThreads(projectId, true),
          });
        }
      }
    },
  });
  const discardDiverged = useMutation({
    mutationFn: () => revertThread(threadId ?? ""),
    onSuccess: () => {
      setDivergedDismissed(true);
      if (threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadEvents(threadId),
        });
      }
    },
    onError: (error) =>
      notifications.show({
        message: String(error),
        color: "red",
      }),
  });
  const fork = useMutation({
    mutationFn: () => forkThread(threadId ?? "", null),
    onSuccess: (response) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectThreads(response.thread.projectId, false),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectThreads(response.thread.projectId, true),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectOverview(response.thread.projectId),
      });
      navigate(
        `/p/${encodeURIComponent(response.thread.projectId)}/t/${encodeURIComponent(
          response.thread.id,
        )}`,
      );
    },
  });
  const remove = useMutation({
    mutationFn: () => deleteThread(threadId ?? "", true),
    onSuccess: () => {
      if (projectId) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.projectThreads(projectId, false),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.projectThreads(projectId, true),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.projectOverview(projectId),
        });
        navigate(`/p/${encodeURIComponent(projectId)}`);
      }
    },
  });
  const restart = useMutation({
    mutationFn: () => restartAgent(threadId ?? ""),
    onSuccess: () => {
      if (threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadEvents(threadId),
        });
      }
    },
  });
  const configure = useMutation({
    mutationFn: ({ model, effort }: { model: string; effort: string }) =>
      updateThread(threadId ?? "", {
        title: null,
        pinned: null,
        archived: null,
        shareLevel: null,
        model: model.trim() || null,
        effort: effort.trim() || null,
      }),
    onSuccess: () => {
      if (threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(threadId),
        });
        void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
      }
    },
  });
  const requestAccessMutation = useMutation({
    mutationFn: () => requestAccess(threadId ?? ""),
    onSuccess: () =>
      notifications.show({
        message: "Access request sent",
        color: "green",
      }),
    onError: (error) =>
      notifications.show({
        message: String(error),
        color: "red",
      }),
  });
  const decision = useMutation({
    mutationFn: ({
      approvalId,
      value,
      note,
    }: {
      approvalId: string;
      value: ApprovalDecision;
      note?: string;
    }) => decideApproval(threadId ?? "", approvalId, value, note ?? null),
    onSuccess: () => {
      if (!threadId) {
        return;
      }
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadBundle(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadEvents(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.inbox("unread"),
      });
    },
  });

  if (!projectId || !threadId) {
    return (
      <Stack align="center" justify="center" h="100%" p="lg">
        <ErrorState
          title="Thread not found"
          description="The URL does not contain a project id and thread id."
        />
      </Stack>
    );
  }

  if (bundle.isLoading) {
    return (
      <Stack h="100%" gap={0}>
        <Box
          px="md"
          py={6}
          style={{ borderBottom: "1px solid var(--mantine-color-default-border)" }}
        >
          <Skeleton h={16} w={128} />
        </Box>
        <ScrollArea style={{ flex: 1, minHeight: 0 }}>
          <Box
            maw={768}
            mx="auto"
            px="md"
            py={{ base: "md", sm: "lg" }}
            w="100%"
          >
            <Stack gap="sm">
              <Skeleton h={64} w="80%" />
              <Skeleton h={112} />
              <Skeleton h={64} w="66%" />
            </Stack>
          </Box>
        </ScrollArea>
      </Stack>
    );
  }

  if (bundle.isError || !bundle.data) {
    return (
      <Stack align="center" justify="center" h="100%" p="lg">
        <ErrorState
          error={bundle.error}
          title="Cannot open thread"
          onRetry={() => void bundle.refetch()}
          action={
            <Anchor
              component={Link}
              to={`/p/${encodeURIComponent(projectId)}`}
              size="sm"
              c="dimmed"
            >
              <Group gap={4}>
                <ArrowLeft size={12} aria-hidden />
                Back to project
              </Group>
            </Anchor>
          }
        />
      </Stack>
    );
  }

  const { thread, project, approvals, subagents, members } = bundle.data.bundle;
  // Shared projection is authoritative; the bundle is only the initial fallback.
  const liveStatus = threadState.data?.status ?? thread.status;
  const livePendingApprovals = threadState.data?.pendingApprovals ?? thread.pendingApprovals;
  const liveQueuedInputs = threadState.data?.queuedInputs ?? thread.queuedInputs;
  const livePendingElicitations = threadState.data?.pendingElicitations ?? 0;
  const liveDiverged = threadState.data?.diverged ?? thread.diverged;
  const myRole = members.find(
    (member) => member.user.id === me.data?.user.id,
  )?.role;
  const canWrite =
    Boolean(me.data?.user.isAdmin) || myRole === "owner" || myRole === "editor";

  const onDecide = (
    approvalId: string,
    value: ApprovalDecision,
    note?: string,
  ) => decision.mutate({ approvalId, value, note });
  const selectedSubagent = subagentId
    ? subagents.find((subagent) => subagent.id === subagentId)
    : undefined;
  const threadEvents = events.data?.events ?? [];
  const visibleEvents = selectedSubagent
    ? threadEvents.filter((event) => event.subagentId === selectedSubagent.id)
    : threadEvents;
  const visibleApprovals = selectedSubagent
    ? approvals.filter(
        (approval) => approval.subagentId === selectedSubagent.id,
      )
    : approvals;

  return (
    <Stack h="100%" gap={0}>
      {!online || !liveConnected ? (
        <Group
          gap="xs"
          px="sm"
          py={4}
          wrap="nowrap"
          style={{
            borderBottom: "1px solid var(--mantine-color-yellow-filled)",
            background: "var(--mantine-color-yellow-light)",
          }}
        >
          <WifiOff size={14} aria-hidden style={{ flexShrink: 0 }} />
          <Text size="sm" c="yellow">
            Reconnecting… Input is preserved while the connection is offline.
          </Text>
        </Group>
      ) : null}
      {events.data?.resync ? (
        <Group
          gap="xs"
          px="sm"
          py={4}
          wrap="nowrap"
          style={{
            borderBottom: "1px solid var(--mantine-color-blue-filled)",
            background: "var(--mantine-color-blue-light)",
          }}
        >
          <AlertCircle size={14} aria-hidden style={{ flexShrink: 0 }} />
          <Text size="sm" c="blue">
            Event history was compacted. The latest window has been loaded.
          </Text>
        </Group>
      ) : null}

      <Group
        gap="xs"
        px="md"
        py={6}
        wrap="nowrap"
        style={{ borderBottom: "1px solid var(--mantine-color-default-border)" }}
      >
        {/* On phones this row is the breadcrumb back to the project; the
            thread title itself already sits in the title bar. */}
        <Anchor
          component={Link}
          to={`/p/${encodeURIComponent(project.id)}`}
          size="sm"
          c="dimmed"
          maw={180}
          truncate
        >
          {project.name}
        </Anchor>
        <Box visibleFrom="sm" style={{ display: "flex", flexShrink: 0 }}>
          <ChevronRight size={12} aria-hidden />
        </Box>
        {selectedSubagent ? (
          <>
            <Anchor
              component={Link}
              to={`/p/${encodeURIComponent(project.id)}/t/${encodeURIComponent(
                thread.id,
              )}`}
              size="sm"
              c="dimmed"
              maw={180}
              visibleFrom="sm"
            >
              {thread.title}
            </Anchor>
            <Box visibleFrom="sm" style={{ display: "flex", flexShrink: 0 }}>
              <ChevronRight size={12} aria-hidden />
            </Box>
            <Group gap={4} style={{ minWidth: 0, flex: 1 }}>
              <Bot size={12} aria-hidden style={{ flexShrink: 0 }} />
              <Text size="sm" truncate="end">
                {selectedSubagent.title}
              </Text>
            </Group>
          </>
        ) : (
          <Text
            size="sm"
            truncate="end"
            visibleFrom="sm"
            style={{ minWidth: 0, flex: 1 }}
          >
            {thread.title}
          </Text>
        )}
        <ThreadStatusBadge status={liveStatus} />
        <Group gap={4} style={{ marginLeft: "auto", flexShrink: 0 }} wrap="nowrap">
          {liveStatus === "error" || liveStatus === "interrupted" ? (
            <Button
              variant="light"
              color="red"
              size="xs"
              visibleFrom="sm"
              loading={restart.isPending}
              disabled={!canWrite}
              onClick={() => restart.mutate()}
            >
              Restart agent
            </Button>
          ) : null}
          <ActionIcon
            variant="subtle"
            color="gray"
            size="md"
            visibleFrom="sm"
            aria-label={
              liveStatus === "archived" ? "Unarchive thread" : "Archive thread"
            }
            disabled={!canWrite || archive.isPending}
            onClick={() => archive.mutate(liveStatus !== "archived")}
          >
            <Archive size={14} />
          </ActionIcon>
          <Menu position="bottom-end">
            <Menu.Target>
              <ActionIcon
                variant="subtle"
                color="gray"
                size="md"
                aria-label="Thread actions"
              >
                <MoreHorizontal size={14} />
              </ActionIcon>
            </Menu.Target>
            <Menu.Dropdown>
              <Menu.Item
                onClick={() => {
                  setRenameValue(thread.title);
                  setRenameOpen(true);
                }}
              >
                Rename
              </Menu.Item>
              <Menu.Item onClick={() => fork.mutate()}>Fork thread</Menu.Item>
              {liveStatus === "error" || liveStatus === "interrupted" ? (
                <Menu.Item
                  hiddenFrom="sm"
                  disabled={!canWrite || restart.isPending}
                  onClick={() => restart.mutate()}
                >
                  Restart agent
                </Menu.Item>
              ) : null}
              <Menu.Item
                hiddenFrom="sm"
                disabled={!canWrite || archive.isPending}
                onClick={() => archive.mutate(liveStatus !== "archived")}
              >
                {liveStatus === "archived" ? "Unarchive thread" : "Archive thread"}
              </Menu.Item>
              <Menu.Divider />
              <Menu.Item color="red" onClick={() => setDeleteOpen(true)}>
                Delete thread…
              </Menu.Item>
            </Menu.Dropdown>
          </Menu>
          {livePendingApprovals > 0 ? (
            <Badge
              variant="light"
              color="yellow"
              size="xs"
              leftSection={<ShieldCheck size={12} aria-hidden />}
            >
              <Box component="span" visibleFrom="sm">
                {livePendingApprovals} waiting
              </Box>
              <Box component="span" hiddenFrom="sm">
                {livePendingApprovals}
              </Box>
            </Badge>
          ) : null}
          {livePendingElicitations > 0 ? (
            <Badge
              variant="light"
              color="blue"
              size="xs"
              leftSection={<CircleHelp size={12} aria-hidden />}
            >
              <Box component="span" visibleFrom="sm">
                {livePendingElicitations} question
                {livePendingElicitations > 1 ? "s" : ""}
              </Box>
              <Box component="span" hiddenFrom="sm">
                {livePendingElicitations}
              </Box>
            </Badge>
          ) : null}
          {liveQueuedInputs > 0 ? (
            <Badge variant="light" color="blue" size="xs" visibleFrom="sm">
              {liveQueuedInputs} queued
            </Badge>
          ) : null}
          {threadState.data?.currentTurn ? (
            <Badge
              variant="outline"
              color="gray"
              size="xs"
              visibleFrom="sm"
              title={`Current turn ${threadState.data.currentTurn}`}
              leftSection={
                <span
                  aria-hidden
                  style={{
                    width: 6,
                    height: 6,
                    borderRadius: "50%",
                    background: "currentColor",
                    animation: "pulse 2s infinite",
                  }}
                />
              }
            >
              live turn
            </Badge>
          ) : null}
        </Group>
      </Group>

      <ScrollArea style={{ flex: 1, minHeight: 0 }}>
        {selectedSubagent ? (
          <SubagentView
            subagent={selectedSubagent}
            threadId={thread.id}
            threadTitle={thread.title}
            projectId={project.id}
            events={visibleEvents}
            approvals={visibleApprovals}
            loading={events.isLoading}
            error={events.error}
            onRetry={() => void events.refetch()}
            onCancel={() => cancelSubagentMutation.mutate(selectedSubagent.id)}
          />
        ) : (
          <ThreadTimeline
            events={visibleEvents}
            approvals={visibleApprovals}
            activeSubagentCount={threadState.data?.activeSubagents ?? 0}
            loading={events.isLoading}
            error={events.error}
            onRetry={() => void events.refetch()}
            onDecide={onDecide}
            onRespondElicitation={(elicitationId, action, values) =>
              elicitation.mutate({ elicitationId, action, values })
            }
            onComment={(anchor) => {
              setCommentAnchor(anchor);
              setCommentOpen(true);
            }}
            onOpenSubagent={(subagentId) => {
              if (!projectId || !threadId) {
                return;
              }
              navigate(
                `/p/${encodeURIComponent(projectId)}/t/${encodeURIComponent(
                  threadId,
                )}/s/${encodeURIComponent(subagentId)}`,
              );
            }}
          />
        )}
      </ScrollArea>

      <Modal
        opened={renameOpen}
        onClose={() => setRenameOpen(false)}
        title="Rename thread"
        size="sm"
      >
        <TextInput
          autoFocus
          aria-label="Thread title"
          value={renameValue}
          onChange={(event) => setRenameValue(event.target.value)}
        />
        <Group justify="flex-end" gap="xs" mt="sm">
          <Button variant="default" size="sm" onClick={() => setRenameOpen(false)}>
            Cancel
          </Button>
          <Button
            variant="filled"
            size="sm"
            loading={rename.isPending}
            disabled={!renameValue.trim()}
            onClick={() => rename.mutate()}
          >
            Save
          </Button>
        </Group>
      </Modal>

      <Modal
        opened={deleteOpen}
        onClose={() => setDeleteOpen(false)}
        title="Delete thread?"
        size="sm"
      >
        <Text size="sm" c="dimmed" mb="sm">
          The worktree and all uncommitted changes will be removed. This cannot
          be undone.
        </Text>
        <VisuallyHidden>
          <Text size="sm">Delete this thread and its worktree.</Text>
        </VisuallyHidden>
        <Group justify="flex-end" gap="xs" mt="sm">
          <Button variant="default" size="sm" onClick={() => setDeleteOpen(false)}>
            Cancel
          </Button>
          <Button
            variant="light"
            color="red"
            size="sm"
            loading={remove.isPending}
            onClick={() => remove.mutate()}
          >
            Delete
          </Button>
        </Group>
      </Modal>

      {threadId && selectionTarget ? (
        <SelectionComment
          threadId={threadId}
          target={selectionTarget}
          onClose={() => setSelectionTarget(null)}
        />
      ) : null}
      {threadId ? (
        <CommentDialog
          open={commentOpen}
          onOpenChange={setCommentOpen}
          threadId={threadId}
          anchor={commentAnchor}
        />
      ) : null}
      {liveDiverged && !divergedDismissed ? (
        <Group
          gap="xs"
          px="sm"
          py="xs"
          wrap="wrap"
          style={{
            borderTop: "1px solid var(--mantine-color-yellow-filled)",
            background: "var(--mantine-color-yellow-light)",
          }}
        >
          <Text size="sm" c="yellow">
            This thread has diverged from the project branch.
          </Text>
          <Button
            variant="subtle"
            color="blue"
            size="xs"
            px="xs"
            onClick={() => fork.mutate()}
          >
            Keep local changes in a new thread
          </Button>
          <Button
            variant="subtle"
            color="red"
            size="xs"
            px="xs"
            disabled={discardDiverged.isPending}
            onClick={() => discardDiverged.mutate()}
          >
            Discard
          </Button>
        </Group>
      ) : null}
      {selectedSubagent ? null : (
        <Composer
          threadId={thread.id}
          canWrite={canWrite}
          onRequestAccess={() => requestAccessMutation.mutate()}
          threadStatus={liveStatus}
          participants={thread.participants}
          model={thread.model}
          effort={thread.effort}
          onApplyConfig={(model, effort) =>
            configure.mutate({ model, effort })
          }
        />
      )}
    </Stack>
  );
}
