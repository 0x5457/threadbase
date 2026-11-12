import {
  useEffect,
  useRef,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";

const MOVE_TOLERANCE = 10;

/**
 * Touch/pen long-press without stealing the normal click. Mouse pointers are
 * ignored so a desktop user can still click through; the press is cancelled as
 * soon as the finger moves, which keeps it compatible with the list scrolling
 * underneath.
 */
export function useLongPress(onLongPress: () => void, delay = 480) {
  const timer = useRef<number | null>(null);
  const origin = useRef<{ x: number; y: number } | null>(null);
  const held = useRef(false);
  const fired = useRef(false);
  const callback = useRef(onLongPress);
  callback.current = onLongPress;

  const clear = () => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
    origin.current = null;
    held.current = false;
  };

  useEffect(() => clear, []);

  return {
    onPointerDown: (event: ReactPointerEvent) => {
      // A click after a long press may be delivered by the mouse even on a
      // hybrid device; clear the suppression before bailing out for mice.
      fired.current = false;
      if (event.pointerType === "mouse") return;
      // Nested rows (a thread inside a project) both register a long press;
      // only the row under the finger should win.
      event.stopPropagation();
      origin.current = { x: event.clientX, y: event.clientY };
      if (timer.current !== null) window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => {
        timer.current = null;
        held.current = true;
        // Android gets a small confirmation; iOS has no vibration API.
        if ("vibrate" in navigator) navigator.vibrate?.(10);
      }, delay);
    },
    onPointerMove: (event: ReactPointerEvent) => {
      const start = origin.current;
      if (!start) return;
      if (
        Math.abs(event.clientX - start.x) > MOVE_TOLERANCE ||
        Math.abs(event.clientY - start.y) > MOVE_TOLERANCE
      ) {
        clear();
      }
    },
    onPointerUp: () => {
      // Open on release: opening a sheet while the finger is still down means
      // the release lands on the new overlay and can dismiss it immediately.
      if (held.current) {
        fired.current = true;
        callback.current();
      }
      clear();
    },
    onPointerCancel: clear,
    onPointerLeave: clear,
    onClickCapture: (event: ReactMouseEvent) => {
      if (!fired.current) return;
      fired.current = false;
      event.preventDefault();
      event.stopPropagation();
    },
  };
}
