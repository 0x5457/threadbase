import { Command } from "cmdk";
import { CornerDownLeft, Search } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useWorkspaceSearch } from "@/api/queries";
import type { SearchResultDto } from "@/api/types/SearchResultDto";
import { Badge, ErrorState, Kbd, Skeleton } from "@/components/ui";
import { useDebouncedValue } from "@/lib/hooks";
import { ApiError } from "@/lib/api";

export interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function resultPath(result: SearchResultDto): string | null {
  if (result.projectId && result.threadId) {
    return `/p/${encodeURIComponent(result.projectId)}/t/${encodeURIComponent(
      result.threadId,
    )}`;
  }
  if (result.projectId) {
    return `/p/${encodeURIComponent(result.projectId)}`;
  }
  return null;
}

function resultIconLabel(kind: string): string {
  switch (kind) {
    case "project":
      return "Project";
    case "thread":
      return "Thread";
    case "file":
      return "File";
    case "comment":
      return "Comment";
    case "message":
      return "Message";
    default:
      return kind;
  }
}

export function CommandPalette({ open, onOpenChange }: CommandPaletteProps) {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const debouncedQuery = useDebouncedValue(query, 180);
  const search = useWorkspaceSearch(debouncedQuery);
  const trimmed = query.trim();
  const searching = trimmed.length > 0 && debouncedQuery !== query;

  const close = () => {
    setQuery("");
    onOpenChange(false);
  };

  const select = (result: SearchResultDto) => {
    const path = resultPath(result);
    if (!path) {
      return;
    }
    close();
    navigate(path);
  };

  return (
    <Command.Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          setQuery("");
        }
        onOpenChange(next);
      }}
      label="Search Threadbase"
      shouldFilter={false}
      loop={false}
      overlayClassName="fixed inset-0 z-[95] bg-black/55 backdrop-blur-[1px]"
      contentClassName="fixed left-1/2 top-[14%] z-[100] w-[calc(100vw-2rem)] max-w-xl -translate-x-1/2 overflow-hidden rounded-xl border border-border-strong bg-panel shadow-2xl"
    >
      <div className="flex items-center gap-2 border-b border-border px-3">
        <Search className="size-4 shrink-0 text-faint" aria-hidden />
        <Command.Input
          autoFocus
          value={query}
          onValueChange={setQuery}
          placeholder="Search projects, threads and files…"
          className="h-11 w-full bg-transparent text-[13px] text-fg outline-none placeholder:text-faint"
        />
        <Kbd>Esc</Kbd>
      </div>

      <Command.List className="max-h-[min(420px,60vh)] overflow-y-auto p-1.5">
        {searching || search.isLoading ? (
          <div className="space-y-1.5 p-1.5">
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-3/4" />
          </div>
        ) : search.isError ? (
          <ErrorState
            error={search.error}
            title="Search failed"
            description={
              search.error instanceof ApiError
                ? search.error.message
                : undefined
            }
            compact
          />
        ) : trimmed.length === 0 ? (
          <div className="px-3 py-8 text-center">
            <p className="text-[12px] text-muted">
              Search across projects, threads and files.
            </p>
            <p className="mt-1 text-[11px] text-faint">
              Results are requested from the workspace search API.
            </p>
          </div>
        ) : search.data && search.data.results.length === 0 ? (
          <Command.Empty className="px-3 py-8 text-center text-[12px] text-muted">
            No results for “{trimmed}”.
          </Command.Empty>
        ) : (
          <Command.Group
            heading="Results"
            className="[&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1 [&_[cmdk-group-heading]]:text-[10px] [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-wider [&_[cmdk-group-heading]]:text-faint"
          >
            {search.data?.results.map((result) => {
              const path = resultPath(result);
              return (
                <Command.Item
                  key={`${result.kind}:${result.id}`}
                  value={`${result.kind}:${result.id}`}
                  disabled={!path}
                  onSelect={() => select(result)}
                  className="flex cursor-default items-center gap-2.5 rounded-md px-2 py-2 text-[12px] text-fg data-[disabled=true]:opacity-50 data-[selected=true]:bg-active"
                >
                  <Badge variant="outline" size="sm">
                    {resultIconLabel(result.kind)}
                  </Badge>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate">{result.title}</span>
                    {result.subtitle ? (
                      <span className="block truncate text-[10.5px] text-faint">
                        {result.subtitle}
                      </span>
                    ) : null}
                  </span>
                  <CornerDownLeft className="size-3 text-faint" aria-hidden />
                </Command.Item>
              );
            })}
          </Command.Group>
        )}
      </Command.List>

      <div className="flex items-center gap-3 border-t border-border px-3 py-1.5 text-[10px] text-faint">
        <span className="flex items-center gap-1">
          <Kbd>↑</Kbd>
          <Kbd>↓</Kbd>
          navigate
        </span>
        <span className="flex items-center gap-1">
          <Kbd>↵</Kbd>
          open
        </span>
        <span className="ml-auto">Workspace search</span>
      </div>
    </Command.Dialog>
  );
}
