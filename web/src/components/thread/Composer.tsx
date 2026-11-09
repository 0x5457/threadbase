import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ChevronDown, Send, Users } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { WebsocketProvider } from "y-websocket";
import * as Y from "yjs";
import { sendMessage } from "@/api/endpoints";
import { queryKeys, useMe } from "@/api/queries";
import type { DeliveryMode } from "@/api/types/DeliveryMode";
import type { Participant } from "@/api/types/Participant";
import { Button } from "@/components/ui";
import { usePresenceStore } from "@/lib/presence";
import { cn } from "@/lib/utils";

interface ComposerProps {
  threadId: string;
  canWrite: boolean;
  threadStatus: string;
  participants?: Participant[];
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
  threadStatus,
  participants = [],
}: ComposerProps) {
  const me = useMe();
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

  const send = useMutation({
    mutationFn: (delivery: DeliveryMode | null) =>
      sendMessage(threadId, {
        text: "",
        delivery,
        fromDraft: true,
      }),
    onSuccess: (response) => {
      if (response.delivery === "queued") {
        toast.success(
          response.queuePosition
            ? `Queued at position ${response.queuePosition}`
            : "Queued",
        );
      } else {
        toast.success(
          response.delivery === "stopped_and_sent"
            ? "Stopped the previous turn and sent"
            : "Sent",
        );
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
    },
    onError: (error) => toast.error(error instanceof Error ? error.message : String(error)),
  });

  if (!canWrite) {
    return (
      <div className="border-t border-border bg-panel px-4 py-3 text-center text-[11px] text-muted">
        <Users className="mr-1.5 inline size-3.5" aria-hidden />
        You are viewing this thread read-only. Ask an owner for edit access.
      </div>
    );
  }

  return (
    <div
      className="border-t border-border bg-panel/80 px-3 py-2.5"
      onKeyDown={(event) => {
        if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
          event.preventDefault();
          if (synced && hasText && !send.isPending) {
            send.mutate("auto");
          }
        }
      }}
    >
      <div className="mx-auto w-full max-w-3xl">
        <div className="rounded-lg border border-border bg-surface">
          <div className="max-h-64 overflow-y-auto px-3 pt-2">
            {list.length === 0 ? (
              <p className="py-2 text-[12px] text-faint">
                Start typing to add a shared draft block…
              </p>
            ) : (
              list.map((block) => (
                <DraftBlockEditor
                  key={String(block.get("id"))}
                  block={block}
                  isMine={block.get("author") === userId}
                  awareness={provider.awareness}
                />
              ))
            )}
          </div>
          <div className="flex items-center gap-2 border-t border-border/70 px-2 py-1.5">
            <span
              className={cn(
                "size-1.5 rounded-full",
                status === "connected" && synced
                  ? "bg-success"
                  : status === "connected"
                    ? "bg-warning"
                    : "bg-danger",
              )}
              title={
                status === "connected" && synced
                  ? "Drafts synced"
                  : "Draft connection unavailable"
              }
            />
            <span className="text-[10px] text-faint">
              {threadStatus === "running"
                ? "Agent is running · input will steer or queue"
                : "Shared draft · agent sees it only after send"}
            </span>
            <div className="relative">
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => setMentionOpen((open) => !open)}
                aria-label="Mention a teammate"
              >
                @
              </Button>
              {mentionOpen ? (
                <div className="absolute bottom-8 left-0 z-20 w-52 overflow-hidden rounded-md border border-border bg-panel py-1 shadow-xl">
                  {participants.length === 0 ? (
                    <p className="px-2 py-1.5 text-[10.5px] text-faint">
                      No teammates to mention
                    </p>
                  ) : (
                    participants.map((participant) => (
                      <button
                        key={participant.user.id}
                        type="button"
                        className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-[11px] text-fg hover:bg-hover"
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
                            text.insert(text.length, `@${participant.user.username} `);
                          }
                          setMentionOpen(false);
                        }}
                      >
                        <span
                          className="size-2 rounded-full"
                          style={{ background: participant.user.color }}
                        />
                        {participant.user.displayName}
                        <span className="ml-auto text-[9.5px] text-faint">
                          @{participant.user.username}
                        </span>
                      </button>
                    ))
                  )}
                </div>
              ) : null}
            </div>
            <div className="ml-auto flex items-center">
              {threadStatus === "running" && synced ? (
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  loading={send.isPending}
                  disabled={!hasText}
                  onClick={() => send.mutate("stop_and_send")}
                >
                  Stop & send
                </Button>
              ) : null}
              <Button
                type="button"
                size="sm"
                variant="ghost"
                className="ml-1"
                loading={send.isPending}
                disabled={!synced || !hasText}
                onClick={() => send.mutate("auto")}
              >
                <Send aria-hidden />
                Send
              </Button>
              <DropdownMenu.Root>
                <DropdownMenu.Trigger asChild>
                  <Button
                    type="button"
                    size="icon-sm"
                    variant="ghost"
                    aria-label="More send options"
                    disabled={!synced || !hasText || send.isPending}
                  >
                    <ChevronDown aria-hidden />
                  </Button>
                </DropdownMenu.Trigger>
                <DropdownMenu.Portal>
                  <DropdownMenu.Content
                    className="z-50 min-w-44 rounded-lg border border-border bg-panel p-1 shadow-xl"
                    sideOffset={6}
                    align="end"
                  >
                    <DropdownMenu.Item
                      className="cursor-pointer rounded px-2 py-1.5 text-[12px] outline-none data-[highlighted]:bg-hover"
                      onSelect={() => send.mutate("queue")}
                    >
                      Queue after the current turn
                    </DropdownMenu.Item>
                    <DropdownMenu.Item
                      className="cursor-pointer rounded px-2 py-1.5 text-[12px] outline-none data-[highlighted]:bg-hover"
                      onSelect={() => send.mutate("inject")}
                    >
                      Inject into current turn
                    </DropdownMenu.Item>
                  </DropdownMenu.Content>
                </DropdownMenu.Portal>
              </DropdownMenu.Root>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function DraftBlockEditor({
  block,
  isMine,
  awareness,
}: {
  block: Y.Map<unknown>;
  isMine: boolean;
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
    <div className="group py-1">
      <div className="flex items-center gap-1.5 text-[9.5px] uppercase tracking-wide text-faint">
        <span className={cn("size-1.5 rounded-full", isMine ? "bg-accent" : "bg-muted")} />
        {isMine ? "You" : String(block.get("author") ?? "member")}
        {remoteEditors.length > 0 ? (
          <span className="normal-case text-info">
            · {remoteEditors.join(", ")} editing
          </span>
        ) : null}
      </div>
      <textarea
        ref={area}
        value={value}
        rows={Math.max(1, Math.min(6, value.split("\n").length))}
        onChange={(event) => update(event.target.value)}
        onFocus={() => {
          awareness.setLocalStateField("blockId", block.get("id"));
          setLocal({ panel: "transcript", typing: true });
        }}
        onBlur={() => {
          awareness.setLocalStateField("blockId", null);
          setLocal({ typing: false });
        }}
        placeholder={isMine ? "Write a message…" : "Edit this shared draft…"}
        className="mt-0.5 w-full resize-none bg-transparent text-[12.5px] leading-relaxed text-fg outline-none placeholder:text-faint"
      />
    </div>
  );
}
