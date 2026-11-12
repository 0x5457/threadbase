import { ActionIcon, Group, Text } from "@mantine/core";
import { IconStack2, IconX } from "@tabler/icons-react";

export function QueueBar({
  items,
  busy,
  onDrop,
}: {
  items: { id: string; text: string; author: { displayName: string } }[];
  busy: string | null;
  onDrop: (id: string) => void;
}) {
  if (!items.length) return null;

  return (
    <div className="queue-bar">
      <Group gap={4}>
        <IconStack2 size={11} stroke={1.5} />
        <Text size="xs" c="dimmed">
          {items.length} queued
        </Text>
      </Group>
      {items.map((item) => (
        <Group key={item.id} gap={6} wrap="nowrap" className="queue-item">
          <Text size="xs" fw={500}>
            {item.author.displayName}
          </Text>
          <Text size="xs" c="dimmed" className="queue-text">
            {item.text}
          </Text>
          <ActionIcon
            size={18}
            aria-label="Drop queued message"
            loading={busy === item.id}
            onClick={() => onDrop(item.id)}
          >
            <IconX size={12} stroke={1.5} />
          </ActionIcon>
        </Group>
      ))}
    </div>
  );
}
