import {
  Box,
  Button,
  Group,
  Loader,
  ScrollArea,
  Text,
  UnstyledButton,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  IconAlertTriangle,
  IconClock,
  IconHelp,
  IconInbox,
  IconMail,
  IconMessage,
} from "@tabler/icons-react";
import { useState } from "react";
import { listInbox, markAllInboxRead, type InboxItem } from "../api";
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

function timeOf(ts: number) {
  const date = new Date(ts);
  return `${date.toLocaleDateString([], {
    month: "short",
    day: "numeric",
  })} ${date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
}

export function InboxView({
  onOpenThread,
}: {
  onOpenThread: (item: InboxItem) => void;
}) {
  const [filter, setFilter] = useState<"unread" | "all">("unread");
  const queryClient = useQueryClient();
  const inbox = useQuery({
    queryKey: ["inbox", filter],
    queryFn: () => listInbox(filter),
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
  });
  const items = inbox.data?.items ?? [];
  const unread = inbox.data?.unread ?? 0;

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
            <UnstyledButton
              className={`inbox-filter${filter === "unread" ? " active" : ""}`}
              onClick={() => setFilter("unread")}
            >
              Unread{unread ? ` ${unread}` : ""}
            </UnstyledButton>
            <UnstyledButton
              className={`inbox-filter${filter === "all" ? " active" : ""}`}
              onClick={() => setFilter("all")}
            >
              All
            </UnstyledButton>
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
            <UnstyledButton
              key={item.id}
              className={`inbox-row${item.read ? "" : " unread"}`}
              onClick={() => onOpenThread(item)}
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
          ))
        )}
      </ScrollArea>
    </div>
  );
}
