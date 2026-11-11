/** Shared keyboard plumbing: edit-target detection and shortcut documentation. */

/**
 * True when a key event originated in a text field or inside an open overlay.
 * Global shortcuts must not steal keys from these surfaces: `⌘J` is a newline
 * in many editors, and `⌘P` belongs to the print dialog.
 */
export function isTypingTarget(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element) {
    return false;
  }
  if (element.isContentEditable) {
    return true;
  }
  const tag = element.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") {
    return true;
  }
  return Boolean(element.closest('[contenteditable="true"]'));
}

/** Overlays own their keyboard; only Escape and local combos should pass. */
export function isInsideOverlay(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element) {
    return false;
  }
  return Boolean(
    element.closest('[role="dialog"], [role="menu"], [role="listbox"]'),
  );
}

export function modKeyPressed(event: KeyboardEvent): boolean {
  return event.metaKey || event.ctrlKey;
}

export interface ShortcutDoc {
  keys: string;
  label: string;
}

/** Single list rendered by the `?` help dialog and kept in sync with handlers. */
export const SHORTCUTS: ShortcutDoc[] = [
  { keys: "mod+K", label: "Search the workspace" },
  { keys: "mod+Shift+P", label: "Command palette" },
  { keys: "mod+Shift+J", label: "Open Inbox" },
  { keys: "mod+Shift+A", label: "Add project" },
  { keys: "mod+Shift+S", label: "Share thread" },
  { keys: "mod+Shift+E", label: "Archive thread" },
  { keys: "mod+Shift+U", label: "Toggle read / unread" },
  { keys: "mod+J", label: "All subthreads" },
  { keys: "mod+Alt+M", label: "Comment on thread" },
  { keys: "mod+Alt+L", label: "Copy link to selection" },
  { keys: "mod+Alt+O", label: "Back to project" },
  { keys: "mod+Alt+F", label: "Follow next active member" },
  { keys: "Alt+Shift+D", label: "Open Changes" },
  { keys: "Alt+Shift+F", label: "Open Files" },
  { keys: "mod+Shift+M", label: "Open Comments" },
  { keys: "Ctrl+Tab", label: "Next thread" },
  { keys: "Shift+Esc", label: "Close sidebar / drawer" },
  { keys: "?", label: "Show this help" },
];
