import Collaboration, {
  isChangeOrigin,
} from "@tiptap/extension-collaboration";
import CollaborationCaret from "@tiptap/extension-collaboration-caret";
import Placeholder from "@tiptap/extension-placeholder";
import { useEditor, type Extension } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { useEffect, useRef, useState } from "react";
import type { WebsocketProvider } from "y-websocket";
import type * as Y from "yjs";
import { notifications } from "@mantine/notifications";
import {
  DRAFT_FIELD,
  EDITORS_MAP,
  MAX_THREAD_DRAFT_BYTES,
  userColor,
} from "../../lib/draft";

interface UseComposerEditorOptions {
  agent: string;
  writable: boolean;
  /** Shared draft document; null in the design-preview composer. */
  document: Y.Doc | null;
  /** Draft awareness channel; null until the socket connects. */
  provider: WebsocketProvider | null;
  userId: string | null;
  userName: string | null;
  setDraftEditing: (active: boolean) => void;
  suggestionExtension: Extension;
}

/**
 * Owns the Tiptap instance. With a shared draft document the editor is bound to
 * `@tiptap/extension-collaboration`, so every participant types into one CRDT
 * document and sees the others' text and carets live; otherwise it is a plain
 * local editor.
 */
export function useComposerEditor({
  agent,
  writable,
  document,
  provider,
  userId,
  userName,
  setDraftEditing,
  suggestionExtension,
}: UseComposerEditorOptions) {
  const [hasText, setHasText] = useState(false);
  const lastAuthorship = useRef(0);
  const rejectedOversize = useRef(false);
  const accent = userId ? userColor(userId) : undefined;

  const editor = useEditor(
    {
      extensions: [
        StarterKit.configure({
          heading: false,
          blockquote: false,
          codeBlock: false,
          horizontalRule: false,
          bulletList: false,
          orderedList: false,
          listItem: false,
          // Collaboration provides its own Yjs-scoped undo history.
          undoRedo: document ? false : undefined,
        }),
        ...(document
          ? [Collaboration.configure({ document, field: DRAFT_FIELD })]
          : []),
        ...(document && provider && userName
          ? [
              CollaborationCaret.configure({
                provider,
                user: { name: userName, color: accent ?? "#74ade8" },
              }),
            ]
          : []),
        Placeholder.configure({
          placeholder: `Message ${agent}…`,
        }),
        suggestionExtension,
      ],
      editorProps: {
        attributes: {
          class: "composer-editor",
          "aria-label": "Message",
        },
      },
      // Viewers keep a read-only editor; `editable` is re-applied below because
      // the Tiptap React binding pins it to the instance's current value.
      editable: writable,
      onUpdate: ({ editor: current, transaction }) => {
        const text = current.getText();
        if (document && isChangeOrigin(transaction) === false) {
          // The server drops a room whose draft outgrows its cap; refuse the
          // edit locally instead of losing the draft for everyone.
          if (new TextEncoder().encode(text).length > MAX_THREAD_DRAFT_BYTES) {
            if (!rejectedOversize.current) {
              rejectedOversize.current = true;
              notifications.show({
                message: "Draft is limited to 200 KB",
                color: "red",
              });
              current.commands.undo();
            }
            return;
          }
          rejectedOversize.current = false;
        }
        setHasText(text.trim().length > 0);
        if (!document || !userId) return;
        // Remote CRDT transactions must not credit this author.
        if (isChangeOrigin(transaction)) return;
        const now = Date.now();
        if (now - lastAuthorship.current < 1000) return;
        lastAuthorship.current = now;
        document.getMap(EDITORS_MAP).set(userId, now);
      },
      onFocus: () => setDraftEditing(true),
      onBlur: () => setDraftEditing(false),
    },
    [document, provider],
  );

  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    editor.setEditable(writable);
  }, [editor, writable]);

  // The document can already carry text (reload, peer edits) before the first
  // keypress; keep the send button honest until an update arrives.
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    setHasText(editor.getText().trim().length > 0);
  }, [editor]);

  return { editor, hasText, setHasText };
}
