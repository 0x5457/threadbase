import { ChevronLeft, FileCode2, Folder, FolderOpen } from "lucide-react";
import { useState } from "react";
import { useFile, useFiles } from "@/api/queries";
import {
  EmptyState,
  ErrorState,
  Skeleton,
} from "@/components/ui";
import { cn } from "@/lib/utils";

export function FilesPanel({ threadId }: { threadId: string }) {
  const [path, setPath] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const files = useFiles(threadId, path);
  const file = useFile(threadId, selected);
  const segments = path.split("/").filter(Boolean);

  if (selected) {
    return (
      <div className="flex min-h-0 flex-col">
        <div className="flex h-8 shrink-0 items-center gap-1 border-b border-border px-2">
          <button
            type="button"
            className="flex size-6 items-center justify-center rounded text-faint hover:bg-hover hover:text-fg"
            onClick={() => setSelected(null)}
            aria-label="Back to file list"
          >
            <ChevronLeft className="size-3.5" aria-hidden />
          </button>
          <span className="truncate font-mono text-[10px] text-faint">
            {selected}
          </span>
        </div>
        {file.isLoading ? (
          <div className="p-3">
            <Skeleton className="h-40 w-full" />
          </div>
        ) : file.isError ? (
          <ErrorState
            error={file.error}
            title="Cannot open file"
            onRetry={() => void file.refetch()}
            compact
          />
        ) : (
          <div className="min-h-0 flex-1 overflow-auto p-2">
            <pre className="font-mono text-[10px] leading-relaxed text-fg">
              {file.data?.file.content}
            </pre>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-col">
      <div className="flex min-h-8 shrink-0 flex-wrap items-center gap-1 border-b border-border px-2 py-1">
        <button
          type="button"
          className="text-[10.5px] text-accent hover:underline"
          onClick={() => setPath("")}
        >
          worktree
        </button>
        {segments.map((segment, index) => {
          const next = segments.slice(0, index + 1).join("/");
          return (
            <span key={next} className="flex items-center gap-1">
              <span className="text-[10px] text-faint">/</span>
              <button
                type="button"
                className="text-[10.5px] text-muted hover:text-fg"
                onClick={() => setPath(next)}
              >
                {segment}
              </button>
            </span>
          );
        })}
      </div>
      {files.isLoading ? (
        <div className="space-y-1.5 p-2">
          {Array.from({ length: 6 }).map((_, index) => (
            <Skeleton key={index} className="h-6 w-full" />
          ))}
        </div>
      ) : files.isError ? (
        <ErrorState
          error={files.error}
          title="Cannot list files"
          onRetry={() => void files.refetch()}
          compact
        />
      ) : (files.data?.entries.length ?? 0) === 0 ? (
        <EmptyState
          icon={FolderOpen}
          title="Empty directory"
          description="This directory has no visible files."
          compact
        />
      ) : (
        <ul className="min-h-0 flex-1 overflow-y-auto py-1">
          {files.data?.entries.map((entry) => (
            <li key={entry.path}>
              <button
                type="button"
                className={cn(
                  "flex w-full items-center gap-2 px-2.5 py-1 text-left hover:bg-hover",
                )}
                onClick={() => {
                  if (entry.isDir) {
                    setPath(entry.path);
                  } else {
                    setSelected(entry.path);
                  }
                }}
              >
                {entry.isDir ? (
                  <Folder className="size-3.5 shrink-0 text-info" aria-hidden />
                ) : (
                  <FileCode2 className="size-3.5 shrink-0 text-faint" aria-hidden />
                )}
                <span className="min-w-0 flex-1 truncate font-mono text-[10.5px] text-fg">
                  {entry.name}
                </span>
                {!entry.isDir ? (
                  <span className="shrink-0 text-[9.5px] text-faint">
                    {entry.size} B
                  </span>
                ) : null}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
