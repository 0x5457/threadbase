import { ActionIcon, Button, Text } from "@mantine/core";
import {
  IconAlertCircle,
  IconCheck,
  IconReload,
  IconX,
} from "@tabler/icons-react";
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { useElapsed } from "../../lib/use-elapsed";

type ProgressStatus = "running" | "done" | "failed";

interface ProgressJobInput {
  /** Stable id for jobs that should survive a re-render without duplicating. */
  key?: string;
  title: string;
  detail?: string;
  stage?: string;
  /** 0..1 when the work can be counted; omit for a stage-only job. */
  progress?: number;
  cancelLabel?: string;
  onCancel?: () => void;
  onOpen?: () => void;
  onRetry?: () => void;
}

interface ProgressJob
  extends Omit<ProgressJobInput, "key" | "progress"> {
  id: string;
  startedAt: number;
  status: ProgressStatus;
  progress: number | null;
  result: string | null;
  error: string | null;
}

interface ProgressApi {
  jobs: ProgressJob[];
  start: (input: ProgressJobInput) => string;
  update: (id: string, patch: Partial<ProgressJobInput>) => void;
  finish: (id: string, result?: string, onOpen?: () => void) => void;
  fail: (id: string, error: string) => void;
  dismiss: (id: string) => void;
}

const ProgressContext = createContext<ProgressApi | null>(null);

export function ProgressProvider({ children }: { children: ReactNode }) {
  const [jobs, setJobs] = useState<ProgressJob[]>([]);

  const start = useCallback((input: ProgressJobInput) => {
    const id =
      input.key ??
      (typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `job-${Date.now()}-${Math.random().toString(36).slice(2)}`);
    setJobs((current) => {
      const existing = current.find((job) => job.id === id);
      if (existing) {
        return current.map((job) =>
          job.id === id
            ? {
                ...job,
                ...input,
                id,
                startedAt: Date.now(),
                status: "running",
                progress: input.progress ?? null,
                result: null,
                error: null,
              }
            : job,
        );
      }
      return [
        ...current,
        {
          ...input,
          id,
          startedAt: Date.now(),
          status: "running",
          progress: input.progress ?? null,
          result: null,
          error: null,
        },
      ];
    });
    return id;
  }, []);

  const update = useCallback((id: string, patch: Partial<ProgressJobInput>) => {
    setJobs((current) =>
      current.map((job) =>
        job.id === id
          ? {
              ...job,
              ...patch,
              progress:
                patch.progress === undefined ? job.progress : patch.progress,
            }
          : job,
      ),
    );
  }, []);

  const finish = useCallback(
    (id: string, result?: string, onOpen?: () => void) => {
      setJobs((current) =>
        current.map((job) =>
          job.id === id
            ? {
                ...job,
                status: "done",
                progress: 1,
                result: result ?? job.result ?? "Done",
                error: null,
                stage: undefined,
                onOpen: onOpen ?? job.onOpen,
              }
            : job,
        ),
      );
    },
    [],
  );

  const fail = useCallback((id: string, error: string) => {
    setJobs((current) =>
      current.map((job) =>
        job.id === id
          ? {
              ...job,
              status: "failed",
              error,
              stage: undefined,
            }
          : job,
      ),
    );
  }, []);

  const dismiss = useCallback((id: string) => {
    setJobs((current) => current.filter((job) => job.id !== id));
  }, []);

  const value = useMemo<ProgressApi>(
    () => ({ jobs, start, update, finish, fail, dismiss }),
    [dismiss, fail, finish, jobs, start, update],
  );

  return (
    <ProgressContext.Provider value={value}>{children}</ProgressContext.Provider>
  );
}

export function useProgressJobs(): ProgressApi {
  const value = useContext(ProgressContext);
  if (!value) {
    throw new Error("useProgressJobs must be used inside ProgressProvider");
  }
  return value;
}

function formatElapsed(seconds: number) {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return `${minutes}m ${String(rest).padStart(2, "0")}s`;
}

function ProgressCard({
  job,
  onDismiss,
}: {
  job: ProgressJob;
  onDismiss: () => void;
}) {
  const elapsed = useElapsed(job.startedAt, job.status === "running");
  const done = job.status === "done";
  const failed = job.status === "failed";
  const running = job.status === "running";
  const pct =
    job.progress === null ? null : Math.round(Math.min(1, job.progress) * 100);
  const long = running && elapsed >= 10;
  const stage = job.stage ?? null;

  const open = () => {
    if (job.onOpen) job.onOpen();
  };

  return (
    <article
      className="progress-card"
      data-status={job.status}
      role="status"
      aria-live="polite"
    >
      <div className="progress-card-main">
        <div className="progress-card-tile" aria-hidden="true">
          {done ? (
            <IconCheck size={18} stroke={2.4} />
          ) : failed ? (
            <IconAlertCircle size={18} stroke={1.8} />
          ) : (
            <span className="progress-card-pulse" />
          )}
        </div>
        <div className="progress-card-copy">
          {job.onOpen ? (
            <button
              type="button"
              className="progress-card-title-button"
              onClick={open}
            >
              <Text size="sm" fw={500} className="progress-card-title">
                {job.title}
              </Text>
            </button>
          ) : (
            <Text size="sm" fw={500} className="progress-card-title">
              {job.title}
            </Text>
          )}
          <Text size="xs" c="dimmed" className="progress-card-detail">
            {job.result ??
              (failed ? job.error : job.detail ?? "Working…")}
            {running ? ` · ${formatElapsed(elapsed)}` : ""}
          </Text>
          {running && !failed && stage ? (
            <Text size="xs" c="dimmed" className="progress-card-stage">
              {stage}
            </Text>
          ) : null}
          {long ? (
            <Text size="xs" c="dimmed" className="progress-card-stage">
              Taking longer than usual; it is still running.
            </Text>
          ) : null}
          {failed && job.onRetry ? (
            <Button
              variant="default"
              size="compact-xs"
              leftSection={<IconReload size={11} stroke={1.5} />}
              mt={6}
              onClick={(event) => {
                event.stopPropagation();
                job.onRetry?.();
              }}
            >
              Retry
            </Button>
          ) : null}
        </div>
        <ActionIcon
          size={20}
          aria-label={
            running && job.onCancel
              ? (job.cancelLabel ?? "Cancel")
              : "Dismiss"
          }
          onClick={(event) => {
            event.stopPropagation();
            if (running && job.onCancel) job.onCancel();
            else onDismiss();
          }}
          className="progress-card-close"
        >
          <IconX size={12} stroke={1.8} />
        </ActionIcon>
      </div>
      {running ? (
        <div
          className={`progress-card-track${
            pct === null ? " is-indeterminate" : ""
          }`}
          aria-hidden="true"
        >
          <span
            className="progress-card-fill"
            style={pct === null ? undefined : { width: `${pct}%` }}
          />
        </div>
      ) : null}
    </article>
  );
}

export function ProgressHost() {
  const { jobs, dismiss } = useProgressJobs();
  if (jobs.length === 0) return null;
  return createPortal(
    <div className="progress-stack" aria-label="Background work">
      {jobs.map((job) => (
        <ProgressCard
          key={job.id}
          job={job}
          onDismiss={() => dismiss(job.id)}
        />
      ))}
    </div>,
    document.body,
  );
}
