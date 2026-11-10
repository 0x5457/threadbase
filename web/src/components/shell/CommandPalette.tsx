import { CornerDownLeft } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useWorkspaceSearch } from "@/api/queries";
import type { SearchResultDto } from "@/api/types/SearchResultDto";
import {
  Badge,
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  ErrorState,
  Kbd,
  Skeleton,
} from "@/components/ui";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
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
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          setQuery("");
        }
        onOpenChange(next);
      }}
    >
      <DialogContent
        className="top-[14%] max-w-xl translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-xl"
        showCloseButton={false}
      >
        <DialogTitle className="sr-only">Search Threadbase</DialogTitle>
        <DialogDescription className="sr-only">
          Search across projects, threads and files
        </DialogDescription>
        <Command shouldFilter={false} loop={false} className="p-0">
          <CommandInput
            autoFocus
            value={query}
            onValueChange={setQuery}
            placeholder="Search projects, threads and files…"
            className="h-11 text-[13px]"
          />
          <CommandList className="max-h-[min(420px,60vh)] p-1.5">
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
                <p className="text-[12px] text-muted-foreground">
                  Search across projects, threads and files.
                </p>
                <p className="mt-1 text-[11px] text-faint">
                  Results are requested from the workspace search API.
                </p>
              </div>
            ) : search.data && search.data.results.length === 0 ? (
              <CommandEmpty className="px-3 py-8 text-center text-[12px] text-muted-foreground">
                No results for “{trimmed}”.
              </CommandEmpty>
            ) : (
              <CommandGroup heading="Results">
                {search.data?.results.map((result) => {
                  const path = resultPath(result);
                  return (
                    <CommandItem
                      key={`${result.kind}:${result.id}`}
                      value={`${result.kind}:${result.id}`}
                      disabled={!path}
                      onSelect={() => select(result)}
                      className="gap-2.5 px-2 py-2 text-[13px]"
                    >
                      <Badge variant="outline" size="sm">
                        {resultIconLabel(result.kind)}
                      </Badge>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate">{result.title}</span>
                        {result.subtitle ? (
                          <span className="block truncate text-[11px] text-muted-foreground">
                            {result.subtitle}
                          </span>
                        ) : null}
                      </span>
                      <CornerDownLeft className="size-3 text-faint" aria-hidden />
                    </CommandItem>
                  );
                })}
              </CommandGroup>
            )}
          </CommandList>
          <div className="flex items-center gap-3 border-t border-border/60 px-3 py-1.5 text-[11px] text-faint">
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
        </Command>
      </DialogContent>
    </Dialog>
  );
}
