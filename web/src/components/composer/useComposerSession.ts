import { notifications } from "@mantine/notifications";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useState } from "react";
import {
  ensureAgentConfig,
  getAgentConfig,
  getMe,
  sendDraft,
  sendMessage,
} from "../../api";
import { useDraftSession } from "../../lib/draft";
import { stateLabel } from "../../lib/machine";

export type ComposerDelivery = "auto" | "queue";

function readDelivery(threadId: string | undefined): ComposerDelivery {
  if (!threadId) return "auto";
  try {
    return window.localStorage.getItem(`threadbase.delivery.${threadId}`) ===
      "queue"
      ? "queue"
      : "auto";
  } catch {
    return "auto";
  }
}

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
  sendWithModifier?: boolean;
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
  sendWithModifier = false,
}: UseComposerSessionOptions) {
  const queryClient = useQueryClient();
  const [sending, setSending] = useState(false);
  const [delivery, setDeliveryState] = useState<ComposerDelivery>(() =>
    readDelivery(threadId),
  );
  const agent = agentName ?? "agent";
  const modelLabel = [agent, modelName, effort].filter(Boolean).join(" · ");

  // AuthGate already fetched this; the query is a cache read for the composer.
  // Delivery is a per-thread preference: switching threads (or reloading)
  // restores what this thread used last instead of snapping back to auto.
  useEffect(() => {
    setDeliveryState(readDelivery(threadId));
  }, [threadId]);

  const setDelivery = useCallback(
    (next: ComposerDelivery) => {
      setDeliveryState(next);
      if (!threadId) return;
      try {
        window.localStorage.setItem(`threadbase.delivery.${threadId}`, next);
      } catch {
        // Private mode: the preference simply does not persist.
      }
    },
    [threadId],
  );

  const me = useQuery({
    queryKey: ["me"],
    queryFn: getMe,
    staleTime: 60_000,
  });
  const readOnlyAccount = me.data?.user.access === "read_only";
  const writable = canWrite !== false && !readOnlyAccount;
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
  const { setEditing: setDraftEditing } = draft;

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
      attachmentIds: string[] = [],
    ): Promise<boolean> => {
      if (!editor || sending || !writable) return false;
      if (editor.getText().trim().length === 0 && attachmentIds.length === 0) {
        return false;
      }
      const shared = Boolean(threadId) && draft.active;
      if (shared && draft.phase !== "live") {
        notifications.show({
          message: "Draft is still syncing with the thread",
          color: "yellow",
        });
        return false;
      }
      resetSuggestion();
      const text = editor.getText().trim();

      if (!threadId) {
        editor.commands.clearContent();
        setHasText(false);
        notifications.show({ message: `Sent to ${agent}`, color: "teal" });
        return true;
      }

      setSending(true);
      try {
        if (shared) {
          // The server consumes the shared CRDT draft and clears it for every
          // open composer, so the editor empties through the collaboration
          // update instead of a local command.
          await sendDraft(threadId, delivery, attachmentIds);
        } else {
          editor.commands.clearContent();
          setHasText(false);
          await sendMessage(threadId, text, delivery, attachmentIds);
        }
        onSent?.();
        // The send started the adapter, so its options can now be read
        // without asking the server to ensure a session.
        void queryClient.invalidateQueries({
          queryKey: ["agent-config", threadId],
        });
        return true;
      } catch (cause) {
        if (!shared) {
          editor.commands.setContent(text);
          setHasText(true);
        }
        notifications.show({
          message:
            cause instanceof Error ? cause.message : "Could not send message",
          color: "red",
        });
        return false;
      } finally {
        setSending(false);
      }
    },
    [
      agent,
      delivery,
      draft.active,
      draft.phase,
      onSent,
      queryClient,
      sending,
      threadId,
      writable,
    ],
  );

  const editingNames = Array.from(
    new Set(draft.remoteEditors.map((peer) => peer.name)),
  );
  const liveLabel = draft.active
    ? `${stateLabel("draft", draft.phase).toLowerCase()}${editingNames.length ? ` · ${editingNames.join(", ")} editing` : ""}`
    : "";
  const sendBlocked = draft.active && draft.phase !== "live";
  const sendLabel = readOnlyAccount
    ? "Read-only account: ask an admin for editing access"
    : !writable
      ? "Viewers cannot send"
      : sendBlocked
        ? "Waiting for the shared draft to sync"
        : `Send  ${sendWithModifier ? "⌘↵" : "↵"}`;

  return {
    agent,
    writable,
    readOnlyAccount,
    modelLabel,
    sending,
    delivery,
    setDelivery,
    draft,
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
    liveLabel,
    sendBlocked,
    sendLabel,
  };
}
