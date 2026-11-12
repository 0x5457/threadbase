import { PickerAgentContext } from "../lib/model-prefs";
import { PendingBlocks } from "./composer/PendingBlocks";
import { ComposerInput } from "./composer/ComposerInput";
import { ComposerToolbar } from "./composer/ComposerToolbar";
import { useComposerEditor } from "./composer/useComposerEditor";
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
  /** Live presence count for this thread. */
  watching?: number;
  /** When false, plain Enter sends and Shift+Enter inserts a newline. */
  sendWithModifier?: boolean;
}

export function Composer({
  threadId,
  projectId,
  agentName,
  modelName,
  effort,
  onModelChange,
  onEffortChange,
  onSent,
  onThreadComment,
  watching,
  sendWithModifier = true,
  userId,
  userName,
  userColor,
  canWrite,
}: ComposerProps) {
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
  });
  const { extension, openTrigger, active: suggestionOpen } =
    useComposerSuggestions({
      threadId,
      projectId,
      agent: session.agent,
    });
  const { editor, hasText, setHasText } = useComposerEditor({
    agent: session.agent,
    threadId,
    writable: session.writable,
    restoredText: session.draft.restoredText,
    mirrorDraft: session.mirrorDraft,
    setDraftEditing: session.setDraftEditing,
    suggestionExtension: extension,
  });

  const send = () => {
    void session.send(editor, setHasText, () =>
      dismissComposerSuggestions(editor),
    );
  };

  // The model/effort pickers are instantiated inside the toolbar subtree; the
  // composer owns the agent they edit and keys their recent picks on it.
  return (
    <PickerAgentContext value={session.agent}>
      <section
        className="composer"
        onKeyDownCapture={(event) => {
          if (
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
        onCompositionEnd={() => {
          if (editor) session.mirrorDraft(editor.getText());
        }}
      >
        <div className="composer-inner">
          <PendingBlocks
            blocks={session.draft.remoteBlocks}
            authorName={session.authorName}
            authorAccent={session.authorAccent}
          />
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
            agent={session.agent}
            watching={watching}
            draftActive={session.draft.active}
            draftSynced={session.draft.synced}
            draftConnected={session.draft.connected}
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
            hasText={hasText}
            sending={session.sending}
            sendBlocked={session.sendBlocked}
            sendLabel={session.sendLabel}
            onSend={send}
          />
        </div>
      </section>
    </PickerAgentContext>
  );
}
