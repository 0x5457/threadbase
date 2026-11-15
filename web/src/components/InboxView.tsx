import {
  Box,
  Button,
  Group,
  Loader,
  ScrollArea,
  SegmentedControl,
  Text,
  UnstyledButton,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import {
  IconAlertTriangle,
  IconArrowUpRight,
  IconCheck,
  IconCircleCheck,
  IconClock,
  IconCopy,
  IconHelp,
  IconInbox,
  IconMessage,
} from "@tabler/icons-react";
import { Fragment, useRef, useState } from "react";
import {
  listInbox,
  markAllInboxRead,
  markInboxRead,
  type InboxItem,
} from "../api";
import { pathFor } from "../lib/router";
import { useLongPress } from "../lib/use-long-press";
import { ActionSheet, type ActionSheetAction } from "./ActionSheet";
import { EmptyState } from "./EmptyState";

const kindLabel: Record<InboxItem["kind"], string> = {
  approval: "Approval",
  elicitation: "Question",
  mention: "Mention",
  completed: "Finished",
  error: "Error",
};

function kindIcon(kind: InboxItem["kind"]) {
  switch (kind) {
    case "approval":
      return <IconClock size={14} stroke={1.5} />;
    case "elicitation":
      return <IconHelp size={14} stroke={1.5} />;
    case "completed":
      return <IconCircleCheck size={14} stroke={1.5} />;
    case "error":
      return <IconAlertTriangle size={14} stroke={1.5} />;
    default:
      return <IconMessage size={14} stroke={1.5} />;
  }
}

const inboxTime = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
});

function timeOf(ts: number) {
  return inboxTime.format(new Date(ts));
}

const dayFormat = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
});

/** Sticky section label: today and yesterday read better than a date. */
function dayLabel(ts: number) {
  const date = new Date(ts);
  const now = new Date();
  const startOf = (value: Date) =>
    new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime();
  const days = Math.round((startOf(now) - startOf(date)) / 86_400_000);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  return dayFormat.format(date);
}

const SWIPE_ACTION_WIDTH = 84;

function InboxRow({
  item,
  onOpen,
  onLongPress,
  onMarkRead,
  onCopyLink,
}: {
  item: InboxItem;
  onOpen: (item: InboxItem) => void;
  onLongPress: (item: InboxItem) => void;
  onMarkRead: (item: InboxItem) => void;
  onCopyLink: (item: InboxItem) => void;
}) {
  const longPress = useLongPress(() => onLongPress(item));
  const [open, setOpen] = useState(false);
  const [dx, setDx] = useState(0);
  const [swiping, setSwiping] = useState(false);
  const gesture = useRef<{
    pointerId: number;
    x: number;
    y: number;
    base: number;
    active: boolean;
    dx: number;
  } | null>(null);
  const suppressClick = useRef(false);

  const close = () => {
    setOpen(false);
    setDx(0);
  };

  const onPointerDown = (event: React.PointerEvent<HTMLButtonElement>) => {
    longPress.onPointerDown(event);
    if (event.pointerType === "mouse") return;
    const target = event.target;
    if (target instanceof Element && target.closest(".inbox-swipe-action")) {
      return;
    }
    gesture.current = {
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      base: open ? -SWIPE_ACTION_WIDTH : 0,
      active: false,
      dx: open ? -SWIPE_ACTION_WIDTH : 0,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
    setSwiping(true);
  };

  const onPointerMove = (event: React.PointerEvent<HTMLButtonElement>) => {
    longPress.onPointerMove(event);
    const current = gesture.current;
    if (!current || current.pointerId !== event.pointerId) return;
    const raw = event.clientX - current.x;
    const vertical = event.clientY - current.y;
    if (!current.active) {
      if (Math.abs(raw) < 8) return;
      if (Math.abs(vertical) > Math.abs(raw)) {
        gesture.current = null;
        setSwiping(false);
        setDx(current.base);
        return;
      }
      current.active = true;
    }
    current.dx = Math.max(
      -SWIPE_ACTION_WIDTH,
      Math.min(0, current.base + raw),
    );
    setDx(current.dx);
  };

  const endSwipe = () => {
    longPress.onPointerUp();
    const current = gesture.current;
    gesture.current = null;
    setSwiping(false);
    if (!current) return;
    if (current.active) {
      const next =
        current.dx < -SWIPE_ACTION_WIDTH / 2 ? -SWIPE_ACTION_WIDTH : 0;
      setDx(next);
      setOpen(next < 0);
      suppressClick.current = true;
      window.setTimeout(() => {
        suppressClick.current = false;
      }, 0);
    }
  };

  const cancelSwipe = () => {
    longPress.onPointerCancel();
    gesture.current = null;
    setSwiping(false);
    setDx(open ? -SWIPE_ACTION_WIDTH : 0);
  };

  return (
    <div className={`inbox-row-shell${item.read ? "" : " unread"}`}>
      <button
        type="button"
        className="inbox-swipe-action"
        tabIndex={open ? 0 : -1}
        aria-hidden={!open}
        onClick={() => {
          close();
          if (item.read) onCopyLink(item);
          else onMarkRead(item);
        }}
      >
        {item.read ? (
          <IconCopy size={16} stroke={1.5} />
        ) : (
          <IconCheck size={16} stroke={1.5} />
        )}
        <span>{item.read ? "Copy" : "Read"}</span>
      </button>
      <UnstyledButton
        className={`inbox-row${item.read ? "" : " unread"}`}
        style={{
          transform: dx !== 0 ? `translateX(${dx}px)` : undefined,
          transition: swiping ? "none" : undefined,
        }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endSwipe}
        onPointerCancel={cancelSwipe}
        onClickCapture={(event) => {
          longPress.onClickCapture(event);
          if (suppressClick.current) {
            suppressClick.current = false;
            event.preventDefault();
            event.stopPropagation();
            return;
          }
          if (open) {
            event.preventDefault();
            event.stopPropagation();
            close();
          }
        }}
        onClick={() => onOpen(item)}
      >
      <Box className="inbox-icon">{kindIcon(item.kind)}</Box>
      <Box className="inbox-body">
        <Group gap={6} wrap="nowrap">
          <Text
            size="sm"
            fw={item.read ? 400 : 600}
            className="inbox-summary"
          >
            {item.summary}
          </Text>
          {item.read ? null : <span className="dot" />}
        </Group>
        <Group gap={6} wrap="nowrap">
          <Text size="xs" c="dimmed" className="inbox-thread">
            {item.threadTitle}
          </Text>
          <Text size="xs" c="dimmed">
            {kindLabel[item.kind]}
          </Text>
        </Group>
      </Box>
      <Text size="xs" c="dimmed" className="inbox-time">
        {timeOf(item.createdAt)}
      </Text>
      </UnstyledButton>
    </div>
  );
}

export function InboxView({
  onOpenThread,
}: {
  onOpenThread: (item: InboxItem) => void;
}) {
  const [filter, setFilter] = useState<"unread" | "all">("unread");
  const [sheetItem, setSheetItem] = useState<InboxItem | null>(null);
  const queryClient = useQueryClient();
  const inbox = useInfiniteQuery({
    queryKey: ["inbox", filter],
    queryFn: ({ pageParam }) => listInbox(filter, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) =>
      last.hasMore && last.items.length > 0
        ? last.items[last.items.length - 1]?.id
        : undefined,
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
  });
  const pages = inbox.data?.pages ?? [];
  const items = pages.flatMap((page) => page.items);
  const unread = pages[0]?.unread ?? 0;

  const markRead = (item: InboxItem) => {
    void markInboxRead(item.threadId)
      .then(() => {
        void queryClient.invalidateQueries({ queryKey: ["inbox"] });
        void queryClient.invalidateQueries({ queryKey: ["inbox-unread"] });
        notifications.show({ message: "Marked read", color: "teal" });
      })
      .catch((cause: unknown) =>
        notifications.show({
          message:
            cause instanceof Error ? cause.message : "Could not mark read",
          color: "red",
        }),
      );
  };

  const copyLink = (item: InboxItem) => {
    const url = `${window.location.origin}${pathFor({
      kind: "thread",
      projectId: item.projectId,
      threadId: item.threadId,
    })}`;
    void navigator.clipboard
      ?.writeText(url)
      .then(() => notifications.show({ message: "Link copied", color: "teal" }))
      .catch(() =>
        notifications.show({ message: "Could not copy link", color: "red" }),
      );
  };

  const sheetActions: ActionSheetAction[] = sheetItem
    ? [
        {
          label: "Open thread",
          icon: <IconArrowUpRight size={17} stroke={1.5} />,
          onSelect: () => onOpenThread(sheetItem),
        },
        ...(!sheetItem.read
          ? [
              {
                label: "Mark read",
                icon: <IconCheck size={17} stroke={1.5} />,
                onSelect: () => markRead(sheetItem),
              },
            ]
          : []),
        {
          label: "Copy link",
          icon: <IconCopy size={17} stroke={1.5} />,
          onSelect: () => copyLink(sheetItem),
        },
      ]
    : [];

  return (
    <div className="inbox-view">
      <div className="inbox-head">
        <Group justify="space-between" wrap="wrap" align="flex-start" gap="sm">
          <Box>
            <Text size="md" fw={500}>
              Inbox
            </Text>
            <Text size="sm" c="dimmed">
              Approvals, questions, mentions and turn outcomes.
            </Text>
          </Box>
          <Group gap={2} className="inbox-filters" wrap="nowrap">
            <SegmentedControl
              size="xs"
              value={filter}
              onChange={(value) => setFilter(value as "unread" | "all")}
              data={[
                {
                  value: "unread",
                  label: `Unread${unread ? ` ${unread}` : ""}`,
                },
                { value: "all", label: "All" },
              ]}
            />
            {unread ? (
              <Button
                variant="subtle"
                size="compact-xs"
                color="gray"
                onClick={() => {
                  void markAllInboxRead()
                    .then(() => {
                      void queryClient.invalidateQueries({
                        queryKey: ["inbox"],
                      });
                      void queryClient.invalidateQueries({
                        queryKey: ["inbox-unread"],
                      });
                      notifications.show({
                        message: "Inbox marked read",
                        color: "teal",
                      });
                    })
                    .catch((cause: unknown) =>
                      notifications.show({
                        message:
                          cause instanceof Error
                            ? cause.message
                            : "Could not mark inbox read",
                        color: "red",
                      }),
                    );
                }}
              >
                Mark all read
              </Button>
            ) : null}
          </Group>
        </Group>
      </div>
      <ScrollArea className="inbox-scroll" type="hover">
        {inbox.isPending ? (
          <Group justify="center" py="xl">
            <Loader size="sm" />
          </Group>
        ) : items.length === 0 ? (
          <EmptyState
            icon={<IconInbox size={16} stroke={1.5} />}
            title={filter === "unread" ? "All clear" : "Nothing here"}
            hint={
              filter === "unread"
                ? "Switch to All to review earlier notifications."
                : undefined
            }
            action={
              filter === "unread" ? (
                <Button
                  variant="default"
                  size="xs"
                  onClick={() => setFilter("all")}
                >
                  View all notifications
                </Button>
              ) : undefined
            }
          >
            {filter === "unread"
              ? "Approvals, questions and mentions waiting on you show up here."
              : "Notifications from every thread show up here."}
          </EmptyState>
        ) : (
          <>
            {items.map((item, index) => {
              const day = dayLabel(item.createdAt);
              const previous = items[index - 1];
              const newDay =
                !previous || dayLabel(previous.createdAt) !== day;
              return (
                <Fragment key={item.id}>
                  {newDay ? <div className="inbox-day">{day}</div> : null}
                  <InboxRow
                    item={item}
                    onOpen={onOpenThread}
                    onLongPress={setSheetItem}
                    onMarkRead={markRead}
                    onCopyLink={copyLink}
                  />
                </Fragment>
              );
            })}
            {inbox.hasNextPage ? (
              <Group justify="center" py="sm">
                <Button
                  variant="subtle"
                  size="compact-sm"
                  color="gray"
                  loading={inbox.isFetchingNextPage}
                  onClick={() => void inbox.fetchNextPage()}
                >
                  Load earlier notifications
                </Button>
              </Group>
            ) : null}
          </>
        )}
      </ScrollArea>

      <ActionSheet
        opened={sheetItem !== null}
        onClose={() => setSheetItem(null)}
        title={sheetItem?.summary}
        subtitle={
          sheetItem
            ? `${sheetItem.threadTitle} · ${kindLabel[sheetItem.kind]}`
            : undefined
        }
        actions={sheetActions}
      />
    </div>
  );
}
