import {
  Alert,
  Button,
  Group,
  Loader,
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
  IconRefresh,
} from "@tabler/icons-react";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import { threadCan } from "../lib/machine";
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
  retryThreadSetup,
  sendComment,
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
import { Composer } from "./Composer";
import { ApprovalCard } from "./thread/ApprovalCard";
import { CommentCard, CommentDraft } from "./thread/CommentCard";
import { CommentThread } from "./thread/CommentThread";
import { ElicitationCard } from "./thread/ElicitationCard";
import { EmptyThread } from "./thread/EmptyThread";
import { Message, Notice, Thinking } from "./thread/Message";
import { formatDuration, timeOf } from "./thread/format";
import { QueueBar } from "./thread/QueueBar";
import { Tool } from "./thread/Tool";

/// Virtuoso renders the transcript inside this wrapper so the document column,
/// its padding and the text-width setting keep applying.

/// A row is either a single reducer item or a run of consecutive tool calls,
/// which Delta-style collapses into one "ran N tools" line.
type ToolItem = Extract<ViewItem, { kind: "tool" }>;

/** Tools rendered inline before a run collapses the rest behind a toggle. */
const PREVIEW_TOOLS = 3;
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

  /** Unique files a run touched; a summary that says what changed, not only
   *  how many tools ran. */
  const toolFileCount = (tools: ToolItem[]) =>
    new Set(
      tools
        .map((tool) => tool.call.locations[0]?.path)
        .filter((path): path is string => Boolean(path)),
    ).size;

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
    const files = toolFileCount(item.tools);
    const ran = ` · ran ${item.tools.length} ${
      item.tools.length === 1 ? "tool" : "tools"
    }${elapsed > 0 ? ` · ${formatDuration(elapsed)}` : ""}${
      files > 0 ? ` · ${files} ${files === 1 ? "file" : "files"}` : ""
    }`;
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
    const files = toolFileCount(item.tools);
    return (
      <div className="tool-group">
        <div className="run-head">
          <span>
            ran {item.tools.length} tools
            {toolDuration(item.tools) > 0
              ? ` · ${formatDuration(toolDuration(item.tools))}`
              : ""}
            {files > 0
              ? ` · ${files} ${files === 1 ? "file" : "files"}`
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
      prev.onMessageLongPress === next.onMessageLongPress
    );
  },
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
  // A long session can hold thousands of blocks; Virtuoso mounts only what is
  // visible, so a 10k-event transcript costs the same as a short one.
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const scrollerRef = useRef<HTMLElement | null>(null);
  const [stickToBottom, setStickToBottom] = useState(true);
  const stickRef = useRef(true);
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
  const queue = bundle?.queue ?? [];
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
      scrollToLatest();
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

  const reportFailure = useCallback((cause: unknown, fallback: string) => {
    notifications.show({
      message: cause instanceof Error ? cause.message : fallback,
      color: "red",
    });
  }, []);

  const setupStatus = bundle?.thread.setupStatus ?? "none";
  const setupActive = setupStatus === "pending" || setupStatus === "running";
  // `unarchive` is legal only while the thread is archived, so it doubles as
  // the machine's answer to "is input frozen here?". Prompting itself may
  // degrade to queueing (a busy adapter), so it is not the right gate.
  const actions =
    live.threadState?.allowedActions ?? bundle?.thread.allowedActions;
  const archived = threadCan(actions, "unarchive");
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
    </>
  );

  if (
    !live.error &&
    live.events.length === 0 &&
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
          atBottomStateChange={(atBottom) => {
            stickRef.current = atBottom;
            setStickToBottom(atBottom);
          }}
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
          itemContent={(_index, item) => (
            <TimelineRow
              item={item}
              active={bundle?.thread.status === "running"}
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
          canWrite={writable}
          onDrop={drop}
          onSteer={steer}
          onEdit={edit}
          onReorder={reorder}
        />
        <Composer
          threadId={threadId}
          canWrite={writable}
          projectId={bundle?.project.id}
          agentName={bundle?.thread.agent}
          modelName={bundle?.thread.model}
          effort={bundle?.thread.effort}
          sendWithModifier={stateQuery.data?.settings.sendWithModifier ?? false}
          onModelChange={handleModelChange}
          onEffortChange={handleEffortChange}
          onSent={refresh}
          usage={usage}
          watching={watching}
          terminalUsers={terminalUsers}
          onThreadComment={openThreadComment}
        />
      </div>
    </>
  );
});
