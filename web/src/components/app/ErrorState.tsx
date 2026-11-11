import { AlertTriangle, RotateCw } from "lucide-react";
import type { ReactNode } from "react";
import { Button, Stack, Text, ThemeIcon } from "@mantine/core";
import { describeError } from "@/lib/errors";

export interface ErrorStateProps {
  error?: unknown;
  title?: string;
  description?: string;
  onRetry?: () => void;
  retryLabel?: string;
  action?: ReactNode;
  className?: string;
  compact?: boolean;
}

export function ErrorState({
  error,
  title,
  description,
  onRetry,
  retryLabel = "Retry",
  action,
  className,
  compact = false,
}: ErrorStateProps) {
  const described = error ? describeError(error, title) : null;
  const resolvedTitle = title ?? described?.title ?? "Something went wrong";
  const resolvedDescription = description ?? described?.description;

  return (
    <Stack
      role="alert"
      align="center"
      className={className}
      gap={compact ? 8 : 12}
      px={compact ? 16 : 24}
      py={compact ? 24 : 48}
    >
      <ThemeIcon
        variant="light"
        color="red"
        size={32}
        radius="md"
      >
        <AlertTriangle size={16} aria-hidden />
      </ThemeIcon>
      <Stack gap={4} align="center">
        <Text size="sm" fw={500} ta="center">
          {resolvedTitle}
        </Text>
        {resolvedDescription ? (
          <Text size="xs" c="dimmed" ta="center" maw={416}>
            {resolvedDescription}
          </Text>
        ) : null}
      </Stack>
      {onRetry ? (
        <Button
          size="xs"
          variant="default"
          leftSection={<RotateCw size={14} aria-hidden />}
          onClick={onRetry}
        >
          {retryLabel}
        </Button>
      ) : null}
      {action ? <Stack align="center">{action}</Stack> : null}
    </Stack>
  );
}
