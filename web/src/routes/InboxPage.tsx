import {
  AtSign,
  CircleAlert,
  CircleHelp,
  Inbox,
  KeyRound,
  ShieldCheck,
  UserPlus,
  CheckCheck,
} from "lucide-react";
import {
  Avatar,
  Box,
  Button,
  Group,
  ScrollArea,
  Skeleton,
  Stack,
  Tabs,
  Text,
  Title,
} from "@mantine/core";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useInbox } from "@/api/queries";
import type { InboxFilter } from "@/api/endpoints";
import type { InboxItemDto } from "@/api/types/InboxItemDto";
import type { InboxKind } from "@/api/types/InboxKind";
import { EmptyState, ErrorState } from "@/components/app";
import { DataBadge, ListRow } from "@/components/ui";
import { formatRelativeTime, formatTimestamp } from "@/lib/format";
import { layout } from "@/lib/design";

const KIND_ICONS: Record<InboxKind, typeof Inbox> = {
  approval: ShieldCheck,
  elicitation: CircleHelp,
  mention: AtSign,
  invite: UserPlus,
  access: KeyRound,
  error: CircleAlert,
};

function InboxRow({ item }: { item: InboxItemDto }) {
  const Icon = KIND_ICONS[item.kind];
  const actorName = item.actor?.displayName ?? "Threadbase";
  const summary = item.summary.startsWith(actorName)
    ? item.summary
    : `${actorName} ${item.summary}`;

  return (
    <ListRow
      href={`/p/${encodeURIComponent(item.projectId)}/t/${encodeURIComponent(
        item.threadId,
      )}`}
      icon={
        <Avatar
          src={item.actor?.avatarUrl ?? null}
          name={item.actor?.displayName ?? "Threadbase"}
          color={item.actor?.color ?? "initials"}
          size="sm"
          radius="xl"
        />
      }
      meta={
        <Group gap="xs" wrap="nowrap">
          <DataBadge tone="neutral">{item.kind}</DataBadge>
          {!item.read ? (
            <Box
              aria-label="Unread"
              style={{
                width: 6,
                height: 6,
                borderRadius: "50%",
                background: "var(--mantine-color-primary-filled)",
                flexShrink: 0,
              }}
            />
          ) : null}
        </Group>
      }
    >
      <Box
        style={{ display: "flex", flexDirection: "column", gap: 1, minWidth: 0 }}
      >
        <Group gap={6} wrap="nowrap">
          <Icon size={layout.icon.xs} aria-hidden style={{ flexShrink: 0 }} />
          <Text size="sm" truncate="end" style={{ flex: 1 }}>
            {summary}
          </Text>
        </Group>
        <Group gap={6} wrap="nowrap">
          <Text
            size="xs"
            c="dimmed"
            truncate="end"
            title={formatTimestamp(item.createdAt)}
          >
            {formatRelativeTime(item.createdAt)}
          </Text>
          <Text size="xs" c="dimmed" aria-hidden>
            ·
          </Text>
          <Text size="xs" c="dimmed" truncate="end">
            {item.threadTitle}
          </Text>
        </Group>
      </Box>
    </ListRow>
  );
}

export function InboxPage() {
  const navigate = useNavigate();
  const [filter, setFilter] = useState<InboxFilter>("unread");
  const inbox = useInbox(filter);

  return (
    <Stack h="100%" gap={0}>
      <Box style={{ borderBottom: "1px solid var(--mantine-color-default-border)" }}>
        <Box
          maw={layout.pageMaxWidth}
          mx="auto"
          px={layout.gutter}
          py="lg"
          w="100%"
        >
          <Group justify="space-between" align="flex-end" wrap="wrap" gap="sm">
            <Stack gap={2}>
              <Group gap="xs">
                <Inbox size={layout.icon.md} aria-hidden />
                <Title order={1}>Inbox</Title>
              </Group>
              <Text size="xs" c="dimmed">
                Approvals, mentions and invitations.
              </Text>
            </Stack>
            <Tabs
              value={filter}
              onChange={(value) => setFilter(value as InboxFilter)}
            >
              <Tabs.List>
                <Tabs.Tab value="unread">
                  <Group gap={4}>
                    Unread
                    {inbox.data ? (
                      <Text
                        size="xs"
                        c="dimmed"
                        style={{ fontVariantNumeric: "tabular-nums" }}
                      >
                        ({inbox.data.unread})
                      </Text>
                    ) : null}
                  </Group>
                </Tabs.Tab>
                <Tabs.Tab value="all">All</Tabs.Tab>
              </Tabs.List>
            </Tabs>
          </Group>
        </Box>
      </Box>

      <ScrollArea style={{ flex: 1, minHeight: 0 }}>
        <Box
          maw={layout.pageMaxWidth}
          mx="auto"
          w="100%"
          px={layout.gutter}
          py="sm"
        >
          {inbox.isLoading ? (
            <Stack gap="xs">
              {Array.from({ length: 3 }).map((_, index) => (
                <Skeleton key={index} h={layout.rowHeight} />
              ))}
            </Stack>
          ) : inbox.isError ? (
            <ErrorState
              error={inbox.error}
              title="Cannot load inbox"
              onRetry={() => void inbox.refetch()}
            />
          ) : (inbox.data?.items.length ?? 0) === 0 ? (
            <EmptyState
              icon={filter === "unread" ? CheckCheck : Inbox}
              title={
                filter === "unread" ? "You are all caught up" : "Inbox is empty"
              }
              description={
                filter === "unread"
                  ? "New approvals, mentions and invitations will appear here."
                  : "There are no inbox items in this workspace yet."
              }
              action={
                filter === "unread" ? (
                  <Button
                    variant="subtle"
                    color="gray"
                    size="xs"
                    className="tb-action-40"
                    onClick={() => setFilter("all")}
                  >
                    View all items
                  </Button>
                ) : (
                  <Button
                    variant="subtle"
                    color="gray"
                    size="xs"
                    className="tb-action-40"
                    onClick={() => navigate("/")}
                  >
                    Back to projects
                  </Button>
                )
              }
            />
          ) : (
            <Stack gap={0}>
              {inbox.data?.items.map((item) => (
                <InboxRow key={item.id} item={item} />
              ))}
            </Stack>
          )}
        </Box>
      </ScrollArea>
    </Stack>
  );
}
