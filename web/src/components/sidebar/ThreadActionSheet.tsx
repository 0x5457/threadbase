import { notifications } from "@mantine/notifications";
import { useQueryClient } from "@tanstack/react-query";
import {
  IconArchive,
  IconArrowUpRight,
  IconCalendarOff,
  IconCheck,
  IconCopy,
  IconMoon,
  IconPin,
  IconPinFilled,
  IconSun,
} from "@tabler/icons-react";
import { updateThread, type ThreadSummary } from "../../api";
import { can } from "../../lib/machine";
import { useThreadPlacement } from "../../lib/use-placement";
import { optimisticThreadPatch } from "../../lib/thread-cache";
import { pathFor } from "../../lib/router";
import { ActionSheet } from "../ActionSheet";

/**
 * The phone/sidebar menu for one thread.
 *
 * Placement items are rendered from the published `placement` machine, so the
 * sheet cannot offer "Settle" on a thread the server would refuse, and adding a
 * shelf later does not mean editing three menus.
 */
export function ThreadActionSheet({
  thread,
  projectId,
  subtitle,
  opened,
  onClose,
  onOpen,
}: {
  thread: ThreadSummary | null;
  projectId: string;
  subtitle?: string;
  opened: boolean;
  onClose: () => void;
  onOpen: (thread: ThreadSummary) => void;
}) {
  const queryClient = useQueryClient();
  const place = useThreadPlacement();

  const setAutoSettle = async (target: ThreadSummary) => {
    const autoSettle = target.autoSettle === "never" ? "default" : "never";
    const rollback = optimisticThreadPatch(
      queryClient,
      target.id,
      { autoSettle },
      target,
    );
    try {
      await updateThread(target.id, { autoSettle });
      void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
      void queryClient.invalidateQueries({ queryKey: ["thread", target.id] });
    } catch (cause) {
      rollback();
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not change auto-settle",
        color: "red",
      });
    }
  };

  const togglePin = async (target: ThreadSummary) => {
    const pinned = !target.pinned;
    const rollback = optimisticThreadPatch(
      queryClient,
      target.id,
      { pinned },
      target,
    );
    try {
      await updateThread(target.id, { pinned });
      void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
      notifications.show({
        message: pinned ? "Thread pinned" : "Thread unpinned",
        color: "teal",
      });
    } catch (cause) {
      rollback();
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not update thread",
        color: "red",
      });
    }
  };

  const copyLink = (target: ThreadSummary) => {
    const url = `${window.location.origin}${pathFor({
      kind: "thread",
      projectId,
      threadId: target.id,
    })}`;
    void navigator.clipboard
      ?.writeText(url)
      .then(() => notifications.show({ message: "Link copied", color: "teal" }))
      .catch(() =>
        notifications.show({ message: "Could not copy link", color: "red" }),
      );
  };

  const placementActions = thread
    ? [
        {
          id: "settle",
          label: "Settle thread",
          icon: <IconArchive size={17} stroke={1.5} />,
        },
        {
          id: "snooze",
          label: "Snooze for a day",
          icon: <IconMoon size={17} stroke={1.5} />,
        },
        {
          id: "unsettle",
          label: "Un-settle thread",
          icon: <IconSun size={17} stroke={1.5} />,
        },
        {
          id: "wake",
          label: "Wake thread",
          icon: <IconSun size={17} stroke={1.5} />,
        },
      ]
        .filter((action) => can("placement", thread.placement, action.id))
        .map((action) => ({
          label: action.label,
          icon: action.icon,
          onSelect: () => void place(thread, action.id),
        }))
    : [];

  return (
    <ActionSheet
      opened={opened}
      onClose={onClose}
      title={thread?.title}
      subtitle={subtitle}
      actions={
        thread
          ? [
              {
                label: "Open thread",
                icon: <IconArrowUpRight size={17} stroke={1.5} />,
                onSelect: () => onOpen(thread),
              },
              {
                label: thread.pinned ? "Unpin thread" : "Pin thread",
                icon: thread.pinned ? (
                  <IconPinFilled size={17} stroke={1.5} />
                ) : (
                  <IconPin size={17} stroke={1.5} />
                ),
                onSelect: () => void togglePin(thread),
              },
              ...placementActions,
              {
                label:
                  thread.autoSettle === "never"
                    ? "Auto-settle off"
                    : "Auto-settle on",
                icon:
                  thread.autoSettle === "never" ? (
                    <IconCheck size={17} stroke={1.5} />
                  ) : (
                    <IconCalendarOff size={17} stroke={1.5} />
                  ),
                onSelect: () => void setAutoSettle(thread),
              },
              {
                label: "Copy link",
                icon: <IconCopy size={17} stroke={1.5} />,
                onSelect: () => copyLink(thread),
              },
            ]
          : []
      }
    />
  );
}
