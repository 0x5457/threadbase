import { ActionIcon, Loader } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import { useQueryClient } from "@tanstack/react-query";
import { IconFileText, IconX } from "@tabler/icons-react";
import { memo, useEffect, useRef, useState } from "react";
import type { ThreadSummary } from "../api";
import type { Usage } from "../api/types/Usage";
import { attachmentSize, isTextLike } from "../lib/attachments";
import { PickerAgentContext } from "../lib/model-prefs";
import { ComposerInput } from "./composer/ComposerInput";
import { ComposerSkeleton } from "./composer/ComposerSkeleton";
import { ComposerToolbar } from "./composer/ComposerToolbar";
import { useComposerEditor } from "./composer/useComposerEditor";
import { useComposerAttachments } from "./composer/useComposerAttachments";
import { useComposerSession } from "./composer/useComposerSession";
import {
  dismissComposerSuggestions,
  useComposerSuggestions,
} from "./composer/useComposerSuggestions";

/** Pastes longer than this become a text attachment instead of bloating the editor. */
const LARGE_PASTE_CHARS = 8_000;

interface ComposerProps {
  threadId?: string;
  projectId?: string;
  agentName?: string;
  modelName?: string | null;
  effort?: string | null;
  onModelChange?: (model: string) => void;
  onEffortChange?: (effort: string) => void;
  onSent?: () => void;
  onThreadComment?: () => void;
  /** Signed-in author; defaults to the cached `GET /api/auth/me` session. */
  userId?: string;
  userName?: string;
  userColor?: string;
  /** Viewers see the shared draft but cannot write or send. */
  canWrite?: boolean;
  /** Latest token/context usage reported by the agent. */
  usage?: Usage | null;
  /** Live presence count for this thread. */
  watching?: number;
  /** People currently looking at the terminal, by display name. */
  terminalUsers?: string[];
  /** When false, plain Enter sends and Shift+Enter inserts a newline. */
  sendWithModifier?: boolean;
  /** The agent is working: show a stop control next to the send button. */
  running?: boolean;
  stopping?: boolean;
  onStop?: () => void;
}

/**
 * The composer owns Tiptap, a CRDT provider and the agent-config queries. On a
 * phone that stack is the most expensive part of opening a thread, so the
 * transcript gets the first paint and the editor follows on the next idle
 * slot (bounded at 800 ms). A tap in the reserved composer area mounts it
 * immediately and focuses the editor.
 */
export const Composer = memo(function Composer(props: ComposerProps) {
  // Only compact/touch clients pay the bill for this deferral. A desktop
  // thread opens with a live editor in the first frame, as before. Read the
  // media query synchronously: Mantine's default returns false until its
  // effect runs, which would mount the editor before we know the client type.
  const compact = useMediaQuery(
    "(max-width: 74.99em), (pointer: coarse)",
    undefined,
    { getInitialValueInEffect: false },
  );
  const [ready, setReady] = useState(!compact);
  const [focusOnMount, setFocusOnMount] = useState(false);

  useEffect(() => {
    if (!compact) {
      setReady(true);
      return;
    }
    if (ready) return;
    let cancelled = false;
    let timeout = 0;
    let idle = 0;
    const idleCapable = window as Window & {
      requestIdleCallback?: (
        callback: () => void,
        options?: { timeout: number },
      ) => number;
      cancelIdleCallback?: (handle: number) => void;
    };
    const mount = () => {
      if (!cancelled) setReady(true);
    };
    const frame = requestAnimationFrame(() => {
      if (typeof idleCapable.requestIdleCallback === "function") {
        idle = idleCapable.requestIdleCallback(mount, { timeout: 800 });
      } else {
        // Safari still has no requestIdleCallback.
        timeout = window.setTimeout(mount, 300);
      }
    });
    return () => {
      cancelled = true;
      cancelAnimationFrame(frame);
      if (timeout) window.clearTimeout(timeout);
      if (idle) idleCapable.cancelIdleCallback?.(idle);
    };
  }, [compact, ready]);

  if (!ready) {
    return (
      <ComposerSkeleton
        agent={props.agentName}
        onActivate={() => {
          setFocusOnMount(true);
          setReady(true);
        }}
      />
    );
  }

  return <ComposerBody {...props} autoFocus={focusOnMount} />;
});

const ComposerBody = memo(function ComposerBody({
  threadId,
  projectId,
  agentName,
  modelName,
  effort,
  onModelChange,
  onEffortChange,
  onSent,
  onThreadComment,
  usage,
  watching,
  terminalUsers,
  sendWithModifier = false,
  userId,
  userName,
  userColor,
  canWrite,
  autoFocus = false,
  running = false,
  stopping = false,
  onStop,
}: ComposerProps & { autoFocus?: boolean }) {
  const session = useComposerSession({
    threadId,
    agentName,
    modelName,
    effort,
    onModelChange,
    onSent,
    userId,
    userName,
    canWrite,
    sendWithModifier,
  });
  const { extension, openTrigger, active: suggestionOpen } =
    useComposerSuggestions({
      threadId,
      projectId,
      agent: session.agent,
    });
  const { editor, hasText, setHasText } = useComposerEditor({
    agent: session.agent,
    writable: session.writable,
    document: session.draft.document,
    awareness: session.draft.awareness,
    userId: session.currentUserId,
    userName: session.currentUserName,
    setDraftEditing: session.setDraftEditing,
    suggestionExtension: extension,
    autoFocus,
  });
  const attachments = useComposerAttachments(threadId, session.writable);
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const queryClient = useQueryClient();
  const lastDraftThread = useRef(threadId);

  // The server persists the marker, but the author should see it as soon as
  // they type. Only set it here; clearing waits for the server flag so a
  // half-loaded editor cannot erase an existing draft marker.
  useEffect(() => {
    if (lastDraftThread.current !== threadId) {
      lastDraftThread.current = threadId;
      return;
    }
    if (!threadId || !hasText) return;
    queryClient.setQueryData<{ threads: ThreadSummary[] }>(
      ["threads-all"],
      (current) => {
        if (!current) return current;
        let changed = false;
        const threads = current.threads.map((thread) => {
          if (thread.id !== threadId || thread.hasDraft) return thread;
          changed = true;
          return { ...thread, hasDraft: true };
        });
        return changed ? { ...current, threads } : current;
      },
    );
  }, [hasText, queryClient, threadId]);

  const send = () => {
    void session
      .send(
        editor,
        setHasText,
        () => dismissComposerSuggestions(editor),
        attachments.attachmentIds,
      )
      .then((sent) => {
        if (sent) {
          attachments.clear();
          void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
        }
      });
  };

  // The model/effort pickers are instantiated inside the toolbar subtree; the
  // composer owns the agent they edit and keys their recent picks on it.
  return (
    <PickerAgentContext value={session.agent}>
      <section
        className="composer"
        onPasteCapture={(event) => {
          if (!session.writable) return;
          const files = Array.from(event.clipboardData?.files ?? []);
          if (files.length > 0) {
            event.preventDefault();
            event.stopPropagation();
            void attachments.accept(files);
            return;
          }
          const text = event.clipboardData?.getData("text/plain") ?? "";
          if (text.length < LARGE_PASTE_CHARS) return;
          event.preventDefault();
          event.stopPropagation();
          const stamp = new Date().toISOString().replace(/[:.]/g, "-");
          const file = new File([text], `pasted-${stamp}.txt`, {
            type: "text/plain",
          });
          void attachments.accept([file]).then((accepted) => {
            if (accepted) {
              notifications.show({
                message: `Pasted text attached as ${file.name}`,
                color: "teal",
              });
            } else {
              // Never lose a paste because the local upload failed.
              editor.chain().focus().insertContent({ type: "text", text }).run();
            }
          });
        }}
        onDragOver={(event) => {
          if (
            session.writable &&
            event.dataTransfer.types.includes("Files")
          ) {
            event.preventDefault();
          }
        }}
        onDrop={(event) => {
          if (!session.writable) return;
          const files = Array.from(event.dataTransfer.files);
          if (files.length === 0) return;
          event.preventDefault();
          event.stopPropagation();
          void attachments.accept(files);
        }}
        onKeyDownCapture={(event) => {
          // Enter sends from the editor, not from every control that happens
          // to live inside the composer: the model picker trigger and the
          // settings chips must keep their native Enter activation.
          const target = event.target;
          const inEditor =
            target instanceof Element &&
            Boolean(target.closest(".composer-editor"));
          if (
            inEditor &&
            !sendWithModifier &&
            event.key === "Enter" &&
            !event.shiftKey &&
            !event.metaKey &&
            !event.ctrlKey &&
            !event.nativeEvent.isComposing &&
            !suggestionOpen
          ) {
            event.preventDefault();
            event.stopPropagation();
            send();
          }
        }}
        onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return;
          if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
            event.preventDefault();
            send();
          }
        }}
      >
        <div className="composer-inner">
          <input
            ref={fileInputRef}
            type="file"
            multiple
            hidden
            onChange={(event) => {
              const files = Array.from(event.currentTarget.files ?? []);
              event.currentTarget.value = "";
              void attachments.accept(files);
            }}
          />
          {attachments.items.length > 0 ? (
            <div className="composer-attachments">
              {attachments.items.map((item) =>
                item.isImage ? (
                  <div key={item.key} className="composer-image">
                    <img src={item.url} alt={item.name} />
                    {item.status === "uploading" ? (
                      <span className="composer-image-busy">
                        <Loader size={14} />
                      </span>
                    ) : null}
                    <ActionIcon
                      className="composer-image-remove"
                      size={16}
                      variant="filled"
                      color="gray"
                      aria-label={`Remove ${item.name}`}
                      onClick={() => attachments.remove(item.key)}
                    >
                      <IconX size={10} stroke={2} />
                    </ActionIcon>
                  </div>
                ) : (
                  <div
                    key={item.key}
                    className="composer-file"
                    title={item.name}
                  >
                    <IconFileText size={18} stroke={1.5} />
                    <span className="composer-file-info">
                      <span className="composer-file-name">{item.name}</span>
                      <span className="composer-file-size">
                        {attachmentSize(item.byteSize)}
                        {isTextLike(item.mimeType, item.name) ? " · text" : ""}
                      </span>
                    </span>
                    {item.status === "uploading" ? (
                      <Loader size={12} />
                    ) : null}
                    <ActionIcon
                      size={16}
                      variant="filled"
                      color="gray"
                      aria-label={`Remove ${item.name}`}
                      onClick={() => attachments.remove(item.key)}
                    >
                      <IconX size={10} stroke={2} />
                    </ActionIcon>
                  </div>
                ),
              )}
            </div>
          ) : null}
          <ComposerInput
            editor={editor}
            hasText={hasText}
            writable={session.writable}
            readOnlyAccount={session.readOnlyAccount}
            agent={session.agent}
            userId={session.currentUserId}
            userName={session.currentUserName}
            avatarUrl={session.currentUserAvatar}
            userColor={userColor}
          />
          <ComposerToolbar
            usage={usage}
            watching={watching}
            terminalUsers={terminalUsers}
            draftActive={session.draft.active}
            draftPhase={session.draft.phase}
            liveLabel={session.liveLabel}
            onThreadComment={onThreadComment}
            onOpenTrigger={(kind) => openTrigger(kind, editor)}
            onPickFiles={() => fileInputRef.current?.click()}
            delivery={session.delivery}
            onDeliveryChange={session.setDelivery}
            modelLabel={session.modelLabel}
            effectiveModel={session.effectiveModel}
            modelOptions={session.modelOption?.options ?? []}
            effectiveEffort={session.effectiveEffort}
            effortOptions={session.effortOption?.options ?? []}
            configLoading={session.configLoading}
            writable={session.writable}
            onModelChange={onModelChange}
            onEffortChange={onEffortChange}
            onConfigOpen={session.ensureConfig}
            hasText={hasText || attachments.items.length > 0}
            sending={session.sending}
            sendBlocked={session.sendBlocked || attachments.uploading}
            sendLabel={session.sendLabel}
            sendWithModifier={sendWithModifier}
            running={running}
            stopping={stopping}
            onStop={onStop}
            onSend={send}
          />
        </div>
      </section>
    </PickerAgentContext>
  );
});
