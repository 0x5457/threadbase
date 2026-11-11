import { Group, Text } from "@mantine/core";
import type { ReactNode } from "react";

export interface SectionHeaderProps {
  /** Section title. */
  title: ReactNode;
  /** Optional leading icon. */
  icon?: ReactNode;
  /** Right-aligned actions. */
  actions?: ReactNode;
  /** Bottom margin. */
  mb?: number | string;
}

/**
 * Consistent section header for cards, settings panels, and grouped content.
 *
 * Uses 14px medium-weight title + optional 14px inline icon.
 * Replaces the old 32px ThemeIcon block that looked like a button.
 */
export function SectionHeader({
  title,
  icon,
  actions,
  mb = "md",
}: SectionHeaderProps) {
  return (
    <Group justify="space-between" align="center" gap="sm" mb={mb}>
      <Group gap="xs" align="center" wrap="nowrap">
        {icon ? (
          <Text c="dimmed" style={{ display: "flex", alignItems: "center" }}>
            {icon}
          </Text>
        ) : null}
        <Text size="md" fw={600}>
          {title}
        </Text>
      </Group>
      {actions ? (
        <Group gap="xs" wrap="nowrap">
          {actions}
        </Group>
      ) : null}
    </Group>
  );
}
