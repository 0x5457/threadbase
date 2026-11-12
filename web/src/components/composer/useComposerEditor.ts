import Placeholder from "@tiptap/extension-placeholder";
import { useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { useEffect, useRef, useState } from "react";
import { detectSuggestion, type Suggestion } from "./commands";

interface UseComposerEditorOptions {
  agent: string;
  threadId?: string;
  writable: boolean;
  restoredText: string | null;
  mirrorDraft: (text: string) => void;
  setDraftEditing: (active: boolean) => void;
}

/** Owns the Tiptap instance, its local text state, and the draft restore. */
export function useComposerEditor({
  agent,
  threadId,
  writable,
  restoredText,
  mirrorDraft,
  setDraftEditing,
}: UseComposerEditorOptions) {
  const [hasText, setHasText] = useState(false);
  const [suggestion, setSuggestion] = useState<Suggestion | null>(null);
  const [active, setActive] = useState(0);
  const restoredThread = useRef<string | null>(null);

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: false,
        blockquote: false,
        codeBlock: false,
        horizontalRule: false,
        bulletList: false,
        orderedList: false,
        listItem: false,
      }),
      Placeholder.configure({
        placeholder: `Message ${agent}…`,
      }),
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
    onUpdate: ({ editor: current }) => {
      setHasText(current.getText().trim().length > 0);
      setSuggestion(detectSuggestion(current));
      // IME composition is reconciled on compositionend instead.
      if (!current.view.composing) mirrorDraft(current.getText());
    },
    onSelectionUpdate: ({ editor: current }) => {
      if (suggestion) setSuggestion(detectSuggestion(current));
    },
    onFocus: () => setDraftEditing(true),
    onBlur: ({ editor: current }) => {
      mirrorDraft(current.getText());
      setDraftEditing(false);
    },
  });

  useEffect(() => {
    editor?.setEditable(writable);
  }, [editor, writable]);

  // Reloading a thread keeps the author's pending text in the CRDT. Restore it
  // once so the draft stays editable — peers' text never enters this editor.
  useEffect(() => {
    if (!editor || !threadId) return;
    if (restoredText === null) return;
    if (restoredThread.current === threadId) return;
    restoredThread.current = threadId;
    if (editor.getText().length > 0 || restoredText.length === 0) return;
    editor.commands.setContent(restoredText, { emitUpdate: false });
    setHasText(restoredText.trim().length > 0);
  }, [editor, restoredText, threadId]);

  return {
    editor,
    hasText,
    setHasText,
    suggestion,
    setSuggestion,
    active,
    setActive,
  };
}
