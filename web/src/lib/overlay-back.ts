import { useEffect, useRef } from "react";

/**
 * Makes the browser back button (and Android predictive back) close an overlay
 * instead of leaving the screen behind it.
 *
 * Opening pushes a history entry with the current URL, so a back press produces
 * no route change. Overlays form a stack: only the topmost one reacts, which
 * matters for an action sheet opened from the still-open navigation drawer.
 *
 * The push is deferred one task so React Strict Mode's mount → cleanup → mount
 * cycle cannot leave a duplicate entry behind.
 */
let overlaySequence = 0;
const overlayStack: number[] = [];
let ignorePopUntil = 0;
let handlingPop = false;

export function useOverlayBack(open: boolean, close: () => void) {
  const closeRef = useRef(close);
  useEffect(() => {
    closeRef.current = close;
  }, [close]);

  // Both timers (the deferred history push and the pop handler release) are
  // owned by this effect and cleared in its cleanup below.
  // react-doctor-disable-next-line react-doctor/effect-needs-cleanup
  useEffect(() => {
    if (!open) return;
    const id = ++overlaySequence;
    let pushed = false;
    let releaseTimer = 0;
    const timer = window.setTimeout(() => {
      window.history.pushState(
        { ...(window.history.state ?? {}), __tbOverlay: id },
        "",
      );
      pushed = true;
      overlayStack.push(id);
    }, 0);

    const onPop = () => {
      if (performance.now() < ignorePopUntil || handlingPop) return;
      if (overlayStack[overlayStack.length - 1] !== id) return;
      handlingPop = true;
      releaseTimer = window.setTimeout(() => {
        handlingPop = false;
      }, 0);
      closeRef.current();
    };
    window.addEventListener("popstate", onPop);

    return () => {
      window.clearTimeout(timer);
      window.clearTimeout(releaseTimer);
      window.removeEventListener("popstate", onPop);
      const index = overlayStack.indexOf(id);
      if (index >= 0) overlayStack.splice(index, 1);
      // Only unwind our own entry. A navigation that happened while the
      // overlay was open has already replaced the current state. The pop that
      // removes our entry is not a user back press, so the overlay below it
      // must ignore it.
      if (pushed && window.history.state?.__tbOverlay === id) {
        ignorePopUntil = performance.now() + 80;
        window.history.back();
      }
    };
  }, [open]);
}
