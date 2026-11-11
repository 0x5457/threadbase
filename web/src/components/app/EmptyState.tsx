import type { LucideIcon } from "lucide-react";
import { Inbox } from "lucide-react";
import type { ReactNode } from "react";
import { Stack, Text, ThemeIcon } from "@mantine/core";

export interface EmptyStateProps {
  icon?: LucideIcon;
  title: string;
  description?: string;
  action?: ReactNode;
  className?: string;
  compact?: boolean;
}

export function EmptyState({
  icon: Icon = Inbox,
  title,
  description,
  action,
  className,
  compact = false,
}: EmptyStateProps) {
  return (
    <Stack
      align="center"
      className={className}
      gap={compact ? 8 : 12}
      px={compact ? 16 : 24}
      py={compact ? 24 : 48}
    >
      <ThemeIcon
        variant="light"
        color="gray"
        size={32}
        radius="md"
      >
        <Icon size={16} aria-hidden />
      </ThemeIcon>
      <Stack gap={4} align="center">
        <Text size="sm" fw={500} ta="center">
          {title}
        </Text>
        {description ? (
          <Text size="xs" c="dimmed" ta="center" maw={384}>
            {description}
          </Text>
        ) : null}
      </Stack>
      {action ? (
        <Stack align="center" pt={4}>
          {action}
        </Stack>
      ) : null}
    </Stack>
  );
}
