import {
  AtSign,
  Bell,
  CheckCheck,
  CircleHelp,
  Inbox,
  KeyRound,
  ShieldAlert,
  UserPlus,
} from "lucide-react";
import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useInbox } from "@/api/queries";
import type { InboxFilter } from "@/api/endpoints";
import type { InboxItemDto } from "@/api/types/InboxItemDto";
import type { InboxKind } from "@/api/types/InboxKind";
import {
  Avatar,
  Badge,
  EmptyState,
  ErrorState,
  Skeleton,
  Tabs,
  TabsList,
  TabsTrigger,
} from "@/components/ui";
import { formatRelativeTime, formatTimestamp } from "@/lib/format";

const KIND_ICONS: Record<InboxKind, typeof Bell> = {
  approval: ShieldAlert,
  elicitation: CircleHelp,
  mention: AtSign,
  invite: UserPlus,
  access: KeyRound,
  error: Bell,
};

function kindVariant(
  kind: InboxKind,
): "warning" | "info" | "accent" | "success" | "danger" {
  switch (kind) {
    case "approval":
      return "warning";
    case "elicitation":
      return "info";
    case "mention":
      return "accent";
    case "invite":
      return "success";
    default:
      return "danger";
  }
}

function InboxRow({ item }: { item: InboxItemDto }) {
  const Icon = KIND_ICONS[item.kind];
  const person = item.actor
    ? {
        id: item.actor.id,
        displayName: item.actor.displayName,
        avatarUrl: item.actor.avatarUrl,
        color: item.actor.color,
      }
    : {
        id: "system",
        displayName: "Threadbase",
      };
  return (
    <Link
      to={`/p/${encodeURIComponent(item.projectId)}/t/${encodeURIComponent(
        item.threadId,
      )}`}
      className="flex items-start gap-3 border-b border-border/40 px-4 py-4 transition-colors last:border-b-0 hover:bg-accent/40"
    >
      <Avatar person={person} size="md" />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-1.5">
          <Icon className="size-3 shrink-0 text-faint" aria-hidden />
          <span className="text-[13px] text-foreground">
            {item.actor ? (
              <span className="font-medium">{item.actor.displayName}</span>
            ) : (
              <span className="font-medium">Threadbase</span>
            )}{" "}
            {item.summary}
          </span>
          <Badge variant={kindVariant(item.kind)} size="sm" className="ml-auto">
            {item.kind}
          </Badge>
        </div>
        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11.5px] text-faint">
          <span
            className="truncate"
            title={formatTimestamp(item.createdAt)}
          >
            {formatRelativeTime(item.createdAt)}
          </span>
          <span aria-hidden>·</span>
          <span className="truncate text-muted-foreground">{item.threadTitle}</span>
        </div>
      </div>
      {!item.read ? (
        <span
          className="mt-1.5 size-1.5 shrink-0 rounded-full bg-info"
          aria-label="Unread"
        />
      ) : null}
    </Link>
  );
}

export function InboxPage() {
  const navigate = useNavigate();
  const [filter, setFilter] = useState<InboxFilter>("unread");
  const inbox = useInbox(filter);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="shrink-0 border-b border-border/40 bg-background px-6 py-6">
        <div className="mx-auto flex w-full max-w-3xl flex-wrap items-end justify-between gap-3">
          <div className="space-y-1">
            <h1 className="flex items-center gap-2 text-[24px] font-semibold tracking-[-0.03em] text-foreground">
              <Inbox className="size-4 text-muted-foreground" aria-hidden />
              Inbox
            </h1>
            <p className="text-[12px] text-muted-foreground">
              Approvals, mentions and invitations.
            </p>
          </div>
          <Tabs
            value={filter}
            onValueChange={(value) => setFilter(value as InboxFilter)}
          >
            <TabsList>
              <TabsTrigger value="unread">
                Unread
                {inbox.data ? (
                  <span className="text-[9.5px] text-faint">
                    ({inbox.data.unread})
                  </span>
                ) : null}
              </TabsTrigger>
              <TabsTrigger value="all">All</TabsTrigger>
            </TabsList>
          </Tabs>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-3xl py-4">
          {inbox.isLoading ? (
            <div className="space-y-2 px-4">
              <Skeleton className="h-14 w-full" />
              <Skeleton className="h-14 w-full" />
              <Skeleton className="h-14 w-3/4" />
            </div>
          ) : inbox.isError ? (
            <div className="mx-4 rounded-xl border border-border bg-panel">
              <ErrorState
                error={inbox.error}
                title="Cannot load inbox"
                onRetry={() => void inbox.refetch()}
              />
            </div>
          ) : (inbox.data?.items.length ?? 0) === 0 ? (
            <div className="mx-4 rounded-xl border border-dashed border-border/60 bg-card/20">
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
                    <button
                      type="button"
                      onClick={() => setFilter("all")}
                      className="text-[13px] text-muted-foreground underline-offset-4 hover:text-fg hover:underline"
                    >
                      View all items
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => navigate("/")}
                      className="text-[13px] text-muted-foreground underline-offset-4 hover:text-fg hover:underline"
                    >
                      Back to projects
                    </button>
                  )
                }
              />
            </div>
          ) : (
            <div className="divide-y divide-border/40">
              {inbox.data?.items.map((item) => (
                <InboxRow key={item.id} item={item} />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
