import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ChevronLeft, Folder, FolderGit2, FolderPlus, LoaderCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { createProject } from "@/api/endpoints";
import { queryKeys, useDirectoryListing } from "@/api/queries";
import type { ProjectDto } from "@/api/types/ProjectDto";
import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  Input,
} from "@/components/ui";
import { describeError } from "@/lib/errors";
import { useDebouncedValue } from "@/lib/hooks";
import { basename } from "@/lib/format";

export interface NewProjectDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: (project: ProjectDto) => void;
}

export function NewProjectDialog({
  open,
  onOpenChange,
  onCreated,
}: NewProjectDialogProps) {
  const queryClient = useQueryClient();
  const [path, setPath] = useState("");
  const [active, setActive] = useState(0);
  const debounced = useDebouncedValue(path, 180);
  const listing = useDirectoryListing(debounced);
  const input = useRef<HTMLInputElement>(null);
  const initialized = useRef(false);
  const mutation = useMutation({
    mutationFn: () => createProject({ path: path.trim(), name: null }),
    onSuccess: (response) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.projects });
      initialized.current = false;
      onCreated(response.project);
      onOpenChange(false);
    },
  });

  useEffect(() => {
    if (!open) {
      initialized.current = false;
      return;
    }
    if (!initialized.current && listing.data) {
      initialized.current = true;
      setPath(listing.data.path);
    }
  }, [open, listing.data]);

  const entries = listing.data?.entries ?? [];
  const choices = [
    ...(listing.data?.parent
      ? [{ name: "..", path: listing.data.parent, isGit: false }]
      : []),
    ...entries,
  ];
  const isRepository = listing.data?.isGit ?? false;
  const selected = choices[active];

  const go = (next: string) => {
    setPath(next);
    setActive(0);
    window.setTimeout(() => input.current?.focus(), 0);
  };

  const submit = () => {
    if (path.trim()) {
      mutation.mutate();
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title="Add project"
        footer={
          <>
            <Button size="sm" variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button
              size="sm"
              variant="primary"
              loading={mutation.isPending}
              disabled={!path.trim()}
              onClick={submit}
            >
              Add {path.trim() ? basename(path) : "project"}
            </Button>
          </>
        }
      >
        <div className="space-y-2">
          <div className="flex items-center gap-1.5">
            {listing.data?.parent ? (
              <button
                type="button"
                className="flex size-8 shrink-0 items-center justify-center rounded-md text-faint hover:bg-hover hover:text-fg"
                aria-label="Parent directory"
                onClick={() => go(listing.data?.parent ?? "")}
              >
                <ChevronLeft className="size-3.5" aria-hidden />
              </button>
            ) : (
              <span className="size-8 shrink-0" />
            )}
            <Input
              ref={input}
              autoFocus
              value={path}
              onChange={(event) => {
                setPath(event.target.value);
                setActive(0);
              }}
              onKeyDown={(event) => {
                if (event.key === "ArrowDown") {
                  event.preventDefault();
                  setActive((value) => Math.min(value + 1, choices.length - 1));
                } else if (event.key === "ArrowUp") {
                  event.preventDefault();
                  setActive((value) => Math.max(value - 1, 0));
                } else if (event.key === "Enter") {
                  event.preventDefault();
                  if (selected) {
                    go(selected.path);
                  } else {
                    submit();
                  }
                }
              }}
              placeholder="/home/you/code"
              spellCheck={false}
              className="font-mono text-[12px]"
            />
          </div>

          {isRepository ? (
            <button
              type="button"
              onClick={submit}
              className="flex w-full items-center gap-2 rounded-md border border-accent/30 bg-accent/5 px-2.5 py-2 text-left"
            >
              <FolderGit2 className="size-4 shrink-0 text-accent" aria-hidden />
              <span className="min-w-0 flex-1 truncate text-[12px] text-fg">
                Use {basename(path)}
              </span>
              <Badge variant="accent" size="sm">
                git
              </Badge>
            </button>
          ) : null}

          {listing.isError ? (
            <p className="px-1 text-[10.5px] text-danger">
              {describeError(listing.error, "Cannot open directory").description}
            </p>
          ) : listing.isLoading ? (
            <div className="flex items-center justify-center py-8">
              <LoaderCircle className="size-4 animate-spin text-faint" aria-hidden />
            </div>
          ) : (
            <ul className="max-h-64 overflow-y-auto py-1">
              {choices.map((entry, index) => (
                <li key={`${entry.path}:${index}`}>
                  <button
                    type="button"
                    className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left ${
                      index === active ? "bg-hover" : ""
                    }`}
                    onMouseEnter={() => setActive(index)}
                    onClick={() => go(entry.path)}
                  >
                    {entry.isGit ? (
                      <FolderGit2 className="size-3.5 shrink-0 text-accent" aria-hidden />
                    ) : (
                      <Folder className="size-3.5 shrink-0 text-faint" aria-hidden />
                    )}
                    <span className="min-w-0 flex-1 truncate text-[12px] text-fg">
                      {entry.name}
                    </span>
                    {entry.isGit ? (
                      <Badge variant="outline" size="sm">
                        git
                      </Badge>
                    ) : null}
                  </button>
                </li>
              ))}
              {choices.length === 0 ? (
                <li className="flex items-center gap-2 px-2 py-2 text-[11px] text-faint">
                  <FolderPlus className="size-3.5" aria-hidden />
                  No subdirectories
                </li>
              ) : null}
            </ul>
          )}

          {mutation.isError ? (
            <p role="alert" className="px-1 text-[10.5px] text-danger">
              {describeError(mutation.error, "Cannot add project").description}
            </p>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  );
}
