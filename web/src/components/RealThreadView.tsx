import { Alert, Center, Loader, ScrollArea } from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { IconAlertCircle } from "@tabler/icons-react";
import { keepPreviousData, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  cancelSubagent,
  createComment,
  decideApproval,
  deleteComment,
  dropQueueItem,
  getState,
  getThreadBundle,
  respondElicitation,
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
import { reduceEvents } from "../lib/thread-events";
import { Composer } from "./Composer";
import { ApprovalCard } from "./thread/ApprovalCard";
import { CommentCard, CommentEditor } from "./thread/CommentCard";
import { ElicitationCard } from "./thread/ElicitationCard";
import { EmptyThread } from "./thread/EmptyThread";
import { Message, Notice, Thinking } from "./thread/Message";
import { QueueBar } from "./thread/QueueBar";
import { SubagentList } from "./thread/SubagentList";
import { Tool } from "./thread/Tool";

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
  const [commentSaving, setCommentSaving] = useState(false);
  const viewportRef = useRef<HTMLDivElement>(null);
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

  const items = useMemo(
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

  useEffect(() => {
    const viewport = viewportRef.current;
    if (viewport) viewport.scrollTop = viewport.scrollHeight;
  }, [threadId, items.length, subagents.length, pending.length]);

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

  const drop = async (itemId: string) => {
    setDropping(itemId);
    try {
      await dropQueueItem(threadId, itemId);
      refresh();
    } catch (cause) {
      reportFailure(cause, "Could not drop message");
    } finally {
      setDropping(null);
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

  if (bundleQuery.isPending && live.events.length === 0) {
    return (
      <Center h="100%">
        <Loader size="sm" />
      </Center>
    );
  }

  return (
    <>
      <ScrollArea
        className="thread-scroll"
        type="hover"
        viewportRef={viewportRef}
      >
        <div
          className="doc"
          style={
            stateQuery.data?.settings.textWidth === "full"
              ? { width: "min(900px, 100%)" }
              : undefined
          }
        >
          {live.error && items.length === 0 ? (
            <Alert
              icon={<IconAlertCircle size={14} />}
              title="Cannot load thread activity"
              color="red"
              mb="md"
            >
              {live.error}
            </Alert>
          ) : null}

          {items.length === 0 && pending.length === 0 ? (
            <EmptyThread agent={bundle?.thread.agent ?? "the agent"} />
          ) : null}

          {items
            .filter(
              (item) =>
                !(
                  pending.length > 0 &&
                  item.kind === "tool" &&
                  item.call.status === "pending"
                ),
            )
            .map((item, index) => {
              if (item.kind === "message") {
                const key = `m:${item.id}`;
                return (
                  <Fragment key={key}>
                    <Message
                      item={item}
                      active={bundle?.thread.status === "running"}
                      onComment={commentOnMessage}
                    />
                    {commentsFor("message", item.id).map(commentCard)}
                    {commentFor === key
                      ? commentEditor({
                          kind: "message",
                          messageId: item.id,
                        })
                      : null}
                  </Fragment>
                );
              }
              switch (item.kind) {
                case "thinking":
                  return <Thinking key={index} />;
                case "tool":
                  return (
                    <Fragment key={index}>
                      <Tool item={item} />
                      {commentsFor("tool_call", item.call.id).map(commentCard)}
                    </Fragment>
                  );
                case "notice":
                  return <Notice key={index} item={item} />;
                case "turn":
                  return (
                    <div key={index} className="turn-gap">
                      {item.label}
                    </div>
                  );
                default:
                  return null;
              }
            })}

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
              onDecide={(kind) => void decide(approval, kind)}
            />
          ))}

          {elicitations.map((elicitation) => (
            <ElicitationCard
              key={elicitation.id}
              elicitation={elicitation}
              busy={responding === elicitation.id}
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
        </div>
      </ScrollArea>
      <div className="composer-stack">
        <QueueBar items={queue} busy={dropping} onDrop={(id) => void drop(id)} />
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
