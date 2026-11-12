import {
  Alert,
  Button,
  Group,
  Skeleton,
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
import { readPick, rememberPick } from "../lib/model-prefs";
import {
  Fragment,
  forwardRef,
  memo,
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
  steerQueueItem,
  updateComment,
  updateQueueItem,
  updateThread,
  type Approval,
  type Comment,
  type CommentAnchor,
  type Elicitation,
  type Subagent,
} from "../api";
import { useLiveThread } from "../lib/live-thread";
import type { LivePanel } from "../lib/live/types";
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
import { formatDuration, timeOf } from "./thread/format";
import { QueueBar } from "./thread/QueueBar";
import { SubagentList } from "./thread/SubagentList";
import { Tool } from "./thread/Tool";

/// Virtuoso renders the transcript inside this wrapper so the document column,
/// its padding and the text-width setting keep applying.

/// A row is either a single reducer item or a run of consecutive tool calls,
/// which Delta-style collapses into one "ran N tools" line.
type ToolItem = Extract<ViewItem, { kind: "tool" }>;

/** Tools rendered inline before a run collapses the rest behind a toggle. */
const PREVIEW_TOOLS = 3;
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
  expanded,
  onToggleExpanded,
  onComment,
  onMessageLongPress,
}: {
  item: TimelineRowItem;
  active: boolean;
  /** Group/row keys whose collapsible body is open; survives virtualization. */
  expanded: Set<string>;
  onToggleExpanded: (key: string) => void;
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
      <Tool
        item={tool}
        expanded={expanded.has(`d:${tool.call.id}`)}
        onToggleExpanded={() => onToggleExpanded(`d:${tool.call.id}`)}
      />
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

  // A run shows its opening tools inline: a collapsed summary alone said that
  // the agent ran tools, never what it was doing. The rest stays one click away.
  // Total tool time says more than the count alone; wall-clock spans would
  // include user idle time between turns.
  const toolDuration = (tools: ToolItem[]) =>
    tools.reduce(
      (total, tool) =>
        total +
        (tool.call.status === "pending"
          ? 0
          : Math.max(0, tool.call.updatedAt - tool.call.createdAt)),
      0,
    );

  const groupTools = (tools: ToolItem[], key: string) => {
    const preview = tools.slice(0, PREVIEW_TOOLS);
    const rest = tools.slice(PREVIEW_TOOLS);
    const open = expanded.has(`g:${key}`);
    return (
      <>
        {preview.map(toolRow)}
        {rest.length > 0 ? (
          <details
            className="run-more"
            open={open}
            onToggle={(event) => {
              // Keep the open state in React so a row that Virtuoso unmounts
              // does not silently collapse and shift the scroll height.
              if (event.currentTarget.open !== open) onToggleExpanded(`g:${key}`);
            }}
          >
            <summary>
              <span>
                {rest.length} more {rest.length === 1 ? "tool" : "tools"}
              </span>
              <IconChevronDown size={11} stroke={1.5} className="chevron" />
            </summary>
            {open ? <div className="run-body">{rest.map(toolRow)}</div> : null}
          </details>
        ) : null}
      </>
    );
  };

  if (item.kind === "run") {
    const count = item.thinking.count;
    const thought = count > 1 ? `Thought ${count} times` : "Thought once";
    const elapsed = toolDuration(item.tools);
    const ran = ` · ran ${item.tools.length} ${
      item.tools.length === 1 ? "tool" : "tools"
    }${elapsed > 0 ? ` · ${formatDuration(elapsed)}` : ""}`;
    return (
      <div className="run">
        <div
          className="run-head"
          title={item.thinking.text.trim().slice(0, 240)}
        >
          <span>
            {thought}
            {ran}
          </span>
        </div>
        {groupTools(item.tools, item.id)}
      </div>
    );
  }

  if (item.kind === "tools" && item.tools.length > 1) {
    return (
      <div className="tool-group">
        <div className="run-head">
          <span>
            ran {item.tools.length} tools
            {toolDuration(item.tools) > 0
              ? ` · ${formatDuration(toolDuration(item.tools))}`
              : ""}
          </span>
        </div>
        {groupTools(item.tools, item.id)}
      </div>
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
          <Tool
            item={item.tools[0]}
            expanded={expanded.has(`d:${item.tools[0].call.id}`)}
            onToggleExpanded={() =>
              onToggleExpanded(`d:${item.tools[0].call.id}`)
            }
          />
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

/**
 * Memoized: shell chrome (the nav drawer, theme toggles) re-renders App on every
 * change, and re-rendering the virtualized transcript there is what made the
 * phone sidebar feel stiff. The props here are stable, so it can skip those.
 */
export const RealThreadView = memo(function RealThreadView({
  threadId,
  onPresence,
  presencePanel = "transcript",
}: {
  threadId: string;
  onPresence?: (onlineUserIds: string[]) => void;
  /** What this browser is looking at, reported to everyone else in the thread. */
  presencePanel?: LivePanel;
}) {
  const queryClient = useQueryClient();
  const [deciding, setDeciding] = useState<string | null>(null);
  const [responding, setResponding] = useState<string | null>(null);
  const [queueBusy, setQueueBusy] = useState<string | null>(null);
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
  const scrollerRef = useRef<HTMLElement | null>(null);
  const [stickToBottom, setStickToBottom] = useState(true);
  // Virtuoso's firstItemIndex keeps the viewport anchored when older rows are
  // prepended; the absolute indices only ever grow downwards.
  const [firstItemIndex, setFirstItemIndex] = useState(1_000_000);
  const [loadingEarlier, setLoadingEarlier] = useState(false);
  const pendingEarlier = useRef<{ before: number } | null>(null);
  // Expansion lives in React state: Virtuoso unmounts off-screen rows, and DOM
  // `<details>` state would collapse with them, shifting the scroll height.
  const [expandedRows, setExpandedRows] = useState<Set<string>>(() => new Set());
  const toggleExpandedRow = useCallback((key: string) => {
    setExpandedRows((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);
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
  useEffect(() => {
    live.sendPresence({ panel: presencePanel });
  }, [live.sendPresence, presencePanel]);
  useEffect(
    () => () => {
      onPresence?.([]);
    },
    [onPresence],
  );

  const { items, diagnostics, usage } = useMemo(
    () =>
      reduceEvents(
        live.events.filter((event) => !event.subagentId),
        bundle?.thread.agent ?? "Agent",
      ),
    [live.events, bundle?.thread.agent],
  );
  const terminalUsers = live.presence.focuses
    .filter(
      (focus) =>
        focus.threadId === threadId && !focus.hidden && focus.panel === "terminal",
    )
    .map((focus) => {
      const participant = bundle?.thread.participants.find(
        (entry) => entry.user.id === focus.userId,
      );
      return participant?.user.displayName ?? focus.userId;
    });
  const pending = (bundle?.approvals ?? []).filter(
    (approval) => approval.status === "pending",
  );
  const elicitations = (bundle?.elicitations ?? []).filter(
    (elicitation) => elicitation.status === "pending",
  );
  const comments = bundle?.comments ?? [];
  const queue = bundle?.queue ?? [];
  const subagents = bundle?.subagents ?? [];
  const canWrite = bundle?.role === "owner" || bundle?.role === "editor";
  const running = bundle?.thread.status === "running";

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
    // An assistant message narrates the work around it, but its row is created
    // at the first streamed chunk — often before the tool calls it introduces.
    // Keep the prose under those tools so the reading order is
    // tools → answer, bounded by the next user message / turn marker.
    for (let index = out.length - 2; index >= 0; index -= 1) {
      const row = out[index];
      if (!(row.kind === "message" && row.role === "agent")) continue;
      let end = index + 1;
      while (end < out.length) {
        const next = out[end];
        const movable =
          next.kind === "tool" ||
          next.kind === "tools" ||
          next.kind === "run" ||
          next.kind === "thinking";
        if (!movable) break;
        end += 1;
      }
      if (end > index + 1) {
        out.splice(index, 1);
        out.splice(end - 1, 0, row);
      }
    }
    return out;
  }, [items, pending.length]);

  useEffect(() => {
    setStickToBottom(true);
    setExpandedRows(new Set());
    setFirstItemIndex(1_000_000);
    setLoadingEarlier(false);
    pendingEarlier.current = null;
  }, [threadId]);

  // History arrives after mount, so `initialTopMostItemIndex` cannot land on
  // the last row; jump there once the first batch renders. A second pass after
  // the tail rows are measured corrects the estimate.
  const scrolledFor = useRef<string | null>(null);
  // `scrollToIndex(LAST, align: "end")` stops at the last row, leaving the
  // footer below the fold. Scrolling the element itself lands on the true end.
  const scrollToLatest = useCallback((behavior: "auto" | "smooth" = "auto") => {
    const scroller = scrollerRef.current;
    if (!scroller) return;
    scroller.scrollTo({ top: scroller.scrollHeight, behavior });
  }, []);
  useEffect(() => {
    if (rows.length === 0) return;
    if (scrolledFor.current === threadId) return;
    scrolledFor.current = threadId;
    // No cleanup on row changes: the corrective pass must survive the first
    // measurement flush, otherwise the footer is left below the fold.
    scrollToLatest();
    window.setTimeout(() => scrollToLatest(), 300);
    window.setTimeout(() => scrollToLatest(), 900);
  }, [rows.length, threadId, scrollToLatest]);

  const loadEarlier = async () => {
    if (loadingEarlier || !live.hasEarlier) return;
    const before = rows.length;
    setLoadingEarlier(true);
    const added = await live.loadEarlier();
    setLoadingEarlier(false);
    if (!added) return;
    pendingEarlier.current = { before };
  };

  // The rows are derived from the event store, so the prepend lands one commit
  // after `loadEarlier` resolves; adjust the anchor once that render is in.
  useEffect(() => {
    const pending = pendingEarlier.current;
    if (!pending) return;
    pendingEarlier.current = null;
    const delta = rows.length - pending.before;
    if (delta > 0) setFirstItemIndex((index) => Math.max(0, index - delta));
  }, [rows.length]);


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
    // Remember the pick for new threads and for threads that never chose one.
    const agent = bundle?.thread.agent;
    if (agent) {
      if (patch.model) rememberPick(agent, "model", patch.model);
      if (patch.effort) rememberPick(agent, "effort", patch.effort);
    }
    void updateThread(threadId, patch)
      .then(() => {
        refresh();
        void queryClient.invalidateQueries({
          queryKey: ["agent-config", threadId],
        });
      })
      .catch((cause: unknown) => reportFailure(cause, failure));
  };

  // New threads start with the agent's defaults; apply the last explicit pick
  // once per thread so the picker does not silently reset on every visit.
  const restoredDefaults = useRef<Set<string>>(new Set());
  useEffect(() => {
    const thread = bundle?.thread;
    if (!thread || restoredDefaults.current.has(thread.id)) return;
    if (thread.model !== null && thread.effort !== null) return;
    const patch: { model?: string; effort?: string } = {};
    if (thread.model === null) {
      const model = readPick(thread.agent, "model");
      if (model) patch.model = model;
    }
    if (thread.effort === null) {
      const effort = readPick(thread.agent, "effort");
      if (effort) patch.effort = effort;
    }
    if (Object.keys(patch).length === 0) return;
    restoredDefaults.current.add(thread.id);
    applyConfig(patch, "Could not restore your last model choice");
  }, [bundle?.thread]);

  const decide = async (
    approval: Approval,
    kind: Approval["options"][number]["kind"],
  ) => {
    if (readOnlyAccount) return;
    setDeciding(approval.id);
    try {
      await decideApproval(approval.id, kind);
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
    } catch (cause) {
      reportFailure(cause, "Could not restore message");
    }
  };

  const steer = async (itemId: string) => {
    setQueueBusy(`steer:${itemId}`);
    try {
      await steerQueueItem(threadId, itemId);
    } catch (cause) {
      reportFailure(cause, "Could not send the message now");
    } finally {
      setQueueBusy(null);
      refresh();
    }
  };

  const edit = async (itemId: string, text: string) => {
    setQueueBusy(`edit:${itemId}`);
    try {
      await updateQueueItem(threadId, itemId, text);
      refresh();
      return true;
    } catch (cause) {
      reportFailure(cause, "Could not edit the message");
      return false;
    } finally {
      setQueueBusy(null);
    }
  };

  const drop = async (itemId: string) => {
    setQueueBusy(`drop:${itemId}`);
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
      setQueueBusy(null);
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

  if (
    !live.error &&
    live.events.length === 0 &&
    (bundleQuery.isPending || live.loading)
  ) {
    // Slow links should see the shape of a transcript instead of a bare spinner.
    return (
      <div
        className="doc timeline-doc thread-skeleton"
        aria-busy="true"
        aria-label="Loading thread"
      >
        {[0, 1, 2, 3].map((row) => (
          <div key={row} className="block">
            <div className="block-gutter">
              <Skeleton height={32} width={32} circle />
            </div>
            <div className="block-body">
              <Skeleton height={10} width={`${28 + row * 6}%`} mb={8} />
              <Skeleton height={12} width={`${94 - row * 5}%`} mb={6} />
              <Skeleton
                height={12}
                width={`${72 + (row % 2) * 12}%`}
                mb={row === 3 ? 0 : 14}
              />
            </div>
          </div>
        ))}
      </div>
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
          scrollerRef={(element) => {
            if (element instanceof HTMLElement) scrollerRef.current = element;
          }}
          className="thread-scroll"
          data={rows}
          firstItemIndex={firstItemIndex}
          initialTopMostItemIndex={
            firstItemIndex + Math.max(0, rows.length - 1)
          }
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
            Header: () => (
              <div className="timeline-header">
                <div className="timeline-top-gap" />
                {live.hasEarlier ? (
                  <div className="load-earlier-row">
                    <Button
                      variant="default"
                      size="compact-xs"
                      loading={loadingEarlier}
                      onClick={() => void loadEarlier()}
                    >
                      Load earlier messages
                    </Button>
                  </div>
                ) : null}
              </div>
            ),
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
              expanded={expandedRows}
              onToggleExpanded={toggleExpandedRow}
              onComment={commentOnMessage}
              onMessageLongPress={openMessageSheet}
            />
          )}
        />
      {!stickToBottom ? (
        <UnstyledButton
          className="jump-latest"
          onClick={() => {
            // The tail may still be unmeasured and rows above may have just
            // changed height; correct the landing once more after the smooth
            // scroll settles.
            scrollToLatest("smooth");
            window.setTimeout(() => scrollToLatest(), 400);
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
          busy={queueBusy}
          canWrite={canWrite}
          canSteer={running}
          onDrop={(id) => void drop(id)}
          onSteer={(id) => void steer(id)}
          onEdit={edit}
          onReorder={(ids) => void reorder(ids)}
        />
        <Composer
          threadId={threadId}
          canWrite={canWrite}
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
          usage={usage}
          watching={
            live.presence.focuses.filter(
              (focus) => focus.threadId === threadId && !focus.hidden,
            ).length
          }
          terminalUsers={terminalUsers}
          onThreadComment={() => openComment("thread")}
        />
      </div>
    </>
  );
});
