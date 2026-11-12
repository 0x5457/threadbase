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
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  IconAlertTriangle,
  IconArrowUpRight,
  IconCheck,
  IconClock,
  IconCopy,
  IconHelp,
  IconInbox,
  IconMail,
  IconMessage,
} from "@tabler/icons-react";
import { useState } from "react";
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
  invite: "Invite",
  access: "Access",
  error: "Error",
};

function kindIcon(kind: InboxItem["kind"]) {
  switch (kind) {
    case "approval":
      return <IconClock size={14} stroke={1.5} />;
    case "elicitation":
      return <IconHelp size={14} stroke={1.5} />;
    case "invite":
      return <IconMail size={14} stroke={1.5} />;
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

function InboxRow({
  item,
  onOpen,
  onLongPress,
}: {
  item: InboxItem;
  onOpen: (item: InboxItem) => void;
  onLongPress: (item: InboxItem) => void;
}) {
  const longPress = useLongPress(() => onLongPress(item));

  return (
    <UnstyledButton
      className={`inbox-row${item.read ? "" : " unread"}`}
      onClick={() => onOpen(item)}
      {...longPress}
    >
      <Box className="inbox-icon">{kindIcon(item.kind)}</Box>
      <Box className="inbox-body">
        <Group gap={6} wrap="nowrap">
          <Text
            size="sm"
            fw={item.read ? 400 : 500}
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
  const inbox = useQuery({
    queryKey: ["inbox", filter],
    queryFn: () => listInbox(filter),
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
  });
  const items = inbox.data?.items ?? [];
  const unread = inbox.data?.unread ?? 0;

  const markRead = (item: InboxItem) => {
    void markInboxRead(item.threadId)
      .then(() => {
        void queryClient.invalidateQueries({ queryKey: ["inbox"] });
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
              Approvals, questions and mentions from every thread.
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
          >
            Approvals and questions waiting on you show up in this list.
          </EmptyState>
        ) : (
          items.map((item) => (
            <InboxRow
              key={item.id}
              item={item}
              onOpen={onOpenThread}
              onLongPress={setSheetItem}
            />
          ))
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
