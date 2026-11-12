import { ActionIcon, Group, Text } from "@mantine/core";
import { IconGripVertical, IconStack2, IconX } from "@tabler/icons-react";
import {
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";
import type { QueueItem } from "../../api";

const SWIPE_DROP = 64;
const SWIPE_MAX = 120;

function QueueRow({
  item,
  busy,
  dragging,
  onDrop,
  onReorderStart,
  onReorderMove,
  onReorderEnd,
}: {
  item: QueueItem;
  busy: string | null;
  dragging: boolean;
  onDrop: (id: string) => void;
  onReorderStart: (event: ReactPointerEvent, id: string) => void;
  onReorderMove: (event: ReactPointerEvent) => void;
  onReorderEnd: () => void;
}) {
  const actionable = item.status === "queued";
  const [dx, setDx] = useState(0);
  const swipe = useRef<{ x: number; y: number; active: boolean } | null>(null);

  const resetSwipe = () => {
    swipe.current = null;
    setDx(0);
  };

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.pointerType === "mouse" || !actionable) return;
    const target = event.target;
    if (target instanceof Element && target.closest("button")) return;
    swipe.current = { x: event.clientX, y: event.clientY, active: false };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const current = swipe.current;
    if (!current) return;
    const dxValue = event.clientX - current.x;
    const dyValue = event.clientY - current.y;
    if (!current.active) {
      if (Math.abs(dxValue) < 8) return;
      if (Math.abs(dyValue) > Math.abs(dxValue)) {
        swipe.current = null;
        return;
      }
      current.active = true;
    }
    if (dxValue < 0) setDx(Math.max(dxValue, -SWIPE_MAX));
  };

  const endSwipe = () => {
    if (!swipe.current) return;
    const dropped = dx <= -SWIPE_DROP;
    resetSwipe();
    if (dropped) onDrop(item.id);
  };

  return (
    <div
      className={`queue-item${dragging ? " dragging" : ""}`}
      data-status={item.status}
    >
      <div className="queue-drop-hint" aria-hidden="true">
        Drop
      </div>
      <div
        className="queue-item-body"
        style={dx ? { transform: `translateX(${dx}px)` } : undefined}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endSwipe}
        onPointerCancel={resetSwipe}
      >
        <button
          type="button"
          className="queue-grip"
          aria-label="Reorder queued message"
          disabled={!actionable}
          onPointerDown={(event) => onReorderStart(event, item.id)}
          onPointerMove={onReorderMove}
          onPointerUp={onReorderEnd}
          onPointerCancel={onReorderEnd}
        >
          <IconGripVertical size={12} stroke={1.5} />
        </button>
        <Text size="xs" fw={500}>
          {item.author.displayName}
        </Text>
        <Text size="xs" c="dimmed" className="queue-text">
          {item.text}
        </Text>
        {actionable ? (
          <ActionIcon
            size={18}
            aria-label="Drop queued message"
            loading={busy === item.id}
            onClick={() => onDrop(item.id)}
          >
            <IconX size={12} stroke={1.5} />
          </ActionIcon>
        ) : null}
      </div>
    </div>
  );
}

export function QueueBar({
  items,
  busy,
  onDrop,
  onReorder,
}: {
  items: QueueItem[];
  busy: string | null;
  onDrop: (id: string) => void;
  onReorder: (ids: string[]) => void;
}) {
  const [order, setOrder] = useState<string[]>([]);
  const orderRef = useRef<string[]>([]);
  const drag = useRef<{
    id: string;
    index: number;
    startY: number;
    rowHeight: number;
    moved: boolean;
  } | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);

  useEffect(() => {
    if (drag.current) return;
    const next = items.map((item) => item.id);
    orderRef.current = next;
    setOrder(next);
  }, [items]);

  const byId = new Map(items.map((item) => [item.id, item]));
  const visible: QueueItem[] = [];
  for (const id of order) {
    const item = byId.get(id);
    if (item) visible.push(item);
  }
  for (const item of items) {
    if (!order.includes(item.id)) visible.push(item);
  }

  const startDrag = (event: ReactPointerEvent, id: string) => {
    event.preventDefault();
    event.stopPropagation();
    const index = visible.findIndex((item) => item.id === id);
    if (index < 0) return;
    const row = (event.currentTarget as Element).closest(".queue-item");
    const rowHeight = row?.getBoundingClientRect().height || 40;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    drag.current = {
      id,
      index,
      startY: event.clientY,
      rowHeight,
      moved: false,
    };
    setDraggingId(id);
  };

  const moveDrag = (event: ReactPointerEvent) => {
    const current = drag.current;
    if (!current || event.pointerId === undefined) return;
    const step = Math.round((event.clientY - current.startY) / current.rowHeight);
    const target = Math.max(
      0,
      Math.min(visible.length - 1, current.index + step),
    );
    if (target === current.index) return;
    const ids = visible.map((item) => item.id);
    const [moved] = ids.splice(current.index, 1);
    ids.splice(target, 0, moved);
    orderRef.current = ids;
    setOrder(ids);
    current.index = target;
    current.startY = event.clientY;
    current.moved = true;
  };

  const endDrag = () => {
    const current = drag.current;
    drag.current = null;
    setDraggingId(null);
    if (current?.moved) onReorder(orderRef.current);
  };

  if (!items.length) return null;

  return (
    <div className="queue-bar">
      <Group gap={4}>
        <IconStack2 size={11} stroke={1.5} />
        <Text size="xs" c="dimmed">
          {items.length} queued
        </Text>
      </Group>
      {visible.map((item) => (
        <QueueRow
          key={item.id}
          item={item}
          busy={busy}
          dragging={draggingId === item.id}
          onDrop={onDrop}
          onReorderStart={startDrag}
          onReorderMove={moveDrag}
          onReorderEnd={endDrag}
        />
      ))}
    </div>
  );
}
