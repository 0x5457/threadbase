import {
  AlertCircle,
  CircleDot,
  FileDiff,
  FolderGit2,
  Folder,
  GitBranch,
  Loader2,
} from "lucide-react";
import type { ProjectDto } from "@/api/types/ProjectDto";
import type { ThreadDto } from "@/api/types/ThreadDto";
import { Tooltip } from "@/components/ui";
import { basename } from "@/lib/format";
import { cn } from "@/lib/utils";

export interface StatusBarProps {
  project?: ProjectDto | null;
  thread?: ThreadDto | null;
  runningCount: number;
  waitingCount: number;
  projectCount: number;
  threadCount: number;
  offline?: boolean;
  onOpenChanges: () => void;
}

export function StatusBar({
  project,
  thread,
  runningCount,
  waitingCount,
  projectCount,
  threadCount,
  offline = false,
  onOpenChanges,
}: StatusBarProps) {
  const workIn = thread?.worktreePath
    ? basename(thread.worktreePath)
    : thread
      ? "Isolated worktree"
      : null;

  return (
    <footer className="z-20 flex h-8 shrink-0 items-center gap-3 border-t border-border/50 bg-background px-3 text-[11px] text-muted-foreground">
      <div className="flex min-w-0 items-center gap-2.5">
        <Tooltip content={project?.path ?? "No project selected"}>
          <span className="flex min-w-0 items-center gap-1">
            <Folder className="size-3 shrink-0 text-faint" aria-hidden />
            <span className="max-w-[160px] truncate">
              {project?.name ?? "Threadbase"}
            </span>
          </span>
        </Tooltip>
        {thread ? (
          <>
            <span className="h-3 w-px bg-border" aria-hidden />
            <span className="flex items-center gap-1">
              <GitBranch className="size-3 shrink-0 text-faint" aria-hidden />
              <span className="max-w-[140px] truncate">{thread.branch}</span>
            </span>
            {workIn ? (
              <>
                <span className="hidden h-3 w-px bg-border sm:block" aria-hidden />
                <Tooltip
                  content={
                    thread.worktreePath
                      ? `Work In: ${thread.worktreePath}`
                      : "Work In: managed isolated worktree"
                  }
                >
                  <span className="hidden items-center gap-1 sm:flex">
                    <FolderGit2
                      className="size-3 shrink-0 text-faint"
                      aria-hidden
                    />
                    <span className="max-w-[140px] truncate">{workIn}</span>
                  </span>
                </Tooltip>
              </>
            ) : null}
          </>
        ) : (
          <span className="text-faint">
            {projectCount} {projectCount === 1 ? "project" : "projects"} ·{" "}
            {threadCount} {threadCount === 1 ? "thread" : "threads"}
          </span>
        )}
      </div>

      <div className="ml-auto flex shrink-0 items-center gap-3">
        {offline ? (
          <span className="flex items-center gap-1 text-warning">
            <AlertCircle className="size-3" aria-hidden />
            Reconnecting…
          </span>
        ) : null}
        {thread ? (
          <button
            type="button"
            onClick={onOpenChanges}
            className="flex items-center gap-1.5 rounded px-1 py-0.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            title="Open changes"
          >
            <FileDiff className="size-3 text-faint" aria-hidden />
            <span className="text-success">
              +{thread.changes.additions}
            </span>
            <span className="text-danger">−{thread.changes.deletions}</span>
            {thread.changes.seen > 0 && thread.changes.total > 0 ? (
              <span className="text-faint">
                {thread.changes.seen}/{thread.changes.total} seen
              </span>
            ) : null}
          </button>
        ) : null}
        <span className="flex items-center gap-1" title="Running threads">
          {runningCount > 0 ? (
            <Loader2 className="size-3 animate-spin text-info" aria-hidden />
          ) : (
            <CircleDot className="size-3 text-faint" aria-hidden />
          )}
          <span className={cn(runningCount > 0 && "text-fg")}>
            {runningCount} running
          </span>
        </span>
        <span className="flex items-center gap-1" title="Waiting for approval">
          <span
            className={cn(
              "size-1.5 rounded-full",
              waitingCount > 0 ? "bg-warning" : "bg-border-strong",
            )}
            aria-hidden
          />
          <span className={cn(waitingCount > 0 && "text-fg")}>
            {waitingCount} waiting
          </span>
        </span>
      </div>
    </footer>
  );
}
