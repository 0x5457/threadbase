import { ActionIcon, Loader } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { IconX } from "@tabler/icons-react";
import { memo, useEffect, useState } from "react";
import type { Usage } from "../api/types/Usage";
import { PickerAgentContext } from "../lib/model-prefs";
import { ComposerInput } from "./composer/ComposerInput";
import { ComposerSkeleton } from "./composer/ComposerSkeleton";
import { ComposerToolbar } from "./composer/ComposerToolbar";
import { useComposerEditor } from "./composer/useComposerEditor";
import { useComposerImages } from "./composer/useComposerImages";
import { useComposerSession } from "./composer/useComposerSession";
import {
  dismissComposerSuggestions,
  useComposerSuggestions,
} from "./composer/useComposerSuggestions";

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
  const images = useComposerImages(threadId, session.writable);

  const send = () => {
    void session
      .send(
        editor,
        setHasText,
        () => dismissComposerSuggestions(editor),
        images.attachmentIds,
      )
      .then((sent) => {
        if (sent) images.clear();
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
          const files = Array.from(event.clipboardData?.files ?? []).filter(
            (file) => file.type.startsWith("image/"),
          );
          if (files.length === 0) return;
          event.preventDefault();
          event.stopPropagation();
          void images.accept(files);
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
          {images.images.length > 0 ? (
            <div className="composer-images">
              {images.images.map((image) => (
                <div key={image.key} className="composer-image">
                  <img src={image.url} alt="" />
                  {image.status === "uploading" ? (
                    <span className="composer-image-busy">
                      <Loader size={14} />
                    </span>
                  ) : null}
                  <ActionIcon
                    className="composer-image-remove"
                    size={16}
                    variant="filled"
                    color="gray"
                    aria-label="Remove image"
                    onClick={() => images.remove(image.key)}
                  >
                    <IconX size={10} stroke={2} />
                  </ActionIcon>
                </div>
              ))}
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
            hasText={hasText || images.images.length > 0}
            sending={session.sending}
            sendBlocked={session.sendBlocked || images.uploading}
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
