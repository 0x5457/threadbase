import { ActionIcon, Group, Text } from "@mantine/core";
import {
  IconGripVertical,
  IconPencil,
  IconSend,
  IconStack2,
  IconX,
} from "@tabler/icons-react";
import {
  memo,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";
import type { QueueItem } from "../../api";

const SWIPE_DROP = 64;
const SWIPE_MAX = 120;

const QueueRow = memo(function QueueRow({
  item,
  busy,
  canWrite,
  canSteer,
  showAuthor,
  dragging,
  onDrop,
  onSteer,
  onEdit,
  onReorderStart,
  onReorderMove,
  onReorderEnd,
}: {
  item: QueueItem;
  busy: string | null;
  canWrite: boolean;
  canSteer: boolean;
  /** False when the previous row was written by the same author. */
  showAuthor: boolean;
  dragging: boolean;
  onDrop: (id: string) => void;
  onSteer: (id: string) => void;
  onEdit: (id: string, text: string) => Promise<boolean>;
  onReorderStart: (event: ReactPointerEvent, id: string) => void;
  onReorderMove: (event: ReactPointerEvent) => void;
  onReorderEnd: () => void;
}) {
  const actionable = canWrite && item.status === "queued";
  const [dx, setDx] = useState(0);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(item.text);
  const swipe = useRef<{ x: number; y: number; active: boolean } | null>(null);

  useEffect(() => {
    if (!editing) setDraft(item.text);
  }, [editing, item.text]);

  const resetSwipe = () => {
    swipe.current = null;
    setDx(0);
  };

  const cancelEdit = () => {
    setEditing(false);
    setDraft(item.text);
  };

  const saveEdit = async () => {
    const next = draft.trim();
    if (next.length === 0 || next === item.text) {
      cancelEdit();
      return;
    }
    if (await onEdit(item.id, next)) setEditing(false);
  };

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.pointerType === "mouse" || !actionable || editing) return;
    const target = event.target;
    if (target instanceof Element && target.closest("button, input, textarea"))
      return;
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
        {showAuthor ? (
          <Text size="xs" className="queue-author">
            {item.author.displayName}
          </Text>
        ) : null}
        {editing ? (
          <input
            className="queue-edit"
            value={draft}
            autoFocus
            aria-label="Edit queued message"
            onChange={(event) => setDraft(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void saveEdit();
              } else if (event.key === "Escape") {
                event.preventDefault();
                cancelEdit();
              }
            }}
            onBlur={() => {
              if (busy !== `edit:${item.id}`) cancelEdit();
            }}
          />
        ) : (
          <Text className="queue-text">{item.text}</Text>
        )}
        {actionable ? (
          <Group gap={0} wrap="nowrap" className="queue-actions">
            <ActionIcon
              size={16}
              variant="subtle"
              color="gray"
              aria-label="Send queued message now"
              title={
                canSteer
                  ? "Send now — steer into the running turn"
                  : "Steering needs a running turn"
              }
              loading={busy === `steer:${item.id}`}
              disabled={editing || !canSteer}
              onClick={() => onSteer(item.id)}
            >
              <IconSend size={11} stroke={1.5} />
            </ActionIcon>
            <ActionIcon
              size={16}
              variant="subtle"
              color="gray"
              aria-label="Edit queued message"
              title="Edit"
              disabled={busy !== null && busy !== `edit:${item.id}`}
              onClick={() => setEditing(true)}
            >
              <IconPencil size={11} stroke={1.5} />
            </ActionIcon>
            <ActionIcon
              size={16}
              variant="subtle"
              color="gray"
              aria-label="Drop queued message"
              title="Drop"
              loading={busy === `drop:${item.id}`}
              onClick={() => onDrop(item.id)}
            >
              <IconX size={12} stroke={1.5} />
            </ActionIcon>
          </Group>
        ) : null}
      </div>
    </div>
  );
});

export const QueueBar = memo(function QueueBar({
  items,
  busy,
  canWrite,
  canSteer,
  onDrop,
  onSteer,
  onEdit,
  onReorder,
}: {
  items: QueueItem[];
  busy: string | null;
  canWrite: boolean;
  canSteer: boolean;
  onDrop: (id: string) => void;
  onSteer: (id: string) => void;
  onEdit: (id: string, text: string) => Promise<boolean>;
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
      <Group gap={4} className="queue-head">
        <IconStack2 size={11} stroke={1.5} />
        <Text size="xs" c="dimmed">
          {items.length} queued
        </Text>
      </Group>
      {visible.map((item, index) => (
        <QueueRow
          key={item.id}
          item={item}
          busy={busy}
          canWrite={canWrite}
          canSteer={canSteer}
          showAuthor={index === 0 || visible[index - 1].author.id !== item.author.id}
          dragging={draggingId === item.id}
          onDrop={onDrop}
          onSteer={onSteer}
          onEdit={onEdit}
          onReorderStart={startDrag}
          onReorderMove={moveDrag}
          onReorderEnd={endDrag}
        />
      ))}
    </div>
  );
});
