import {
  Alert,
  Button,
  Center,
  Group,
  Loader,
  Text,
  UnstyledButton,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import {
  IconAlertCircle,
  IconArrowDown,
  IconChevronDown,
  IconCopy,
  IconMessagePlus,
} from "@tabler/icons-react";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Fragment,
  forwardRef,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import {
  cancelSubagent,
  createComment,
  decideApproval,
  deleteComment,
  dropQueueItem,
  getMe,
  getState,
  getThreadBundle,
  reorderQueue,
  respondElicitation,
  restoreQueueItem,
  sendComment,
  updateComment,
  updateThread,
  type Approval,
  type Comment,
  type CommentAnchor,
  type Elicitation,
  type Subagent,
} from "../api";
import { useLiveThread } from "../lib/live-thread";
import { reduceEvents, type ViewItem } from "../lib/thread-events";
import { ActionSheet } from "./ActionSheet";
import { AgentLog } from "./AgentLog";
import { Composer } from "./Composer";
import { ApprovalCard } from "./thread/ApprovalCard";
import { CommentCard, CommentEditor } from "./thread/CommentCard";
import { CommentThread } from "./thread/CommentThread";
import { ElicitationCard } from "./thread/ElicitationCard";
import { EmptyThread } from "./thread/EmptyThread";
import { Message, Notice, Thinking } from "./thread/Message";
import { timeOf } from "./thread/format";
import { QueueBar } from "./thread/QueueBar";
import { SubagentList } from "./thread/SubagentList";
import { Tool } from "./thread/Tool";

/// Virtuoso renders the transcript inside this wrapper so the document column,
/// its padding and the text-width setting keep applying.

/// A row is either a single reducer item or a run of consecutive tool calls,
/// which Delta-style collapses into one "ran N tools" line.
type ToolItem = Extract<ViewItem, { kind: "tool" }>;
type TimelineRowItem =
  | ViewItem
  | { kind: "tools"; id: string; tools: ToolItem[] }
  | {
      kind: "run";
      id: string;
      thinking: Extract<ViewItem, { kind: "thinking" }>;
      tools: ToolItem[];
    };

/// One transcript row. Rendering is a pure function of the reducer item so
/// Virtuoso can mount and unmount rows freely while scrolling.
function TimelineRow({
  item,
  active,
  comments,
  commentsForTool,
  threadKey,
  openComments,
  onToggleComments,
  renderComment,
  editing,
  editor,
  onComment,
  onMessageLongPress,
}: {
  item: TimelineRowItem;
  active: boolean;
  comments: Comment[];
  commentsForTool: (toolCallId: string) => Comment[];
  threadKey: string | null;
  openComments: Set<string>;
  onToggleComments: (key: string) => void;
  renderComment: (comment: Comment) => ReactNode;
  editing: string | null;
  editor: (anchor: CommentAnchor) => ReactNode;
  onComment: (messageId: string) => void;
  onMessageLongPress: (item: Extract<ViewItem, { kind: "message" }>) => void;
}) {
  const toolRow = (tool: ToolItem) => (
    <Fragment key={tool.call.id}>
      <Tool item={tool} />
      <CommentThread
        comments={commentsForTool(tool.call.id)}
        threadKey={`t:${tool.call.id}`}
        expanded={
          openComments.has(`t:${tool.call.id}`) ||
          editing === `t:${tool.call.id}`
        }
        onToggle={onToggleComments}
        renderCard={renderComment}
        editing={editing === `t:${tool.call.id}`}
        editor={null}
      />
    </Fragment>
  );

  if (item.kind === "run") {
    const count = item.thinking.count;
    const thought = count > 1 ? `Thought ${count} times` : "Thought once";
    const ran = ` · ran ${item.tools.length} ${
      item.tools.length === 1 ? "tool" : "tools"
    }`;
    return (
      <details className="run">
        <summary title={item.thinking.text.trim().slice(0, 240)}>
          <span>
            {thought}
            {ran}
          </span>
          <IconChevronDown size={11} stroke={1.5} />
        </summary>
        <div className="run-body">{item.tools.map(toolRow)}</div>
      </details>
    );
  }

  if (item.kind === "tools" && item.tools.length > 1) {
    return (
      <details className="tool-group">
        <summary>
          <span>ran {item.tools.length} tools</span>
          <IconChevronDown size={11} stroke={1.5} />
        </summary>
        <div className="tool-group-body">{item.tools.map(toolRow)}</div>
      </details>
    );
  }

  const thread =
    threadKey === null ? null : (
      <CommentThread
        comments={comments}
        threadKey={threadKey}
        expanded={openComments.has(threadKey)}
        onToggle={onToggleComments}
        renderCard={renderComment}
        editing={editing === threadKey}
        editor={
          item.kind === "message"
            ? editor({ kind: "message", messageId: item.id })
            : null
        }
      />
    );

  if (item.kind === "message") {
    return (
      <>
        <Message
          item={item}
          active={active}
          onComment={onComment}
          onLongPress={onMessageLongPress}
        />
        {thread}
      </>
    );
  }
  switch (item.kind) {
    case "thinking":
      return <Thinking count={item.count} tools={item.tools} text={item.text} />;
    case "tools":
      return (
        <>
          <Tool item={item.tools[0]} />
          {thread}
        </>
      );
    case "notice":
      return <Notice item={item} />;
    case "turn":
      return <div className="turn-gap">{item.label}</div>;
    default:
      return null;
  }
}

const TimelineList = forwardRef<HTMLDivElement, { style?: React.CSSProperties; children?: React.ReactNode }>(
  ({ style, children }, ref) => (
    <div ref={ref} style={style} className="doc timeline-doc">
      {children}
    </div>
  ),
);
TimelineList.displayName = "TimelineList";

export function RealThreadView({
  threadId,
  onPresence,
}: {
  threadId: string;
  onPresence?: (onlineUserIds: string[]) => void;
}) {
  const queryClient = useQueryClient();
  const [deciding, setDeciding] = useState<string | null>(null);
  const [responding, setResponding] = useState<string | null>(null);
  const [dropping, setDropping] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState<string | null>(null);
  const [commentBusy, setCommentBusy] = useState<string | null>(null);
  const [commentFor, setCommentFor] = useState<string | null>(null);
  const [commentBody, setCommentBody] = useState("");
  const [messageSheet, setMessageSheet] = useState<Extract<
    ViewItem,
    { kind: "message" }
  > | null>(null);
  const [commentSaving, setCommentSaving] = useState(false);
  // A long session can hold thousands of blocks; Virtuoso mounts only what is
  // visible, so a 10k-event transcript costs the same as a short one.
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const [stickToBottom, setStickToBottom] = useState(true);
  const [openComments, setOpenComments] = useState<Set<string>>(new Set());
  const toggleComments = (key: string) =>
    setOpenComments((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  const notifiedApprovals = useRef<Set<string>>(new Set());

  const bundleQuery = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => getThreadBundle(threadId),
    staleTime: 2_000,
    refetchInterval: 4_000,
    refetchIntervalInBackground: false,
    placeholderData: keepPreviousData,
  });
  const bundle = bundleQuery.data?.bundle;
  const stateQuery = useQuery({
    queryKey: ["state"],
    queryFn: getState,
    staleTime: 30_000,
  });
  const me = useQuery({
    queryKey: ["me"],
    queryFn: getMe,
    staleTime: 60_000,
  });
  const readOnlyAccount = me.data?.user.access === "read_only";
  const live = useLiveThread(threadId, {
    projectId: bundle?.project.id,
    hidden: stateQuery.data?.settings.hiddenPresence ?? false,
    onInboxCount: () => {
      void queryClient.invalidateQueries({ queryKey: ["inbox"] });
    },
  });

  useEffect(() => {
    onPresence?.(live.presence.onlineUserIds);
  }, [live.presence.onlineUserIds, onPresence]);
  useEffect(
    () => () => {
      onPresence?.([]);
    },
    [onPresence],
  );

  const { items, diagnostics } = useMemo(
    () =>
      reduceEvents(
        live.events.filter((event) => !event.subagentId),
        bundle?.thread.agent ?? "Agent",
      ),
    [live.events, bundle?.thread.agent],
  );
  const pending = (bundle?.approvals ?? []).filter(
    (approval) => approval.status === "pending",
  );
  const elicitations = (bundle?.elicitations ?? []).filter(
    (elicitation) => elicitation.status === "pending",
  );
  const comments = bundle?.comments ?? [];
  const queue = bundle?.queue ?? [];
  const subagents = bundle?.subagents ?? [];

  // One pass over comments instead of a filter per timeline row.
  const commentsByAnchor = useMemo(() => {
    const map = new Map<string, Comment[]>();
    for (const comment of comments) {
      const key =
        comment.anchor.kind === "message"
          ? `m:${comment.anchor.messageId}`
          : comment.anchor.kind === "tool_call"
            ? `t:${comment.anchor.toolCallId}`
            : null;
      if (key === null) continue;
      const list = map.get(key);
      if (list) list.push(comment);
      else map.set(key, [comment]);
    }
    return map;
  }, [comments]);

  const commentsFor = (kind: "message" | "tool_call", id: string) =>
    commentsByAnchor.get(`${kind === "message" ? "m" : "t"}:${id}`) ?? [];

  // Rows handed to Virtuoso: every reducer block except a pending tool card,
  // which the approval card rendered in the footer replaces.
  const rows = useMemo<TimelineRowItem[]>(() => {
    const visible = items.filter(
      (item) =>
        !(
          pending.length > 0 &&
          item.kind === "tool" &&
          item.call.status === "pending"
        ),
    );
    const out: TimelineRowItem[] = [];
    for (const item of visible) {
      if (item.kind !== "tool") {
        out.push(item);
        continue;
      }
      const previous = out[out.length - 1];
      if (previous?.kind === "run") {
        previous.tools.push(item);
        continue;
      }
      if (previous?.kind === "tools") {
        previous.tools.push(item);
        continue;
      }
      if (previous?.kind === "thinking" && previous.tools > 0) {
        out[out.length - 1] = {
          kind: "run",
          id: `r:${previous.id}`,
          thinking: previous,
          tools: [item],
        };
        continue;
      }
      out.push({ kind: "tools", id: item.call.id, tools: [item] });
    }
    return out;
  }, [items, pending.length]);

  useEffect(() => {
    setStickToBottom(true);
  }, [threadId]);


  useEffect(() => {
    if (!stateQuery.data?.settings.browserNotifications) return;
    if (typeof Notification === "undefined") return;
    if (Notification.permission !== "granted") return;
    for (const approval of pending) {
      if (notifiedApprovals.current.has(approval.id)) continue;
      notifiedApprovals.current.add(approval.id);
      new Notification("Threadbase approval requested", {
        body: approval.title,
        tag: approval.id,
      });
    }
  }, [pending, stateQuery.data?.settings.browserNotifications]);

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ["thread", threadId] });
  };

  const reportFailure = (cause: unknown, fallback: string) => {
    notifications.show({
      message: cause instanceof Error ? cause.message : fallback,
      color: "red",
    });
  };

  const applyConfig = (
    patch: { model?: string; effort?: string },
    failure: string,
  ) => {
    void updateThread(threadId, patch)
      .then(() => {
        refresh();
        void queryClient.invalidateQueries({
          queryKey: ["agent-config", threadId],
        });
      })
      .catch((cause: unknown) => reportFailure(cause, failure));
  };

  const decide = async (
    approval: Approval,
    kind: Approval["options"][number]["kind"],
  ) => {
    if (readOnlyAccount) return;
    setDeciding(approval.id);
    try {
      await decideApproval(approval.id, kind);
      notifications.show({ message: "Decision recorded", color: "teal" });
      refresh();
    } catch (cause) {
      reportFailure(cause, "Could not record decision");
    } finally {
      setDeciding(null);
    }
  };

  const respond = async (
    elicitation: Elicitation,
    action: "accept" | "decline",
    values: Record<string, string>,
  ) => {
    if (readOnlyAccount) return;
    setResponding(elicitation.id);
    try {
      await respondElicitation(
        elicitation.id,
        action,
        Object.keys(values).length ? values : null,
      );
      notifications.show({ message: "Response recorded", color: "teal" });
      refresh();
    } catch (cause) {
      reportFailure(cause, "Could not respond");
    } finally {
      setResponding(null);
    }
  };

  const cancel = async (subagent: Subagent) => {
    setCancelling(subagent.id);
    try {
      await cancelSubagent(threadId, subagent.id);
      notifications.show({ message: "Subagent cancelled", color: "teal" });
      refresh();
    } catch (cause) {
      reportFailure(cause, "Could not cancel subagent");
    } finally {
      setCancelling(null);
    }
  };

  const restore = async (itemId: string) => {
    try {
      await restoreQueueItem(threadId, itemId);
      refresh();
      notifications.show({ message: "Message restored", color: "teal" });
    } catch (cause) {
      reportFailure(cause, "Could not restore message");
    }
  };

  const drop = async (itemId: string) => {
    setDropping(itemId);
    try {
      await dropQueueItem(threadId, itemId);
      refresh();
      notifications.show({
        id: `queue-drop-${itemId}`,
        autoClose: 6000,
        withCloseButton: true,
        message: (
          <Group gap="xs" wrap="nowrap">
            <Text size="sm">Message dropped</Text>
            <Button
              size="compact-xs"
              variant="subtle"
              color="gray"
              onClick={() => {
                notifications.hide(`queue-drop-${itemId}`);
                void restore(itemId);
              }}
            >
              Undo
            </Button>
          </Group>
        ),
      });
    } catch (cause) {
      reportFailure(cause, "Could not drop message");
    } finally {
      setDropping(null);
    }
  };

  const reorder = async (ids: string[]) => {
    try {
      await reorderQueue(threadId, ids);
      refresh();
    } catch (cause) {
      reportFailure(cause, "Could not reorder queue");
      refresh();
    }
  };

  const saveComment = async (anchor: CommentAnchor) => {
    if (!commentBody.trim()) return;
    setCommentSaving(true);
    try {
      await createComment(threadId, commentBody.trim(), anchor, false);
      notifications.show({
        message: "Comment saved for the next turn",
        color: "teal",
      });
      setCommentFor(null);
      setCommentBody("");
      refresh();
    } catch (cause) {
      reportFailure(cause, "Could not save comment");
    } finally {
      setCommentSaving(false);
    }
  };

  const actOnComment = async (
    comment: Comment,
    action: "resolve" | "send" | "delete",
  ) => {
    setCommentBusy(comment.id);
    try {
      if (action === "resolve") {
        await updateComment(comment.id, { status: "resolved" });
      } else if (action === "send") {
        await sendComment(comment.id);
      } else {
        await deleteComment(comment.id);
      }
      refresh();
    } catch (cause) {
      reportFailure(cause, "Could not update comment");
    } finally {
      setCommentBusy(null);
    }
  };

  const commentCard = (comment: Comment) => (
    <CommentCard
      key={comment.id}
      comment={comment}
      busy={commentBusy === comment.id}
      onResolve={() => void actOnComment(comment, "resolve")}
      onSend={() => void actOnComment(comment, "send")}
      onDelete={() => void actOnComment(comment, "delete")}
    />
  );

  const openComment = useCallback((key: string) => {
    setCommentFor(key);
    setCommentBody("");
  }, []);

  const commentOnMessage = useCallback(
    (messageId: string) => openComment(`m:${messageId}`),
    [openComment],
  );

  const openMessageSheet = useCallback(
    (item: Extract<ViewItem, { kind: "message" }>) => setMessageSheet(item),
    [],
  );

  const commentEditor = (anchor: CommentAnchor) => (
    <CommentEditor
      value={commentBody}
      onChange={setCommentBody}
      saving={commentSaving}
      onCancel={() => {
        setCommentFor(null);
        setCommentBody("");
      }}
      onSubmit={() => void saveComment(anchor)}
    />
  );

  const footerContent = (
    <>
      {comments
        .filter((comment) => comment.anchor.kind === "thread")
        .map(commentCard)}

      {commentFor === "thread"
        ? commentEditor({ kind: "thread" })
        : null}

      {pending.map((approval) => (
        <ApprovalCard
          key={approval.id}
          approval={approval}
          busy={deciding === approval.id}
          readOnly={readOnlyAccount}
          onDecide={(kind) => void decide(approval, kind)}
        />
      ))}

      {elicitations.map((elicitation) => (
        <ElicitationCard
          key={elicitation.id}
          elicitation={elicitation}
          busy={responding === elicitation.id}
          readOnly={readOnlyAccount}
          onRespond={(action, values) =>
            void respond(elicitation, action, values)
          }
        />
      ))}

      <SubagentList
        subagents={subagents}
        limit={stateQuery.data?.settings.subthreadsPerThread ?? 10}
        cancelling={cancelling}
        onCancel={(subagent) => void cancel(subagent)}
      />
    </>
  );

  if (bundleQuery.isPending && live.events.length === 0) {
    return (
      <Center h="100%">
        <Loader size="sm" />
      </Center>
    );
  }

  return (
    <>
      <div
        className={`thread-scroll-wrap${
          stateQuery.data?.settings.textWidth === "full" ? " wide" : ""
        }`}
      >
        <Virtuoso
          ref={virtuosoRef}
          className="thread-scroll"
          data={rows}
          initialTopMostItemIndex={Math.max(0, rows.length - 1)}
          followOutput={(atBottom) => (atBottom ? "smooth" : false)}
          atBottomStateChange={setStickToBottom}
          increaseViewportBy={{ top: 600, bottom: 800 }}
          computeItemKey={(index, item) =>
            item.kind === "message"
              ? `m:${item.id}`
              : item.kind === "tools"
                ? `g:${item.id}`
                : item.kind === "run"
                  ? item.id
                  : `i:${index}`
          }
          components={{
            List: TimelineList,
            Header: () => <div className="timeline-top-gap" />,
            Footer: () => <div className="timeline-footer">{footerContent}</div>,
            EmptyPlaceholder: () =>
              live.error ? (
                <Alert
                  icon={<IconAlertCircle size={14} />}
                  title="Cannot load thread activity"
                  color="red"
                >
                  {live.error}
                </Alert>
              ) : (
                <EmptyThread agent={bundle?.thread.agent ?? "the agent"} />
              ),
          }}
          itemContent={(_index, item) => (
            <TimelineRow
              item={item}
              active={bundle?.thread.status === "running"}
              commentsForTool={(id) => commentsFor("tool_call", id)}
              comments={commentsFor(
                item.kind === "message" ? "message" : "tool_call",
                item.kind === "message"
                  ? item.id
                  : item.kind === "tools"
                    ? item.tools[0].call.id
                    : "",
              )}
              threadKey={
                item.kind === "message"
                  ? `m:${item.id}`
                  : item.kind === "tools" && item.tools.length === 1
                    ? `t:${item.tools[0].call.id}`
                    : null
              }
              openComments={openComments}
              onToggleComments={toggleComments}
              renderComment={commentCard}
              editing={commentFor}
              editor={commentEditor}
              onComment={commentOnMessage}
              onMessageLongPress={openMessageSheet}
            />
          )}
        />
      {!stickToBottom ? (
        <UnstyledButton
          className="jump-latest"
          onClick={() => {
            virtuosoRef.current?.scrollToIndex({
              index: "LAST",
              behavior: "smooth",
            });
            setStickToBottom(true);
          }}
        >
          <IconArrowDown size={12} stroke={1.5} />
          Jump to latest
        </UnstyledButton>
      ) : null}
      </div>
      <ActionSheet
        opened={messageSheet !== null}
        onClose={() => setMessageSheet(null)}
        title={messageSheet?.name}
        subtitle={
          messageSheet
            ? `${messageSheet.role === "agent" ? "Agent" : "Message"} · ${timeOf(messageSheet.ts)}`
            : undefined
        }
        actions={
          messageSheet
            ? [
                {
                  label: "Comment on message",
                  icon: <IconMessagePlus size={17} stroke={1.5} />,
                  onSelect: () => commentOnMessage(messageSheet.id),
                },
                {
                  label: "Copy text",
                  icon: <IconCopy size={17} stroke={1.5} />,
                  onSelect: () => {
                    void navigator.clipboard
                      ?.writeText(messageSheet.text)
                      .then(() =>
                        notifications.show({
                          message: "Message copied",
                          color: "teal",
                        }),
                      )
                      .catch(() =>
                        notifications.show({
                          message: "Could not copy message",
                          color: "red",
                        }),
                      );
                  },
                },
              ]
            : []
        }
      />
      <div className="composer-stack">
        <AgentLog lines={diagnostics} />
        <QueueBar
          items={queue}
          busy={dropping}
          onDrop={(id) => void drop(id)}
          onReorder={(ids) => void reorder(ids)}
        />
        <Composer
          threadId={threadId}
          canWrite={bundle?.role === "owner" || bundle?.role === "editor"}
          projectId={bundle?.project.id}
          agentName={bundle?.thread.agent}
          modelName={bundle?.thread.model}
          effort={bundle?.thread.effort}
          sendWithModifier={stateQuery.data?.settings.sendWithModifier ?? true}
          onModelChange={(model) =>
            applyConfig({ model }, "Could not update model")
          }
          onEffortChange={(effort) =>
            applyConfig({ effort }, "Could not update effort")
          }
          onSent={refresh}
          watching={
            live.presence.focuses.filter(
              (focus) => focus.threadId === threadId && !focus.hidden,
            ).length
          }
          onThreadComment={() => openComment("thread")}
        />
      </div>
    </>
  );
}
