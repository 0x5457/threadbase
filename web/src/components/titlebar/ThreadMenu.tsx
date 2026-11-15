import { ActionIcon, Menu } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  IconArchive,
  IconArrowBackUp,
  IconBell,
  IconBellOff,
  IconDots,
  IconGitFork,
  IconGitMerge,
  IconListCheck,
  IconPencil,
  IconPin,
  IconPlayerStop,
  IconRefresh,
} from "@tabler/icons-react";
import type { WatchState } from "../../api/types/WatchState";
import {
  cancelThread,
  getThreadBundle,
  getThreadWatch,
  restartThread,
  retryThreadSetup,
  setThreadWatch,
} from "../../api";
import { actionLabel, threadCan } from "../../lib/machine";
import { useRoute } from "../../lib/router";

export function ThreadMenu({
  canManage,
  hasWorktree,
  canRunSetup,
  inlineActions = false,
  isPinned,
  allowedActions = [],
  onMerge,
  onReviews,
  onRename,
  onTogglePin,
  onFork,
  onRevert,
  onArchive,
}: {
  canManage: boolean;
  /** Thread-machine commands legal for the current state. */
  allowedActions?: string[];
  /** Merge / reviews / revert need a thread worktree to diff against. */
  hasWorktree: boolean;
  /** The project has a setup command; otherwise “Run setup” is a dead item. */
  canRunSetup: boolean;
  /** Reviews and Apply changes are already buttons in the title bar. */
  inlineActions?: boolean;
  isPinned: boolean;
  onMerge: () => void;
  onReviews: () => void;
  onRename: () => void;
  onTogglePin: () => void;
  onFork: () => void;
  onRevert: () => void;
  onArchive: (archived: boolean) => void;
}) {
  const route = useRoute();
  const threadId = route.kind === "thread" ? route.threadId : null;
  const queryClient = useQueryClient();
  // Same cache entry as the thread view's metadata query, so the menu sees a
  // turn start or finish without its own polling rule.
  const bundle = useQuery({
    queryKey: ["thread", threadId, "meta"],
    queryFn: () => getThreadBundle(threadId!, { changes: false }),
    enabled: Boolean(threadId),
    staleTime: 2_000,
  });
  const actions = bundle.data?.bundle.thread.allowedActions ?? allowedActions;
  const watch = useQuery({
    queryKey: ["thread-watch", threadId],
    queryFn: () => getThreadWatch(threadId!),
    enabled: Boolean(threadId),
  });
  const updateWatch = useMutation({
    mutationFn: (state: WatchState) => setThreadWatch(threadId!, state),
    onSuccess: (state) => {
      queryClient.setQueryData(["thread-watch", threadId], state);
      void queryClient.invalidateQueries({ queryKey: ["inbox-unread"] });
    },
    onError: (cause) => {
      notifications.show({
        message:
          cause instanceof Error
            ? cause.message
            : "Could not update notifications",
        color: "red",
      });
    },
  });
  const watchState = watch.data?.state ?? "not_watching";
  // The watch machine decides the next command, so the label and the request
  // can never disagree about whether this click mutes or unmutes.
  const nextWatch = watchState === "watching" ? "muted" : "watching";
  const watchLabel =
    watchState === "watching"
      ? actionLabel("watch", "mute")
      : watchState === "muted"
        ? actionLabel("watch", "unmute")
        : actionLabel("watch", "watch");

  const stopTurn = useMutation({
    mutationFn: () => cancelThread(threadId!),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["thread", threadId] });
      void queryClient.invalidateQueries({ queryKey: ["threads"] });
    },
    onError: (cause) => {
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not stop the turn",
        color: "red",
      });
    },
  });
  const restartAgent = useMutation({
    mutationFn: () => restartThread(threadId!),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["thread", threadId] });
      void queryClient.invalidateQueries({ queryKey: ["threads"] });
    },
    onError: (cause) => {
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not restart the agent",
        color: "red",
      });
    },
  });
  const canStop = threadId !== null && threadCan(actions, "cancel");
  const canRestart = threadId !== null && threadCan(actions, "restart");
  const canArchive = threadCan(actions, "archive");
  const canUnarchive = threadCan(actions, "unarchive");

  if (!canManage) return null;
  return (
    <Menu position="bottom-end" width={190} shadow="md">
      <Menu.Target>
        <ActionIcon aria-label="Thread actions">
          <IconDots size={15} stroke={1.5} />
        </ActionIcon>
      </Menu.Target>
      <Menu.Dropdown>
        {hasWorktree ? (
          <>
            {inlineActions ? null : (
              <>
                <Menu.Item
                  leftSection={<IconGitMerge size={13} />}
                  onClick={onMerge}
                >
                  Apply changes
                </Menu.Item>
                <Menu.Item
                  leftSection={<IconListCheck size={13} />}
                  onClick={onReviews}
                >
                  Reviews
                </Menu.Item>
              </>
            )}
            {threadId && canRunSetup ? (
              <Menu.Item
                leftSection={<IconRefresh size={13} />}
                onClick={() => {
                  void retryThreadSetup(threadId)
                    .then(() => {
                      void queryClient.invalidateQueries({
                        queryKey: ["thread", threadId],
                      });
                      notifications.show({
                        message: "Worktree setup started",
                        color: "teal",
                      });
                    })
                    .catch((cause: unknown) =>
                      notifications.show({
                        message:
                          cause instanceof Error
                            ? cause.message
                            : "Could not start setup",
                        color: "red",
                      }),
                    );
                }}
              >
                Run setup
              </Menu.Item>
            ) : null}
            {inlineActions && !canRunSetup ? null : <Menu.Divider />}
          </>
        ) : null}
        <Menu.Item leftSection={<IconPencil size={13} />} onClick={onRename}>
          Rename
        </Menu.Item>
        <Menu.Item leftSection={<IconPin size={13} />} onClick={onTogglePin}>
          {isPinned ? "Unpin" : "Pin"}
        </Menu.Item>
        {threadId ? (
          <Menu.Item
            leftSection={
              watchState === "watching" ? (
                <IconBellOff size={13} />
              ) : (
                <IconBell size={13} />
              )
            }
            disabled={updateWatch.isPending}
            onClick={() => updateWatch.mutate(nextWatch)}
          >
            {watchLabel}
          </Menu.Item>
        ) : null}
        {canStop || canRestart ? <Menu.Divider /> : null}
        {canStop ? (
          <Menu.Item
            leftSection={<IconPlayerStop size={13} />}
            disabled={stopTurn.isPending}
            onClick={() => stopTurn.mutate()}
          >
            {actionLabel("thread", "cancel")}
          </Menu.Item>
        ) : null}
        {canRestart ? (
          <Menu.Item
            leftSection={<IconRefresh size={13} />}
            disabled={restartAgent.isPending}
            onClick={() => restartAgent.mutate()}
          >
            {actionLabel("thread", "restart")}
          </Menu.Item>
        ) : null}
        <Menu.Divider />
        <Menu.Item leftSection={<IconGitFork size={13} />} onClick={onFork}>
          Fork thread
        </Menu.Item>
        {hasWorktree ? (
          <Menu.Item
            leftSection={<IconArrowBackUp size={13} />}
            onClick={onRevert}
          >
            Revert changes
          </Menu.Item>
        ) : null}
        {canArchive || canUnarchive ? <Menu.Divider /> : null}
        {canArchive || canUnarchive ? (
          <Menu.Item
            color={canArchive ? "red" : undefined}
            leftSection={<IconArchive size={13} />}
            onClick={() => onArchive(canArchive)}
          >
            {actionLabel("thread", canArchive ? "archive" : "unarchive")}
          </Menu.Item>
        ) : null}
      </Menu.Dropdown>
    </Menu>
  );
}
