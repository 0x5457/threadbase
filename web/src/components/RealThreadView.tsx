import {
  Alert,
  Button,
  Group,
  Loader,
  Modal,
  Skeleton,
  Stack,
  Text,
  UnstyledButton,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import {
  IconAlertCircle,
  IconArrowBackUp,
  IconArrowDown,
  IconChevronDown,
  IconCopy,
  IconDots,
  IconLoader2,
  IconMessagePlus,
  IconPencil,
  IconRefresh,
} from "@tabler/icons-react";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { threadCan } from "../lib/machine";
import { readPick, rememberPick } from "../lib/model-prefs";
import {
  Fragment,
  forwardRef,
  lazy,
  memo,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import { Virtuoso, type VirtuosoHandle } from "react-virtuoso";
import {
  cancelThread,
  createComment,
  decideApproval,
  deleteComment,
  dropQueueItem,
  editThreadMessage,
  getMe,
  getState,
  getThreadBundle,
  getThreadGoal,
  reorderQueue,
  respondElicitation,
  rewindThread,
  restoreQueueItem,
  retryThreadSetup,
  restartThread,
  sendComment,
  sendMessage,
  steerQueueItem,
  updateComment,
  updateQueueItem,
  updateThread,
  type Approval,
  type Comment,
  type CommentAnchor,
  type Elicitation,
} from "../api";
import { useLiveThread } from "../lib/live-thread";
import type { LivePanel } from "../lib/live/types";
import { type ViewItem } from "../lib/thread-events";
import { ActionSheet } from "./ActionSheet";
import { AgentLog } from "./AgentLog";
import { ComposerSkeleton } from "./composer/ComposerSkeleton";
import { ApprovalCard } from "./thread/ApprovalCard";
import { CommentCard, CommentDraft } from "./thread/CommentCard";
import { CommentThread } from "./thread/CommentThread";
import { ElicitationCard } from "./thread/ElicitationCard";
import { EmptyThread } from "./thread/EmptyThread";
import { EditMessageDialog } from "./thread/EditMessageDialog";
import { Message, Notice, Thinking } from "./thread/Message";
import { formatDuration, timeOf } from "./thread/format";
import { GoalBar } from "./thread/GoalBar";
import { QueueBar } from "./thread/QueueBar";
import { Tool } from "./thread/Tool";

/// Virtuoso renders the transcript inside this wrapper so the document column,
/// its padding and the text-width setting keep applying.

/// A row is either a single reducer item or a run of consecutive tool calls,
/// which the transcript folds into one quiet tree.
type ToolItem = Extract<ViewItem, { kind: "tool" }>;

/** Tool rows rendered before a long run folds its tail away. Ten commands
 *  say what the agent did without turning a fifty-command turn into a wall. */
const PREVIEW_TOOLS = 10;

/** Stable empty list: a fresh `[]` per row would break memoized rows. */
const NO_COMMENTS: Comment[] = [];
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
function TimelineRowImpl({
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
  onEditMessage,
  onRewindMessage,
  onMessageLongPress,
  continued = false,
}: {
  item: TimelineRowItem;
  active: boolean;
  /** Same author as the row above; the avatar gutter collapses. */
  continued?: boolean;
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
  onEditMessage?: (item: Extract<ViewItem, { kind: "message" }>) => void;
  onRewindMessage?: (item: Extract<ViewItem, { kind: "message" }>) => void;
  onMessageLongPress: (item: Extract<ViewItem, { kind: "message" }>) => void;
}) {
  const toolRow = (tool: ToolItem, nested = false) => (
    <Fragment key={tool.call.id}>
      <Tool
        item={tool}
        nested={nested}
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

  // A run shows every tool it ran under one summary so the tree reads as a
  // sequence of actions. Total tool time says more than the count alone;
  // wall-clock spans would include user idle time between turns.
  const toolDuration = (tools: ToolItem[]) =>
    tools.reduce(
      (total, tool) =>
        total +
        (tool.call.status === "pending"
          ? 0
          : Math.max(0, tool.call.updatedAt - tool.call.createdAt)),
      0,
    );

  /** Unique files a run touched; a summary that says what changed, not only
   *  how many tools ran. */
  const toolFileCount = (tools: ToolItem[]) =>
    new Set(
      tools
        .map((tool) => tool.call.locations[0]?.path)
        .filter((path): path is string => Boolean(path)),
    ).size;

  const groupRows = (id: string, tools: ToolItem[]) => {
    const open = expanded.has(`g:${id}`);
    const shown = open ? tools : tools.slice(0, PREVIEW_TOOLS);
    const rest = tools.length - shown.length;
    return (
      <>
        {shown.map((tool) => toolRow(tool, true))}
        {rest > 0 ? (
          <button
            type="button"
            className="tool-row tool-more-row"
            aria-expanded={false}
            onClick={() => onToggleExpanded(`g:${id}`)}
          >
            <span className="tool-icon" aria-hidden>
              <IconDots size={15} stroke={1.5} />
            </span>
            <span className="tool-text">
              <span className="tool-verb">
                {rest} more {rest === 1 ? "tool" : "tools"}
              </span>
            </span>
            <span className="tool-end">
              <IconChevronDown
                size={12}
                stroke={1.5}
                className="tool-chevron"
                aria-hidden
              />
            </span>
          </button>
        ) : null}
      </>
    );
  };

  /** One quiet summary line; the tool rows below carry the detail. */
  const toolSummary = (tools: ToolItem[]) => {
    const files = toolFileCount(tools);
    const elapsed = toolDuration(tools);
    return [
      `${tools.length} ${tools.length === 1 ? "tool" : "tools"}`,
      elapsed > 0 ? formatDuration(elapsed) : null,
      files > 0 ? `${files} ${files === 1 ? "file" : "files"}` : null,
    ]
      .filter((part): part is string => Boolean(part))
      .join(" · ");
  };

  const groupHead = (
    kind: "run" | "tools",
    id: string,
    label: string,
    tools: ToolItem[],
    title?: string,
  ) => {
    // Collapsed is tracked as presence in the shared set; groups start open.
    const collapsed = expanded.has(`c:${id}`);
    const busy = tools.some(
      (tool) =>
        tool.call.status === "pending" || tool.call.status === "in_progress",
    );
    const failed = tools.some((tool) => tool.call.status === "failed");
    return (
      <div className={kind === "run" ? "run" : "tool-group"}>
        <button
          type="button"
          className="run-head"
          aria-expanded={!collapsed}
          title={title || undefined}
          onClick={() => onToggleExpanded(`c:${id}`)}
        >
          <span>{label}</span>
          {busy ? (
            <IconLoader2 size={12} stroke={1.75} className="tool-spinner" />
          ) : null}
          {failed ? (
            <IconAlertCircle size={12} stroke={1.5} className="run-head-failed" />
          ) : null}
          <IconChevronDown
            size={12}
            stroke={1.5}
            className="chevron"
            aria-hidden
          />
        </button>
        {collapsed ? null : (
          <div className="run-body">{groupRows(id, tools)}</div>
        )}
      </div>
    );
  };

  if (item.kind === "run") {
    const count = item.thinking.count;
    const thought = count > 1 ? `Thought ${count} times` : "Thought once";
    return groupHead(
      "run",
      `run:${item.id}`,
      `${thought} · ${toolSummary(item.tools)}`,
      item.tools,
      item.thinking.text.trim().slice(0, 240),
    );
  }

  if (item.kind === "tools" && item.tools.length > 1) {
    return groupHead(
      "tools",
      `tools:${item.id}`,
      toolSummary(item.tools),
      item.tools,
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
          continued={continued}
          onComment={onComment}
          onEdit={onEditMessage}
          onRewind={onRewindMessage}
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

/**
 * Memoized per row: a token batch replaces only the streaming block's item,
 * so every other mounted row can skip the render. All callbacks passed from
 * the view are stable for the same reason.
 */
const TimelineRow = memo(
  TimelineRowImpl,
  (prev, next) => {
    // Group rows are rebuilt on every batch (their wrapper objects are new),
    // but their tool calls keep identity until one of them actually updates.
    const prevItem = prev.item;
    const nextItem = next.item;
    const sameTools = (a: ToolItem[], b: ToolItem[]) =>
      a.length === b.length &&
      a.every(
        (tool, index) =>
          tool.call === b[index].call && tool.ts === b[index].ts,
      );
    const sameItem =
      prevItem === nextItem ||
      (prevItem.kind === "tools" && nextItem.kind === "tools"
        ? sameTools(prevItem.tools, nextItem.tools)
        : prevItem.kind === "run" && nextItem.kind === "run"
          ? prevItem.thinking === nextItem.thinking &&
            sameTools(prevItem.tools, nextItem.tools)
          : false);
    return (
      sameItem &&
      prev.continued === next.continued &&
      prev.active === next.active &&
      prev.expanded === next.expanded &&
      prev.onToggleExpanded === next.onToggleExpanded &&
      prev.comments === next.comments &&
      prev.commentsForTool === next.commentsForTool &&
      prev.threadKey === next.threadKey &&
      prev.openComments === next.openComments &&
      prev.onToggleComments === next.onToggleComments &&
      prev.renderComment === next.renderComment &&
      prev.editing === next.editing &&
      prev.editor === next.editor &&
      prev.onComment === next.onComment &&
      prev.onEditMessage === next.onEditMessage &&
      prev.onRewindMessage === next.onRewindMessage &&
      prev.onMessageLongPress === next.onMessageLongPress
    );
  },
);

// Tiptap, the CRDT draft provider and the suggestion stack are the composer's
// dependencies. Keeping them in a separate chunk lets the transcript paint
// while the editor downloads behind its reserved space.
const Composer = lazy(() =>
  import("./Composer").then((module) => ({ default: module.Composer })),
);

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
  const [commentBusy, setCommentBusy] = useState<string | null>(null);
  const [commentFor, setCommentFor] = useState<string | null>(null);
  const [messageSheet, setMessageSheet] = useState<Extract<
    ViewItem,
    { kind: "message" }
  > | null>(null);
  const [editingMessage, setEditingMessage] = useState<Extract<
    ViewItem,
    { kind: "message" }
  > | null>(null);
  const [rewinding, setRewinding] = useState<Extract<
    ViewItem,
    { kind: "message" }
  > | null>(null);
  const [rewindBusy, setRewindBusy] = useState(false);
  const [stopping, setStopping] = useState(false);
  // A long session can hold thousands of blocks; Virtuoso mounts only what is
  // visible, so a 10k-event transcript costs the same as a short one.
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const scrollerRef = useRef<HTMLElement | null>(null);
  const [stickToBottom, setStickToBottom] = useState(true);
  const stickRef = useRef(true);
  const removeScrollerListenerRef = useRef<(() => void) | null>(null);
  /** Coalesce Virtuoso height notifications into one follow per frame. */
  const followFrameRef = useRef(0);
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
  const toggleComments = useCallback((key: string) => {
    setOpenComments((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);
  const notifiedApprovals = useRef<Set<string>>(new Set());

  // Metadata-only bundle: this view never renders `changes`, so asking for
  // it would run a git diff on every 4 s poll. The Changes panel keeps the
  // full `["thread", id]` query.
  const bundleQuery = useQuery({
    queryKey: ["thread", threadId, "meta"],
    queryFn: () => getThreadBundle(threadId, { changes: false }),
    staleTime: 2_000,
    // The live socket carries events and thread state; this poll only picks
    // up cross-device approvals/comments/queue changes. Eight seconds keeps
    // those responsive without re-downloading the bundle every four.
    refetchInterval: 8_000,
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
    agentName: bundle?.thread.agent,
    hidden: stateQuery.data?.settings.hiddenPresence ?? false,
    onInboxCount: () => {
      void queryClient.invalidateQueries({ queryKey: ["inbox"] });
      void queryClient.invalidateQueries({ queryKey: ["inbox-unread"] });
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

  const { items, diagnostics, usage } = live.view;
  const terminalUsers = useMemo(
    () =>
      live.presence.focuses
        .filter(
          (focus) =>
            focus.threadId === threadId &&
            !focus.hidden &&
            focus.panel === "terminal",
        )
        .map((focus) => {
          const participant = bundle?.thread.participants.find(
            (entry) => entry.user.id === focus.userId,
          );
          return participant?.user.displayName ?? focus.userId;
        }),
    [bundle?.thread.participants, live.presence.focuses, threadId],
  );
  const pending = (bundle?.approvals ?? []).filter(
    (approval) => approval.status === "pending",
  );
  const elicitations = (bundle?.elicitations ?? []).filter(
    (elicitation) => elicitation.status === "pending",
  );
  const comments = bundle?.comments ?? [];
  // The live fold carries the queue snapshot the server pushes with every
  // queue change; the 8 s bundle poll only fills in before that event lands
  // (or while the socket is down).
  const queue =
    (live.phase === "live" ? live.view.queue : null) ?? bundle?.queue ?? [];
  const canWrite = bundle?.role === "owner" || bundle?.role === "editor";

  // The bundle refetches every 4 s. A fresh JSON object must not hand every
  // visible row a new comment lookup: that invalidates TimelineRow's memo and
  // re-runs markdown for the whole viewport on each poll. Rebuild the index
  // only when a comment actually changed.
  const commentsSignature = useMemo(
    () => comments.map((comment) => JSON.stringify(comment)).join("\n"),
    [comments],
  );

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
    // `commentsSignature` covers every field the map reads; keying the memo on
    // `comments` itself would rebuild it on every bundle poll.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [commentsSignature]);

  const commentsFor = useCallback(
    (kind: "message" | "tool_call", id: string) =>
      commentsByAnchor.get(`${kind === "message" ? "m" : "t"}:${id}`) ??
      NO_COMMENTS,
    [commentsByAnchor],
  );
  const commentsForTool = useCallback(
    (id: string) => commentsFor("tool_call", id),
    [commentsFor],
  );

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

  // History arrives after mount, so `initialTopMostItemIndex` cannot land on
  // the last row; jump there once the first batch renders. A second pass after
  // the tail rows are measured corrects the estimate.
  const rowsLengthRef = useRef(0);
  useEffect(() => {
    rowsLengthRef.current = rows.length;
  }, [rows.length]);

  // `scrollToIndex(LAST, align: "end")` stops at the last row, leaving the
  // footer below the fold. Scrolling the element itself lands on the true end.
  const scrollToLatest = useCallback(
    (behavior: "auto" | "smooth" = "auto", force = false) => {
      // The post-load correction must not yank a reader who already scrolled up.
      if (!force && !stickRef.current) return;
      const scroller = scrollerRef.current;
      if (!scroller) return;
      scroller.scrollTo({ top: scroller.scrollHeight, behavior });
    },
    [],
  );

  // `followOutput` stops following as soon as Virtuoso's at-bottom estimate
  // goes false. A tall tool result or a batch of streamed lines can cross that
  // line before the next frame, which used to leave the reader permanently
  // short of the tail with the "Jump to latest" button showing. Follow our own
  // pin instead: only an upward scroll by the reader releases it.
  const followTail = useCallback(() => {
    if (!stickRef.current || followFrameRef.current !== 0) return;
    followFrameRef.current = window.requestAnimationFrame(() => {
      followFrameRef.current = 0;
      if (stickRef.current) scrollToLatest("auto", true);
    });
  }, [scrollToLatest]);

  const bindScroller = useCallback((element: HTMLElement | Window | null) => {
    removeScrollerListenerRef.current?.();
    removeScrollerListenerRef.current = null;
    if (!(element instanceof HTMLElement)) {
      scrollerRef.current = null;
      return;
    }
    scrollerRef.current = element;
    // A phone keyboard shrinks this box without a window resize (iOS keeps the
    // layout viewport), and the newest rows then sit below the fold. Re-pin the
    // tail whenever the box we follow changes size.
    const resizeObserver = new ResizeObserver(() => {
      if (stickRef.current) scrollToLatest("auto", true);
    });
    resizeObserver.observe(element);
    const unpin = () => {
      if (!stickRef.current || element.scrollTop <= 0) return;
      stickRef.current = false;
      setStickToBottom(false);
    };
    const onScroll = () => {
      const gap = element.scrollHeight - element.scrollTop - element.clientHeight;
      if (gap > 32 || stickRef.current) return;
      stickRef.current = true;
      setStickToBottom(true);
    };
    // Unpin from explicit reader input, never from `scrollTop` moving on its
    // own. Virtuoso re-anchors the viewport while streaming rows grow, and that
    // adjustment used to look exactly like a reader scrolling up.
    const onWheel = (event: WheelEvent) => {
      if (event.deltaY < 0) unpin();
    };
    let touchStartY: number | null = null;
    const onTouchStart = (event: TouchEvent) => {
      touchStartY = event.touches[0]?.clientY ?? null;
    };
    const onTouchMove = (event: TouchEvent) => {
      const y = event.touches[0]?.clientY;
      if (touchStartY !== null && y !== undefined && y > touchStartY + 6) {
        unpin();
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.key !== "PageUp" &&
        event.key !== "Home" &&
        event.key !== "ArrowUp"
      ) {
        return;
      }
      const target = event.target;
      if (
        target instanceof Element &&
        target.closest("input, textarea, [contenteditable='true']")
      ) {
        return;
      }
      unpin();
    };
    const onPointerDown = (event: PointerEvent) => {
      const rect = element.getBoundingClientRect();
      // A press on the scrollbar track/thumb is an explicit scroll gesture;
      // the scroll handler alone cannot tell it apart from re-anchoring.
      if (event.clientX >= rect.left + element.clientWidth - 1) unpin();
    };
    element.addEventListener("scroll", onScroll, { passive: true });
    element.addEventListener("wheel", onWheel, { passive: true });
    element.addEventListener("touchstart", onTouchStart, { passive: true });
    element.addEventListener("touchmove", onTouchMove, { passive: true });
    window.addEventListener("keydown", onKeyDown);
    element.addEventListener("pointerdown", onPointerDown);
    removeScrollerListenerRef.current = () => {
      resizeObserver.disconnect();
      element.removeEventListener("scroll", onScroll);
      element.removeEventListener("wheel", onWheel);
      element.removeEventListener("touchstart", onTouchStart);
      element.removeEventListener("touchmove", onTouchMove);
      window.removeEventListener("keydown", onKeyDown);
      element.removeEventListener("pointerdown", onPointerDown);
    };
  }, [scrollToLatest]);

  useEffect(
    () => () => {
      removeScrollerListenerRef.current?.();
      if (followFrameRef.current !== 0) {
        window.cancelAnimationFrame(followFrameRef.current);
      }
    },
    [],
  );

  useEffect(() => {
    let cancelled = false;
    const timers: number[] = [];
    let tries = 0;
    const scrollLater = (delay: number) => {
      timers.push(
        window.setTimeout(() => {
          if (!cancelled) scrollToLatest();
        }, delay),
      );
    };
    const step = () => {
      if (cancelled) return;
      if (rowsLengthRef.current === 0 && tries < 100) {
        tries += 1;
        timers.push(window.setTimeout(step, 100));
        return;
      }
      // Opening a thread always lands on the latest message. The first jump is
      // forced because Virtuoso may still place the last row at the top; the
      // later corrections respect a reader who has already scrolled away.
      scrollToLatest("auto", true);
      scrollLater(300);
      scrollLater(900);
    };
    timers.push(window.setTimeout(step, 0));
    return () => {
      cancelled = true;
      for (const timer of timers) window.clearTimeout(timer);
    };
  }, [scrollToLatest]);

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

  const refresh = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: ["thread", threadId] });
  }, [queryClient, threadId]);

  // Goal progress lives in the adapter's workspace (`<cwd>/.pi/goals`), not in
  // the transcript. Read it on mount and after each turn, with a slow poll as
  // a floor for goals that change without a turn of their own.
  const goalQuery = useQuery({
    queryKey: ["thread", threadId, "goal"],
    queryFn: () => getThreadGoal(threadId),
    staleTime: 2_000,
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  });
  const goal = goalQuery.data?.goal ?? null;
  const [goalBusy, setGoalBusy] = useState(false);
  // The event stream carries the status change before the 4s bundle poll does;
  // the poll stays as the cold-start fallback.
  const threadStatus = live.threadState?.status ?? bundle?.thread.status;

  useEffect(() => {
    // A turn can create, complete or block a goal; re-read once it settles.
    if (threadStatus === "running") return;
    void queryClient.invalidateQueries({ queryKey: ["thread", threadId, "goal"] });
  }, [threadStatus, queryClient, threadId]);

  const reportFailure = useCallback((cause: unknown, fallback: string) => {
    notifications.show({
      message: cause instanceof Error ? cause.message : fallback,
      color: "red",
    });
  }, []);

  const openMessageEdit = useCallback(
    (item: Extract<ViewItem, { kind: "message" }>) => {
      setMessageSheet(null);
      setEditingMessage(item);
    },
    [],
  );

  const openMessageRewind = useCallback(
    (item: Extract<ViewItem, { kind: "message" }>) => {
      setMessageSheet(null);
      setRewinding(item);
    },
    [],
  );

  const confirmRewind = useCallback(async () => {
    if (!rewinding) return;
    setRewindBusy(true);
    try {
      await rewindThread(threadId, rewinding.id);
      setRewinding(null);
      refresh();
    } catch (cause) {
      reportFailure(cause, "Could not rewind the thread");
    } finally {
      setRewindBusy(false);
    }
  }, [refresh, reportFailure, rewinding, threadId]);

  const saveMessageEdit = useCallback(
    async (text: string) => {
      if (!editingMessage) return;
      try {
        await editThreadMessage(threadId, editingMessage.id, text);
        setEditingMessage(null);
        refresh();
      } catch (cause) {
        reportFailure(cause, "Could not edit the message");
        throw cause;
      }
    },
    [editingMessage, refresh, reportFailure, threadId],
  );

  const stopTurn = useCallback(async () => {
    setStopping(true);
    try {
      await cancelThread(threadId);
      refresh();
    } catch (cause) {
      reportFailure(cause, "Could not stop the agent");
    } finally {
      setStopping(false);
    }
  }, [refresh, reportFailure, threadId]);

  /** Poll the goal endpoint until the adapter records `status`, or give up. */
  const settleGoal = useCallback(
    async (status: string) => {
      for (let attempt = 0; attempt < 15; attempt += 1) {
        const current = await getThreadGoal(threadId);
        if (current.goal?.status === status) return true;
        await new Promise((resolve) => setTimeout(resolve, 400));
      }
      return false;
    },
    [threadId],
  );

  const runGoalCommand = useCallback(
    async (command: string, interrupt = false) => {
      setGoalBusy(true);
      try {
        // A busy adapter would queue the command behind the running turn, so
        // pausing stops the turn first: "pause" has to mean "stop now".
        await sendMessage(
          threadId,
          command,
          interrupt && threadStatus === "running" ? "stop_and_send" : undefined,
        );
        if (command === "/goal-pause") {
          // pi-acp never settles a prompt whose extension command starts no run
          // (pause only writes the goal file), which leaves the adapter's turn
          // stuck. Restarting resumes the same pi session on disk and clears it.
          await settleGoal("paused");
          await restartThread(threadId);
        } else if (command === "/goal-resume") {
          await settleGoal("active");
        }
        refresh();
      } catch (cause) {
        reportFailure(cause, "Could not run the goal command");
      } finally {
        setGoalBusy(false);
      }
    },
    [refresh, reportFailure, settleGoal, threadId, threadStatus],
  );

  const setupStatus = bundle?.thread.setupStatus ?? "none";
  const setupActive = setupStatus === "pending" || setupStatus === "running";
  // `unarchive` is legal only while the thread is archived, so it doubles as
  // the machine's answer to "is input frozen here?". Prompting itself may
  // degrade to queueing (a busy adapter), so it is not the right gate.
  const actions =
    live.threadState?.allowedActions ?? bundle?.thread.allowedActions;
  const archived = threadCan(actions, "unarchive");
  const canStop = threadCan(actions, "cancel");
  const writable = canWrite && !setupActive && !archived;
  const retrySetup = useCallback(() => {
    void retryThreadSetup(threadId)
      .then(() => refresh())
      .catch((cause: unknown) => reportFailure(cause, "Could not start setup"));
  }, [refresh, reportFailure, threadId]);
  const setupBanner = setupActive ? (
    <div className="setup-banner" role="status">
      <Loader size={12} />
      <Text size="xs" c="dimmed">
        Setting up the worktree — the agent starts after it finishes.
      </Text>
    </div>
  ) : setupStatus === "failed" ? (
    <Alert
      className="setup-banner setup-banner-failed"
      icon={<IconAlertCircle size={14} />}
      color="red"
      title="Worktree setup failed"
    >
      <Group justify="space-between" gap="sm" wrap="nowrap">
        <Text size="xs" className="setup-error">
          {bundle?.thread.setupError ||
            "The setup command did not finish successfully."}
        </Text>
        <Button
          size="compact-xs"
          variant="default"
          leftSection={<IconRefresh size={12} />}
          onClick={retrySetup}
        >
          Retry setup
        </Button>
      </Group>
    </Alert>
  ) : null;

  const applyConfig = useCallback(
    (patch: { model?: string; effort?: string }, failure: string) => {
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
    },
    [bundle?.thread.agent, queryClient, refresh, reportFailure, threadId],
  );

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
    option: Approval["options"][number],
  ) => {
    if (readOnlyAccount) return;
    setDeciding(approval.id);
    try {
      await decideApproval(approval.id, option.kind, undefined, option.id);
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

  const restore = useCallback(
    async (itemId: string) => {
      try {
        await restoreQueueItem(threadId, itemId);
        refresh();
      } catch (cause) {
        reportFailure(cause, "Could not restore message");
      }
    },
    [refresh, reportFailure, threadId],
  );

  const steer = useCallback(
    async (itemId: string) => {
      setQueueBusy(`steer:${itemId}`);
      try {
        await steerQueueItem(threadId, itemId);
      } catch (cause) {
        reportFailure(cause, "Could not send the message now");
      } finally {
        setQueueBusy(null);
        refresh();
      }
    },
    [refresh, reportFailure, threadId],
  );

  const edit = useCallback(
    async (itemId: string, text: string) => {
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
    },
    [refresh, reportFailure, threadId],
  );

  const drop = useCallback(
    async (itemId: string) => {
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
  }, [refresh, reportFailure, restore, threadId]);

  const reorder = useCallback(
    async (ids: string[]) => {
      try {
        await reorderQueue(threadId, ids);
        refresh();
      } catch (cause) {
        reportFailure(cause, "Could not reorder queue");
        refresh();
      }
    },
    [refresh, reportFailure, threadId],
  );

  const saveComment = useCallback(
    async (anchor: CommentAnchor, body: string): Promise<boolean> => {
      if (!body.trim()) return false;
      try {
        await createComment(threadId, body.trim(), anchor, false);
        notifications.show({
          message: "Comment saved for the next turn",
          color: "teal",
        });
        setCommentFor(null);
        refresh();
        return true;
      } catch (cause) {
        reportFailure(cause, "Could not save comment");
        return false;
      }
    },
    [refresh, reportFailure, threadId],
  );

  const actOnComment = useCallback(
    async (comment: Comment, action: "resolve" | "send" | "delete") => {
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
    },
    [refresh, reportFailure],
  );

  const commentCard = useCallback(
    (comment: Comment) => (
      <CommentCard
        key={comment.id}
        comment={comment}
        busy={commentBusy === comment.id}
        onResolve={() => void actOnComment(comment, "resolve")}
        onSend={() => void actOnComment(comment, "send")}
        onDelete={() => void actOnComment(comment, "delete")}
      />
    ),
    [actOnComment, commentBusy],
  );

  const openComment = useCallback((key: string) => {
    setCommentFor(key);
  }, []);

  const closeComment = useCallback(() => setCommentFor(null), []);

  const commentOnMessage = useCallback(
    (messageId: string) => openComment(`m:${messageId}`),
    [openComment],
  );

  const openMessageSheet = useCallback(
    (item: Extract<ViewItem, { kind: "message" }>) => setMessageSheet(item),
    [],
  );

  const handleModelChange = useCallback(
    (model: string) => applyConfig({ model }, "Could not update model"),
    [applyConfig],
  );
  const handleEffortChange = useCallback(
    (effort: string) => applyConfig({ effort }, "Could not update effort"),
    [applyConfig],
  );
  const openThreadComment = useCallback(
    () => openComment("thread"),
    [openComment],
  );
  const watching = useMemo(
    () =>
      live.presence.focuses.filter(
        (focus) => focus.threadId === threadId && !focus.hidden,
      ).length,
    [live.presence.focuses, threadId],
  );

  const commentEditor = useCallback(
    (anchor: CommentAnchor) => (
      <CommentDraft
        onSave={(body) => saveComment(anchor, body)}
        onCancel={closeComment}
      />
    ),
    [closeComment, saveComment],
  );

  const footerContent = (
    <>
      {comments
        .filter((comment) => comment.anchor.kind === "thread")
        .map(commentCard)}

      {commentFor === "thread" ? (
        <CommentDraft
          onSave={(body) => saveComment({ kind: "thread" }, body)}
          onCancel={closeComment}
        />
      ) : null}

      {pending.map((approval) => (
        <ApprovalCard
          key={approval.id}
          approval={approval}
          busy={deciding === approval.id}
          readOnly={readOnlyAccount}
          onDecide={(option) => void decide(approval, option)}
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
    </>
  );

  if (
    !live.error &&
    !live.hasEvents &&
    (bundleQuery.isPending || live.phase === "loading")
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
          scrollerRef={bindScroller}
          className="thread-scroll"
          data={rows}
          firstItemIndex={firstItemIndex}
          initialTopMostItemIndex={
            firstItemIndex + Math.max(0, rows.length - 1)
          }
          totalListHeightChanged={followTail}
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
                {setupBanner}
              </div>
            ),
            Footer: () => <div className="timeline-footer">{footerContent}</div>,
            EmptyPlaceholder: () => (
              <>
                {setupBanner}
                {live.error ? (
                  <Alert
                    icon={<IconAlertCircle size={14} />}
                    title="Cannot load thread activity"
                    color="red"
                  >
                    {live.error}
                  </Alert>
                ) : (
                  <EmptyThread agent={bundle?.thread.agent ?? "the agent"} />
                )}
              </>
            ),
          }}
          itemContent={(index, item) => {
            const previous =
              index > firstItemIndex
                ? rows[index - firstItemIndex - 1]
                : undefined;
            const continued =
              item.kind === "message" &&
              previous?.kind === "message" &&
              previous.role === item.role;
            return (
              <TimelineRow
                item={item}
                continued={continued}
                active={threadStatus === "running"}
                commentsForTool={commentsForTool}
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
                onEditMessage={writable ? openMessageEdit : undefined}
                onRewindMessage={writable ? openMessageRewind : undefined}
                onMessageLongPress={openMessageSheet}
            />
            );
          }}
        />
      {!stickToBottom ? (
        <UnstyledButton
          className="jump-latest"
          onClick={() => {
            // The tail may still be unmeasured and rows above may have just
            // changed height; correct the landing once more after the smooth
            // scroll settles.
            stickRef.current = true;
            scrollToLatest("smooth", true);
            window.setTimeout(() => scrollToLatest("auto", true), 400);
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
                ...(messageSheet.role === "user" && writable
                  ? [
                      {
                        label: "Edit message",
                        icon: <IconPencil size={17} stroke={1.5} />,
                        onSelect: () => openMessageEdit(messageSheet),
                      },
                      {
                        label: "Rewind to before this turn",
                        icon: <IconArrowBackUp size={17} stroke={1.5} />,
                        destructive: true,
                        onSelect: () => openMessageRewind(messageSheet),
                      },
                    ]
                  : []),
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
      <EditMessageDialog
        message={editingMessage}
        onClose={() => setEditingMessage(null)}
        onSave={saveMessageEdit}
      />
      <Modal
        opened={rewinding !== null}
        onClose={() => setRewinding(null)}
        title="Rewind to before this turn?"
        size="md"
        centered
      >
        <Stack gap="sm">
          <Text size="sm">
            The worktree goes back to the checkpoint taken before this prompt,
            and every later turn is discarded from the transcript. Untracked
            files created after that point are removed.
          </Text>
          <Text size="sm" c="dimmed">
            The agent&apos;s conversation cannot be rolled back, so its context is
            cleared: the next message starts a fresh session with what remains
            in the transcript.
          </Text>
          <Group justify="flex-end" gap="xs" mt={4}>
            <Button variant="default" onClick={() => setRewinding(null)}>
              Cancel
            </Button>
            <Button
              color="red"
              loading={rewindBusy}
              onClick={() => void confirmRewind()}
            >
              Rewind
            </Button>
          </Group>
        </Stack>
      </Modal>
      <div className="composer-stack">
        <AgentLog lines={diagnostics} />
        {goal ? (
          <GoalBar
            goal={goal}
            canWrite={writable}
            busy={goalBusy}
            onCommand={runGoalCommand}
          />
        ) : null}
        <QueueBar
          items={queue}
          busy={queueBusy}
          canWrite={writable}
          onDrop={drop}
          onSteer={steer}
          onEdit={edit}
          onReorder={reorder}
        />
        <Suspense
          fallback={<ComposerSkeleton agent={bundle?.thread.agent} />}
        >
          <Composer
            threadId={threadId}
            canWrite={writable}
            projectId={bundle?.project.id}
            agentName={bundle?.thread.agent}
            modelName={bundle?.thread.model}
            effort={bundle?.thread.effort}
            sendWithModifier={
              stateQuery.data?.settings.sendWithModifier ?? false
            }
            onModelChange={handleModelChange}
            onEffortChange={handleEffortChange}
            onSent={refresh}
            usage={usage}
            watching={watching}
            terminalUsers={terminalUsers}
            onThreadComment={openThreadComment}
            running={canStop && writable}
            stopping={stopping}
            onStop={writable ? () => void stopTurn() : undefined}
          />
        </Suspense>
      </div>
    </>
  );
});
