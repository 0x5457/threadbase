import { Button, Group, Text } from "@mantine/core";
import { IconAlertCircle, IconRefresh } from "@tabler/icons-react";

/**
 * A failure stays where the thing failed. A toast is fine for a result; it is
 * not enough for something the person has to fix.
 */
export function LoadError({
  title = "Could not load",
  message,
  onRetry,
  retryLabel = "Retry",
  compact = false,
  className = "",
}: {
  title?: string;
  message?: string;
  onRetry?: () => void;
  retryLabel?: string;
  compact?: boolean;
  className?: string;
}) {
  return (
    <div
      className={`load-error${compact ? " is-compact" : ""}${
        className ? ` ${className}` : ""
      }`}
      role="alert"
    >
      <Group gap={compact ? 6 : 8} wrap="nowrap" align="flex-start">
        <IconAlertCircle
          size={compact ? 13 : 16}
          stroke={1.5}
          className="load-error-icon"
          aria-hidden="true"
        />
        <div className="load-error-copy">
          <Text size={compact ? "xs" : "sm"} fw={500}>
            {title}
          </Text>
          {message ? (
            <Text size="xs" c="dimmed">
              {message}
            </Text>
          ) : null}
        </div>
        {onRetry ? (
          <Button
            variant="default"
            size="compact-xs"
            leftSection={<IconRefresh size={11} stroke={1.5} />}
            onClick={onRetry}
            className="load-error-retry"
          >
            {retryLabel}
          </Button>
        ) : null}
      </Group>
    </div>
  );
}
