import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, Send, Square, Users } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { notifications } from "@mantine/notifications";
import {
  ActionIcon,
  Box,
  Button,
  Flex,
  Menu,
  Paper,
  Popover,
  Select,
  Stack,
  Text,
  Textarea,
} from "@mantine/core";
import { WebsocketProvider } from "y-websocket";
import * as Y from "yjs";
import {
  cancelTurn,
  dropQueueItem,
  retryQueueItem,
  sendMessage,
} from "@/api/endpoints";
import {
  queryKeys,
  useAgentConfig,
  useMe,
  useQueue,
  useWorkspaceState,
} from "@/api/queries";
import type { DeliveryMode } from "@/api/types/DeliveryMode";
import type { QueueItemDto } from "@/api/types/QueueItemDto";
import type { Participant } from "@/api/types/Participant";
import { usePresenceStore } from "@/lib/presence";

interface ComposerProps {
  threadId: string;
  canWrite: boolean;
  onRequestAccess?: () => void;
  threadStatus: string;
  participants?: Participant[];
  model?: string | null;
  effort?: string | null;
  onApplyConfig?: (model: string, effort: string) => void;
}

function websocketUrl(): string {
  const protocol = window.location.protocol === "https:" ? "wss" : "ws";
  return `${protocol}://${window.location.host}/ws/drafts`;
}

function randomId(): string {
  return (
    globalThis.crypto?.randomUUID?.() ??
    `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`
  );
}

export function Composer({
  threadId,
  canWrite,
  onRequestAccess,
  threadStatus,
  participants = [],
  model,
  effort,
  onApplyConfig,
}: ComposerProps) {
  const me = useMe();
  const queue = useQueue(threadId);
  const workspaceState = useWorkspaceState();
  const sendWithModifier = workspaceState.data?.settings.sendWithModifier ?? true;
  const config = useAgentConfig(threadId);
  const modelOption = config.data?.options.find(
    (option) => option.category === "model",
  );
  const effortOption = config.data?.options.find(
    (option) => option.category === "thought_level",
  );
  const [modelOverride, setModelOverride] = useState<string | null>(null);
  const [effortOverride, setEffortOverride] = useState<string | null>(null);
  const modelValue =
    modelOverride ?? modelOption?.currentValue ?? model ?? "";
  const effortValue =
    effortOverride ?? effortOption?.currentValue ?? effort ?? "";
  const queryClient = useQueryClient();
  const { doc, provider } = useMemo(() => {
    const document = new Y.Doc();
    const connection = new WebsocketProvider(websocketUrl(), threadId, document, {
      connect: true,
    });
    return { doc: document, provider: connection };
  }, [threadId]);
  const [synced, setSynced] = useState(false);
  const [status, setStatus] = useState<string>("connecting");
  const [version, setVersion] = useState(0);
  const [mentionOpen, setMentionOpen] = useState(false);
  const [confirmStop, setConfirmStop] = useState<{ status: string } | null>(null);
  const retry = useMutation({
    mutationFn: (item: QueueItemDto) => retryQueueItem(threadId, item.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.queue(threadId),
      });
    },
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
  });
  const stopTurn = useMutation({
    mutationFn: () => cancelTurn(threadId),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.queue(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadBundle(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadEvents(threadId),
      });
    },
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
  });
  const blocks = doc.getArray<Y.Map<unknown>>("blocks");

  useEffect(() => {
    const onSync = (isSynced: boolean) => setSynced(isSynced);
    const onStatus = ({ status: next }: { status: string }) => setStatus(next);
    provider.on("sync", onSync);
    provider.on("status", onStatus);
    return () => {
      provider.off("sync", onSync);
      provider.off("status", onStatus);
      provider.destroy();
      doc.destroy();
    };
  }, [doc, provider]);

  useEffect(() => {
    const refresh = () => setVersion((current) => current + 1);
    blocks.observeDeep(refresh);
    return () => blocks.unobserveDeep(refresh);
  }, [blocks]);

  useEffect(() => {
    if (me.data) {
      provider.awareness.setLocalStateField("user", {
        id: me.data.user.id,
        displayName: me.data.user.displayName,
        color: "#8aa4ff",
      });
    }
  }, [me.data, provider]);

  const userId = me.data?.user.id ?? "";
  useEffect(() => {
    if (!userId || !synced) {
      return;
    }
    const existing = blocks
      .toArray()
      .find(
        (block) => block.get("author") === userId && !block.get("sentAt"),
      );
    if (existing) {
      return;
    }
    doc.transact(() => {
      const block = new Y.Map<unknown>();
      block.set("id", randomId());
      block.set("author", userId);
      block.set("createdAt", Date.now());
      block.set("text", new Y.Text(""));
      blocks.push([block]);
    });
  }, [blocks, doc, synced, userId, version]);

  const list = blocks
    .toArray()
    .filter((block) => !block.get("sentAt"));
  const hasText = list.some((block) => {
    const text = block.get("text");
    return text instanceof Y.Text && text.toString().trim().length > 0;
  });
  const showConfirmStop = confirmStop?.status === threadStatus;
  const pendingDraftText = useRef("");

  // If the server consumed the draft before this client's send arrived,
  // restore the text locally so the author can retry or edit it.
  const restoreDraft = (text: string) => {
    if (!synced || !userId || !text.trim()) {
      return;
    }
    doc.transact(() => {
      const block = new Y.Map<unknown>();
      block.set("id", randomId());
      block.set("author", userId);
      block.set("createdAt", Date.now());
      block.set("text", new Y.Text(text));
      blocks.push([block]);
    });
  };

  const sendWithRetention = (delivery: DeliveryMode | null) => {
    const text = list
      .map((block) => block.get("text"))
      .filter((text): text is Y.Text => text instanceof Y.Text)
      .map((text) => text.toString())
      .join("\n")
      .trim();
    pendingDraftText.current = text;
    send.mutate(delivery);
  };

  const send = useMutation({
    mutationFn: (delivery: DeliveryMode | null) =>
      sendMessage(threadId, {
        text: "",
        delivery,
        fromDraft: true,
      }),
    onSuccess: (response) => {
      if (response.delivery === "queued") {
        notifications.show({
          message: response.queuePosition
            ? `Queued at position ${response.queuePosition}`
            : "Queued",
          color: "green",
        });
      } else {
        notifications.show({
          message:
            response.delivery === "stopped_and_sent"
              ? "Stopped the previous turn and sent"
              : "Sent",
          color: "green",
        });
      }
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadEvents(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadBundle(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.comments(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.queue(threadId),
      });
    },
    onError: (error) => {
      const message = error instanceof Error ? error.message : String(error);
      notifications.show({ message, color: "red" });
      if (message.includes("draft already sent") && pendingDraftText.current.trim()) {
        restoreDraft(pendingDraftText.current);
        pendingDraftText.current = "";
      }
    },
  });

  const withdraw = useMutation({
    mutationFn: (item: QueueItemDto) => dropQueueItem(threadId, item.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: queryKeys.queue(threadId),
      });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.threadBundle(threadId),
      });
    },
    onError: (error) =>
      notifications.show({ message: String(error), color: "red" }),
  });

  if (!canWrite) {
    return (
      <Flex
        align="center"
        justify="center"
        gap="xs"
        px="md"
        py="sm"
        style={{
          borderTop: "1px solid var(--mantine-color-default-border)",
          background: "var(--mantine-color-default)",
          fontSize: "var(--mantine-font-size-sm)",
          color: "var(--mantine-color-dimmed)",
        }}
      >
        <Users size={14} aria-hidden />
        <Text component="span" fz={12} c="dimmed">
          Read-only.
        </Text>
        {onRequestAccess ? (
          <Button
            type="button"
            variant="subtle"
            color="blue"
            size="compact-xs"
            px={0}
            onClick={onRequestAccess}
          >
            Request access
          </Button>
        ) : null}
      </Flex>
    );
  }

  return (
    <Box
      style={{
        background: "var(--mantine-color-default)",
        borderTop: "1px solid var(--mantine-color-default-border)",
      }}
      onKeyDown={(event) => {
        const modifier = event.metaKey || event.ctrlKey;
        if (modifier && event.key === "Enter") {
          event.preventDefault();
          if (synced && hasText && !send.isPending) {
            sendWithRetention("auto");
          }
          return;
        }
        if (
          !modifier &&
          !event.shiftKey &&
          !event.altKey &&
          event.key === "Enter" &&
          !sendWithModifier
        ) {
          event.preventDefault();
          if (synced && hasText && !send.isPending) {
            sendWithRetention("auto");
          }
        }
      }}
    >
      {/* Same 768/16 column as the timeline, so the composer card and the
          message column share one left edge at every width. */}
      <Box
        px="md"
        py="sm"
        style={{ maxWidth: 768, margin: "0 auto", width: "100%" }}
      >
        {(queue.data?.items.length ?? 0) > 0 ? (
          <Paper withBorder p="sm" radius="md" mb="xs" data-testid="queue-panel">
            <Flex
              align="center"
              gap="xs"
              mb="xs"
              style={{
                fontSize: "var(--mantine-font-size-md)",
                textTransform: "uppercase",
                letterSpacing: "0.05em",
                color: "var(--mantine-color-dimmed)",
              }}
            >
              Queue
              <Text size="xs" ff="monospace" c="dimmed">
                {queue.data?.items.length}
              </Text>
            </Flex>
            <Stack gap="xs">
              {queue.data?.items.map((item) => (
                <Flex
                  key={item.id}
                  align="flex-start"
                  gap="xs"
                  p="xs"
                  style={{
                    background: "var(--mantine-color-default-hover)",
                    borderRadius: "var(--mantine-radius-md)",
                  }}
                >
                  <Text size="sm" ff="monospace" c="dimmed">
                    {item.position}
                  </Text>
                  <Box flex={1} miw={0}>
                    <Text size="sm" truncate="end">
                      {item.text}
                    </Text>
                    <Text fz={11} c="dimmed">
                      {item.author.displayName}
                      {item.superseded ? " · previous turn cancelled" : ""}
                    </Text>
                  </Box>
                  {item.superseded ? (
                    <Button
                      type="button"
                      variant="subtle"
                      color="blue"
                      size="compact-xs"
                      onClick={() => retry.mutate(item)}
                    >
                      continue
                    </Button>
                  ) : null}
                  <Button
                    type="button"
                    variant="subtle"
                    color="dark"
                    size="compact-xs"
                    onClick={() => withdraw.mutate(item)}
                  >
                    withdraw
                  </Button>
                </Flex>
              ))}
            </Stack>
          </Paper>
        ) : null}
        <Paper withBorder radius="md" style={{ overflow: "hidden" }}>
          <Box
            px="sm"
            py="sm"
            style={{
              maxHeight: "46vh",
              minHeight: 96,
              overflowY: "auto",
            }}
          >
            <Stack gap="xs">
              {list.map((block) => {
                const author = participants.find(
                  (participant) =>
                    participant.user.id === block.get("author"),
                );
                return (
                  <DraftBlockEditor
                    key={String(block.get("id"))}
                    block={block}
                    isMine={block.get("author") === userId}
                    authorName={author?.user.displayName}
                    authorColor={author?.user.color}
                    awareness={provider.awareness}
                  />
                );
              })}
            </Stack>
          </Box>
          <Flex
            align="center"
            gap="xs"
            px="xs"
            py="xs"
            style={{
              minHeight: 40,
              borderTop: "1px solid var(--mantine-color-default-border)",
            }}
          >
            <span
              aria-hidden
              title={
                status === "connected" && synced
                  ? "Drafts synced"
                  : "Draft connection unavailable"
              }
              style={{
                width: 6,
                height: 6,
                borderRadius: "50%",
                flexShrink: 0,
                background:
                  status === "connected" && synced
                    ? "var(--mantine-color-green-6)"
                    : status === "connected"
                      ? "var(--mantine-color-yellow-6)"
                      : "var(--mantine-color-red-6)",
              }}
            />
            {modelOption ? (
              <Select
                value={modelValue || null}
                onChange={(next) => {
                  if (next) {
                    setModelOverride(next);
                    onApplyConfig?.(next, effortValue);
                  }
                }}
                data={[
                  ...(!modelOption.options.some(
                    (value) => value.value === modelValue,
                  ) && modelValue
                    ? [{ value: modelValue, label: modelValue }]
                    : []),
                  ...modelOption.options.map((value) => ({
                    value: value.value,
                    label: value.name,
                  })),
                ]}
                placeholder="Model"
                aria-label="Model"
                size="xs"
                variant="unstyled"
                style={{ maxWidth: 190, fontSize: "var(--mantine-font-size-sm)" }}
              />
            ) : null}
            {effortOption ? (
              <Select
                value={effortValue || null}
                onChange={(next) => {
                  if (next) {
                    setEffortOverride(next);
                    onApplyConfig?.(modelValue, next);
                  }
                }}
                data={effortOption.options.map((value) => ({
                  value: value.value,
                  label: value.name,
                }))}
                placeholder="Effort"
                aria-label="Effort"
                size="xs"
                variant="unstyled"
                style={{ fontSize: "var(--mantine-font-size-sm)" }}
              />
            ) : null}
            <Popover
              opened={mentionOpen}
              onChange={setMentionOpen}
              position="top-start"
              width={208}
            >
              <Popover.Target>
                <Button
                  type="button"
                  variant="subtle"
                  color="dark"
                  size="xs"
                  aria-label="Mention a teammate"
                >
                  @
                </Button>
              </Popover.Target>
              <Popover.Dropdown p="xs">
                {participants.length === 0 ? (
                  <Text size="xs" c="dimmed" px="xs" py="xs">
                    No teammates to mention
                  </Text>
                ) : (
                  participants.map((participant) => (
                    <Button
                      key={participant.user.id}
                      type="button"
                      variant="subtle"
                      color="dark"
                      fullWidth
                      justify="flex-start"
                      size="xs"
                      leftSection={
                        <span
                          aria-hidden
                          style={{
                            width: 8,
                            height: 8,
                            borderRadius: "50%",
                            background: participant.user.color,
                          }}
                        />
                      }
                      rightSection={
                        <Text fz={11} c="dimmed">
                          @{participant.user.username}
                        </Text>
                      }
                      onClick={() => {
                        const own = blocks
                          .toArray()
                          .find(
                            (block) =>
                              block.get("author") === userId &&
                              block.get("text") instanceof Y.Text,
                          );
                        const text = own?.get("text");
                        if (text instanceof Y.Text) {
                          text.insert(
                            text.length,
                            `@${participant.user.username} `,
                          );
                        }
                        setMentionOpen(false);
                      }}
                    >
                      {participant.user.displayName}
                    </Button>
                  ))
                )}
              </Popover.Dropdown>
            </Popover>
            <Flex ml="auto" align="center" gap="xs">
              {threadStatus === "running" ? (
                <Button
                  type="button"
                  variant="subtle"
                  color="dark"
                  size="xs"
                  loading={stopTurn.isPending}
                  onClick={() => stopTurn.mutate()}
                >
                  <Square size={12} aria-hidden />
                  Stop
                </Button>
              ) : null}
              {threadStatus === "running" && synced ? (
                <Button
                  type="button"
                  variant="outline"
                  size="xs"
                  loading={send.isPending}
                  disabled={!hasText}
                  title={
                    showConfirmStop
                      ? "Cancels the current turn and any pending approvals"
                      : "Cancel the current turn and send this message"
                  }
                  onClick={() => {
                    if (showConfirmStop) {
                      sendWithRetention("stop_and_send");
                      setConfirmStop(null);
                    } else {
                      setConfirmStop({ status: threadStatus });
                    }
                  }}
                >
                  {showConfirmStop ? "Confirm stop & send" : "Stop & send"}
                </Button>
              ) : null}
              <Button
                type="button"
                variant="subtle"
                color="dark"
                size="xs"
                loading={send.isPending}
                disabled={!synced || !hasText}
                onClick={() => sendWithRetention("auto")}
              >
                <Send size={12} aria-hidden />
                Send
              </Button>
              <Menu shadow="md" width={200}>
                <Menu.Target>
                  <ActionIcon
                    type="button"
                    variant="subtle"
                    color="dark"
                    size="sm"
                    aria-label="More send options"
                    disabled={!synced || !hasText || send.isPending}
                  >
                    <ChevronDown size={14} aria-hidden />
                  </ActionIcon>
                </Menu.Target>
                <Menu.Dropdown>
                  <Menu.Item onClick={() => sendWithRetention("queue")}>
                    Queue after the current turn
                  </Menu.Item>
                  <Menu.Item onClick={() => sendWithRetention("inject")}>
                    Inject into current turn
                  </Menu.Item>
                </Menu.Dropdown>
              </Menu>
            </Flex>
          </Flex>
        </Paper>
      </Box>
    </Box>
  );
}

function DraftBlockEditor({
  block,
  isMine,
  authorName,
  authorColor,
  awareness,
}: {
  block: Y.Map<unknown>;
  isMine: boolean;
  authorName?: string;
  authorColor?: string | null;
  awareness: WebsocketProvider["awareness"];
}) {
  const text = block.get("text");
  const textRef = text instanceof Y.Text ? text : null;
  const [value, setValue] = useState(textRef?.toString() ?? "");
  const area = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (!textRef) {
      return;
    }
    const refresh = () => setValue(textRef.toString());
    textRef.observe(refresh);
    refresh();
    return () => textRef.unobserve(refresh);
  }, [textRef]);

  const [remoteEditors, setRemoteEditors] = useState<string[]>([]);
  const setLocal = usePresenceStore((state) => state.setLocal);

  useEffect(() => {
    const refresh = () => {
      const names: string[] = [];
      for (const [clientId, state] of awareness.getStates()) {
        if (clientId === awareness.clientID) {
          continue;
        }
        const editing = state.blockId;
        const user = state.user as
          | { displayName?: string; id?: string }
          | undefined;
        if (editing === block.get("id") && user?.displayName) {
          names.push(user.displayName);
        }
      }
      setRemoteEditors(names);
    };
    awareness.on("change", refresh);
    refresh();
    return () => awareness.off("change", refresh);
  }, [awareness, block]);

  if (!textRef) {
    return null;
  }

  const update = (next: string) => {
    setValue(next);
    const cursor = area.current?.selectionStart ?? next.length;
    const current = textRef.toString();
    let start = 0;
    while (
      start < current.length &&
      start < next.length &&
      current[start] === next[start]
    ) {
      start += 1;
    }
    let currentEnd = current.length;
    let nextEnd = next.length;
    while (
      currentEnd > start &&
      nextEnd > start &&
      current[currentEnd - 1] === next[nextEnd - 1]
    ) {
      currentEnd -= 1;
      nextEnd -= 1;
    }
    // Mutate only the changed range so concurrent edits merge at character level.
    textRef.doc?.transact(() => {
      if (currentEnd > start) {
        textRef.delete(start, currentEnd - start);
      }
      if (nextEnd > start) {
        textRef.insert(start, next.slice(start, nextEnd));
      }
    });
    requestAnimationFrame(() => {
      area.current?.setSelectionRange(cursor, cursor);
    });
  };

  return (
    <Box
      pl="xs"
      py="xs"
      style={{
        borderLeft: `2px solid ${authorColor ?? "var(--mantine-color-default-border)"}`,
      }}
    >
      <Flex align="center" gap="xs" fz={13} c="dimmed">
        <span
          aria-hidden
          style={{
            width: 6,
            height: 6,
            borderRadius: "50%",
            background:
              authorColor ?? "var(--mantine-color-default-border)",
            flexShrink: 0,
          }}
        />
        <Text component="span" fz={13} c={isMine ? "dimmed" : undefined}>
          {isMine ? "You" : (authorName ?? "Member")}
        </Text>
        {remoteEditors.length > 0 ? (
          <Text component="span" fz={13} c="blue.6">
            · {remoteEditors.join(", ")} editing
          </Text>
        ) : null}
      </Flex>
      <Textarea
        ref={area}
        value={value}
        rows={Math.max(4, Math.min(12, value.split("\n").length))}
        onChange={(event) => update(event.target.value)}
        onFocus={() => {
          awareness.setLocalStateField("blockId", block.get("id"));
          setLocal({ panel: "transcript", typing: true });
        }}
        onBlur={() => {
          awareness.setLocalStateField("blockId", null);
          setLocal({ typing: false });
        }}
        aria-label={isMine ? "Message" : "Edit shared draft"}
        placeholder={isMine ? "Message" : "Edit draft…"}
        variant="unstyled"
        resize="none"
        style={{
          padding: "4px 0",
          fontSize: "var(--mantine-font-size-lg)",
          lineHeight: 1.65,
          minHeight: 80,
        }}
      />
    </Box>
  );
}
