import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { createProject } from "@/api/endpoints";
import type { ProjectDto } from "@/api/types/ProjectDto";
import { Button, Dialog, DialogContent, Input } from "@/components/ui";
import { describeError } from "@/lib/errors";
import { basename } from "@/lib/format";
import { queryKeys } from "@/api/queries";

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
  const [name, setName] = useState("");
  const [validation, setValidation] = useState<string | null>(null);
  const mutation = useMutation({
    mutationFn: () =>
      createProject({
        path: path.trim(),
        name: name.trim() ? name.trim() : null,
      }),
    onSuccess: (response) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.projects });
      onCreated(response.project);
    },
  });

  const submit = () => {
    if (!path.trim()) {
      setValidation("Enter the absolute path of a local git repository.");
      return;
    }
    setValidation(null);
    mutation.mutate();
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title="Add project"
        description="Register a local repository. Threadbase validates the path and reads its git metadata."
        footer={
          <>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => onOpenChange(false)}
            >
              Cancel
            </Button>
            <Button
              size="sm"
              variant="primary"
              loading={mutation.isPending}
              onClick={submit}
            >
              Add project
            </Button>
          </>
        }
      >
        <form
          className="space-y-3"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              Repository path
            </span>
            <Input
              value={path}
              onChange={(event) => setPath(event.target.value)}
              placeholder="/home/you/code/project"
              autoFocus
              invalid={Boolean(validation)}
            />
          </label>
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              Display name <span className="text-faint">(optional)</span>
            </span>
            <Input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={path ? basename(path) : "Project name"}
            />
          </label>
          {validation ? (
            <p className="text-[11px] text-danger">{validation}</p>
          ) : null}
          {mutation.isError ? (
            <p role="alert" className="text-[11px] text-danger">
              {describeError(mutation.error, "Cannot add project").description}
            </p>
          ) : null}
        </form>
      </DialogContent>
    </Dialog>
  );
}
