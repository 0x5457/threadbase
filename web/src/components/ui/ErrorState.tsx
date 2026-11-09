import { AlertTriangle, RotateCw } from "lucide-react";
import { describeError } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { Button } from "./Button";

export interface ErrorStateProps {
  error?: unknown;
  title?: string;
  description?: string;
  onRetry?: () => void;
  retryLabel?: string;
  className?: string;
  compact?: boolean;
}

export function ErrorState({
  error,
  title,
  description,
  onRetry,
  retryLabel = "Retry",
  className,
  compact = false,
}: ErrorStateProps) {
  const described = error ? describeError(error, title) : null;
  const resolvedTitle = title ?? described?.title ?? "Something went wrong";
  const resolvedDescription = description ?? described?.description;

  return (
    <div
      role="alert"
      className={cn(
        "flex flex-col items-center justify-center text-center",
        compact ? "gap-2 px-4 py-6" : "gap-3 px-6 py-12",
        className,
      )}
    >
      <span
        className={cn(
          "flex items-center justify-center rounded-lg border border-danger/25 bg-danger/10 text-danger",
          compact ? "size-8" : "size-10",
        )}
      >
        <AlertTriangle className={compact ? "size-4" : "size-5"} aria-hidden />
      </span>
      <div className="space-y-1">
        <p className="text-[13px] font-medium text-fg">{resolvedTitle}</p>
        {resolvedDescription ? (
          <p className="max-w-md text-[12px] leading-relaxed text-muted">
            {resolvedDescription}
          </p>
        ) : null}
      </div>
      {onRetry ? (
        <Button size="sm" variant="secondary" onClick={onRetry}>
          <RotateCw aria-hidden />
          {retryLabel}
        </Button>
      ) : null}
    </div>
  );
}
