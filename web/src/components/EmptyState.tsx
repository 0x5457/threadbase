import { Box, Stack, Text } from "@mantine/core";
import type { ReactNode } from "react";

/**
 * One shell for every empty/loading surface, so blank screens read as a state
 * of the product rather than a missing panel.
 */
export function EmptyState({
  icon,
  title,
  children,
  hint,
  action,
}: {
  icon?: ReactNode;
  title: string;
  children?: ReactNode;
  hint?: string;
  /** Optional next step, rendered below the copy. */
  action?: ReactNode;
}) {
  return (
    <Box className="empty-state">
      <Stack align="center" gap={8}>
        <span className="empty-state-icon">
          {icon}
        </span>
        <Text size="sm" fw={500} className="empty-state-title">
          {title}
        </Text>
        {children ? (
          <Text size="sm" c="dimmed" ta="center" className="empty-state-body">
            {children}
          </Text>
        ) : null}
        {hint ? <Text className="empty-state-hint">{hint}</Text> : null}
        {action ? <Box mt={4}>{action}</Box> : null}
      </Stack>
    </Box>
  );
}
