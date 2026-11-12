import { useEffect, useRef, type PointerEvent } from "react";

const DISMISS_DISTANCE = 72;
const FLICK_DISTANCE = 24;
const FLICK_VELOCITY = 0.5;

/**
 * Drag handle for the phone bottom sheet. Dragging follows the finger and
 * dismisses the sheet past a distance or with a downward flick; tapping it
 * closes the sheet as well, so the gesture is never the only way out.
 */
export function SheetGrabber({ onClose }: { onClose: () => void }) {
  const handleRef = useRef<HTMLButtonElement>(null);
  const timerRef = useRef<number | null>(null);
  const suppressClick = useRef(false);
  const drag = useRef<{
    pointerId: number;
    startY: number;
    lastY: number;
    lastT: number;
    dy: number;
    velocity: number;
    moved: boolean;
  } | null>(null);

  useEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    },
    [],
  );

  const content = () =>
    handleRef.current?.closest<HTMLElement>(".mantine-Drawer-content") ?? null;

  const reset = () => {
    const el = content();
    if (!el) return;
    el.style.transition = "transform 180ms cubic-bezier(.22, 1, .36, 1)";
    el.style.transform = "";
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      el.style.transition = "";
      timerRef.current = null;
    }, 200);
  };

  const finish = (dismiss: boolean) => {
    const el = content();
    if (!el) {
      if (dismiss) onClose();
      return;
    }
    el.style.transition = "transform 160ms ease";
    el.style.transform = dismiss ? "translate3d(0, 100%, 0)" : "";
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(
      () => {
        if (dismiss) onClose();
        else {
          el.style.transition = "";
          el.style.transform = "";
        }
        timerRef.current = null;
      },
      dismiss ? 150 : 170,
    );
  };

  const onPointerDown = (event: PointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      pointerId: event.pointerId,
      startY: event.clientY,
      lastY: event.clientY,
      lastT: performance.now(),
      dy: 0,
      velocity: 0,
      moved: false,
    };
    const el = content();
    if (el) el.style.transition = "none";
  };

  const onPointerMove = (event: PointerEvent<HTMLButtonElement>) => {
    const current = drag.current;
    if (!current || current.pointerId !== event.pointerId) return;
    const offset = event.clientY - current.startY;
    current.dy = Math.max(0, offset);
    current.moved = current.moved || Math.abs(offset) > 6;
    const now = performance.now();
    const elapsed = Math.max(1, now - current.lastT);
    current.velocity = (event.clientY - current.lastY) / elapsed;
    current.lastY = event.clientY;
    current.lastT = now;
    const el = content();
    if (el) el.style.transform = `translate3d(0, ${current.dy}px, 0)`;
  };

  const endDrag = () => {
    const current = drag.current;
    if (!current) return;
    drag.current = null;
    suppressClick.current = current.moved;
    const dismiss =
      current.dy > DISMISS_DISTANCE ||
      (current.dy > FLICK_DISTANCE && current.velocity > FLICK_VELOCITY);
    if (dismiss) finish(true);
    else reset();
  };

  return (
    <button
      ref={handleRef}
      type="button"
      className="sheet-grabber"
      aria-label="Close panel"
      onClick={() => {
        if (suppressClick.current) {
          suppressClick.current = false;
          return;
        }
        onClose();
      }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={() => {
        drag.current = null;
        reset();
      }}
    />
  );
}
