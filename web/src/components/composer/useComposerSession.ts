import { notifications } from "@mantine/notifications";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Editor } from "@tiptap/react";
import { useCallback, useState } from "react";
import {
  ensureAgentConfig,
  getAgentConfig,
  getMe,
  sendDraft,
  sendMessage,
} from "../../api";
import { useDraftSession, userColor as colorForUser } from "../../lib/draft";

export type ComposerDelivery = "auto" | "queue";

interface UseComposerSessionOptions {
  threadId?: string;
  agentName?: string;
  modelName?: string | null;
  effort?: string | null;
  onModelChange?: (model: string) => void;
  onSent?: () => void;
  userId?: string;
  userName?: string;
  canWrite?: boolean;
}

/**
 * Owns the composer's identity, shared draft, agent config, and delivery.
 * `send` takes the editor at call time so this hook can run before the
 * editor hook is created.
 */
export function useComposerSession({
  threadId,
  agentName,
  modelName,
  effort,
  onModelChange,
  onSent,
  userId,
  userName,
  canWrite,
}: UseComposerSessionOptions) {
  const queryClient = useQueryClient();
  const [sending, setSending] = useState(false);
  const [delivery, setDelivery] = useState<ComposerDelivery>("auto");
  const agent = agentName ?? "agent";
  const writable = canWrite !== false;
  const modelLabel = [agent, modelName, effort].filter(Boolean).join(" · ");

  // AuthGate already fetched this; the query is a cache read for the composer.
  const me = useQuery({
    queryKey: ["me"],
    queryFn: getMe,
    staleTime: 60_000,
  });
  const currentUserId = userId ?? me.data?.user.id ?? null;
  const currentUserAvatar = me.data?.user.avatarUrl ?? null;
  const currentUserName =
    userName ?? me.data?.user.displayName ?? me.data?.user.username ?? null;
  const draft = useDraftSession(
    threadId && currentUserId ? threadId : null,
    currentUserId && currentUserName
      ? { id: currentUserId, name: currentUserName }
      : null,
  );
  const {
    setMyText: mirrorDraft,
    markSent: markDraftSent,
    setEditing: setDraftEditing,
  } = draft;

  // Reading config must not start the adapter: opening a thread should not
  // spawn the agent process. An absent session reports no options.
  const agentConfig = useQuery({
    queryKey: ["agent-config", threadId],
    queryFn: () => getAgentConfig(threadId as string),
    enabled: Boolean(threadId) && Boolean(onModelChange),
    staleTime: 30_000,
    retry: false,
  });
  // Only an explicit picker open may start the adapter so it can advertise
  // its model and effort choices. The result fills the same config cache.
  const ensureConfigRequest = useMutation({
    mutationFn: (id: string) => ensureAgentConfig(id),
    onSuccess: (config, id) => {
      queryClient.setQueryData(["agent-config", id], config);
    },
  });
  const ensureConfig = useCallback(() => {
    if (!threadId || !onModelChange || ensureConfigRequest.isPending) return;
    ensureConfigRequest.mutate(threadId);
  }, [ensureConfigRequest, onModelChange, threadId]);
  const configLoading =
    agentConfig.isFetching || ensureConfigRequest.isPending;
  const modelOption = agentConfig.data?.options.find(
    (option) => option.category === "model",
  );
  const effortOption = agentConfig.data?.options.find(
    (option) => option.category === "thought_level",
  );
  const effectiveModel = modelName ?? modelOption?.currentValue ?? null;
  const effectiveEffort = effort ?? effortOption?.currentValue ?? null;

  const send = useCallback(
    async (
      editor: Editor | null,
      setHasText: (value: boolean) => void,
      resetSuggestion: () => void,
    ) => {
      if (!editor || sending || !writable) return;
      if (editor.getText().trim().length === 0) return;
      const shared = Boolean(threadId) && draft.active;
      if (shared && !draft.synced) {
        notifications.show({
          message: "Draft is still syncing with the thread",
          color: "yellow",
        });
        return;
      }
      resetSuggestion();
      const text = editor.getText().trim();

      if (!threadId) {
        editor.commands.clearContent();
        setHasText(false);
        notifications.show({ message: `Sent to ${agent}`, color: "teal" });
        return;
      }

      setSending(true);
      try {
        if (shared) {
          // The server reads the pending CRDT blocks, so the editor must stay
          // populated until the POST returns. Its tombstone clears the draft.
          const result = await sendDraft(threadId, delivery);
          // A successful send is visible in the timeline; only queueing, which
          // changes when the agent sees the message, is worth announcing.
          if (result.delivery === "queued") {
            notifications.show({
              message: "Queued for the next turn",
              color: "teal",
            });
          }
          // Allocate the next draft block before the editor clears, so the
          // cleared text does not allocate a second empty block.
          markDraftSent();
          editor.commands.clearContent();
          setHasText(false);
        } else {
          editor.commands.clearContent();
          setHasText(false);
          const result = await sendMessage(threadId, text, delivery);
          if (result.delivery === "queued") {
            notifications.show({
              message: "Queued for the next turn",
              color: "teal",
            });
          }
        }
        onSent?.();
        // The send started the adapter, so its options can now be read
        // without asking the server to ensure a session.
        void queryClient.invalidateQueries({
          queryKey: ["agent-config", threadId],
        });
      } catch (cause) {
        editor.commands.setContent(text);
        setHasText(true);
        if (shared) mirrorDraft(text);
        notifications.show({
          message:
            cause instanceof Error ? cause.message : "Could not send message",
          color: "red",
        });
      } finally {
        setSending(false);
      }
    },
    [
      agent,
      delivery,
      draft.active,
      draft.synced,
      markDraftSent,
      mirrorDraft,
      onSent,
      queryClient,
      sending,
      threadId,
      writable,
    ],
  );

  const authorName = (authorId: string) =>
    draft.remoteEditors.find((peer) => peer.id === authorId)?.name ?? "Teammate";
  const authorAccent = (authorId: string) =>
    draft.remoteEditors.find((peer) => peer.id === authorId)?.color ??
    colorForUser(authorId);
  const editingNames = Array.from(
    new Set(draft.remoteEditors.map((peer) => peer.name)),
  );
  const liveLabel = draft.active
    ? `${draft.synced && draft.connected ? "live" : draft.connected ? "syncing" : "offline"}${editingNames.length ? ` · ${editingNames.join(", ")} editing` : ""}`
    : "";
  const sendBlocked = draft.active && !draft.synced;
  const sendLabel = !writable
    ? "Viewers cannot send"
    : sendBlocked
      ? "Waiting for the shared draft to sync"
      : "Send  ⌘↵";

  return {
    agent,
    writable,
    modelLabel,
    sending,
    delivery,
    setDelivery,
    draft,
    mirrorDraft,
    setDraftEditing,
    send,
    ensureConfig,
    configLoading,
    modelOption,
    effortOption,
    effectiveModel,
    effectiveEffort,
    currentUserId,
    currentUserName,
    currentUserAvatar,
    authorName,
    authorAccent,
    liveLabel,
    sendBlocked,
    sendLabel,
  };
}
