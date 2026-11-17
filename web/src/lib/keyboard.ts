import type { KeyboardEvent as ReactKeyboardEvent } from "react";

/**
 * True when an `Enter` keydown is the IME confirming a candidate, not the user
 * submitting.
 *
 * With Chinese, Japanese or Korean input the conversion candidate is committed
 * with `Enter`, and that keystroke is also delivered to `keydown` handlers. A
 * handler that submits on `event.key === "Enter"` alone therefore sends the
 * pinyin/romaji that is still being composed.
 *
 * `isComposing` covers Chrome, Edge and Firefox. Safari dispatches
 * `compositionend` before the confirming keydown, so `isComposing` is already
 * false by then; the keystroke still reports `keyCode: 229` (VK_PROCESSKEY).
 * Modifier shortcuts such as ⌘/Ctrl+Enter never collide with IME confirmation
 * and do not need this check.
 */
export function isImeConfirming(
  event: ReactKeyboardEvent | globalThis.KeyboardEvent,
): boolean {
  const native = "nativeEvent" in event ? event.nativeEvent : event;
  return native.isComposing || native.keyCode === 229;
}
