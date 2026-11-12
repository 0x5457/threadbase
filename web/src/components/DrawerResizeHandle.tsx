import { useRef } from "react";

const MIN = 360;
const MAX = 760;

/**
 * Mouse/touch/pencil resize handle for the tablet changes drawer. The drag
 * follows the pointer directly on the Drawer content and only commits on
 * release, so resizing does not re-render the thread on every frame.
 */
export function DrawerResizeHandle({
  width,
  onResize,
}: {
  width: number;
  onResize: (width: number) => void;
}) {
  const handleRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{
    pointerId: number;
    startX: number;
    startWidth: number;
    width: number;
  } | null>(null);

  const content = () =>
    handleRef.current?.closest<HTMLElement>(".mantine-Drawer-content") ?? null;

  const clamp = (value: number) =>
    Math.max(
      MIN,
      Math.min(value, Math.min(MAX, Math.round(window.innerWidth * 0.8))),
    );

  return (
    <div
      ref={handleRef}
      className="drawer-resize"
      role="separator"
      aria-label="Resize changes panel"
      aria-orientation="vertical"
      tabIndex={0}
      title="Drag to resize"
      onDoubleClick={() => onResize(520)}
      onKeyDown={(event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
        event.preventDefault();
        onResize(clamp(width + (event.key === "ArrowLeft" ? 24 : -24)));
      }}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = {
          pointerId: event.pointerId,
          startX: event.clientX,
          startWidth: width,
          width,
        };
        const el = content();
        if (el) el.style.transition = "none";
      }}
      onPointerMove={(event) => {
        const current = drag.current;
        if (!current || current.pointerId !== event.pointerId) return;
        current.width = clamp(
          current.startWidth - (event.clientX - current.startX),
        );
        const el = content();
        if (el) el.style.width = `${current.width}px`;
      }}
      onPointerUp={() => {
        const current = drag.current;
        if (!current) return;
        drag.current = null;
        const el = content();
        if (el) el.style.transition = "";
        onResize(current.width);
      }}
      onPointerCancel={() => {
        drag.current = null;
      }}
    />
  );
}
