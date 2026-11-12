import { useEffect, useRef } from "react";

/**
 * Makes the browser back button (and Android predictive back) close an overlay
 * instead of leaving the screen behind it.
 *
 * Opening pushes a history entry that carries the same URL as the current
 * route, so a back press produces no route change: the hook closes the overlay
 * and the app stays put. Closing from the UI removes the entry again, unless
 * an in-app navigation replaced it in the meantime.
 *
 * The push is deferred one task so React Strict Mode's mount → cleanup → mount
 * cycle cannot leave a duplicate entry behind.
 */
let overlaySequence = 0;

export function useOverlayBack(open: boolean, close: () => void) {
  const closeRef = useRef(close);
  closeRef.current = close;

  useEffect(() => {
    if (!open) return;
    const id = ++overlaySequence;
    let pushed = false;
    const timer = window.setTimeout(() => {
      window.history.pushState(
        { ...(window.history.state ?? {}), __tbOverlay: id },
        "",
      );
      pushed = true;
    }, 0);

    const onPop = () => closeRef.current();
    window.addEventListener("popstate", onPop);

    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("popstate", onPop);
      // Only unwind our own entry. A navigation that happened while the
      // overlay was open has already replaced the current state.
      if (pushed && window.history.state?.__tbOverlay === id) {
        window.history.back();
      }
    };
  }, [open]);
}
