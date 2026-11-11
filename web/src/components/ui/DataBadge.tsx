import { Badge } from "@mantine/core";
import type { ReactNode } from "react";

export interface DataBadgeProps {
  /** Badge text or number. */
  children: ReactNode;
  /** Visual variant. */
  tone?: "neutral" | "success" | "warning" | "danger" | "info";
  /** Optional leading dot or icon. */
  dot?: boolean;
}

/**
 * Compact data badge for counts, statuses, and metadata.
 *
 * Always small radius, light variant, uniform height.
 * Replaces ad-hoc Badge usage across the app.
 */
export function DataBadge({
  children,
  tone = "neutral",
  dot = false,
}: DataBadgeProps) {
  const colorMap = {
    neutral: "gray",
    success: "green",
    warning: "yellow",
    danger: "red",
    info: "blue",
  } as const;

  return (
    <Badge
      size="sm"
      radius="sm"
      variant="light"
      color={colorMap[tone]}
      leftSection={
        dot ? (
          <span
            style={{
              display: "inline-block",
              width: 6,
              height: 6,
              borderRadius: "50%",
              background: `var(--mantine-color-${colorMap[tone]}-filled)`,
            }}
          />
        ) : undefined
      }
    >
      {children}
    </Badge>
  );
}
