import { useEffect, useRef, useState } from "react";

/**
 * Hides a loader until it has been active for `delay` ms, then keeps it visible
 * for at least `minVisible` ms. Fast local reads never paint a spinner, and a
 * loader that does appear cannot flicker away in the next frame.
 */
export function useDelayedFlag(
  active: boolean,
  delay = 300,
  minVisible = 500,
): boolean {
  const [visible, setVisible] = useState(false);
  const shownAt = useRef(0);

  useEffect(() => {
    let showTimer: number | null = null;
    let hideTimer: number | null = null;
    if (active) {
      if (!visible) {
        showTimer = window.setTimeout(() => {
          shownAt.current = Date.now();
          setVisible(true);
        }, delay);
      }
    } else if (visible) {
      const remaining = Math.max(
        0,
        minVisible - (Date.now() - shownAt.current),
      );
      hideTimer = window.setTimeout(() => setVisible(false), remaining);
    }
    return () => {
      if (showTimer !== null) window.clearTimeout(showTimer);
      if (hideTimer !== null) window.clearTimeout(hideTimer);
    };
  }, [active, visible, delay, minVisible]);

  return visible;
}
