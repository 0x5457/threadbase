import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { createThread } from "@/api/endpoints";
import { queryKeys } from "@/api/queries";
import type { ProjectDto } from "@/api/types/ProjectDto";
import type { ThreadDto } from "@/api/types/ThreadDto";
import {
  Button,
  Dialog,
  DialogContent,
  Input,
  Textarea,
} from "@/components/ui";
import { describeError } from "@/lib/errors";

export interface NewThreadDialogProps {
  open: boolean;
  project: ProjectDto;
  onOpenChange: (open: boolean) => void;
  onCreated: (thread: ThreadDto) => void;
}

export function NewThreadDialog({
  open,
  project,
  onOpenChange,
  onCreated,
}: NewThreadDialogProps) {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [prompt, setPrompt] = useState("");
  const [baseBranch, setBaseBranch] = useState(project.defaultBranch);
  const [validation, setValidation] = useState<string | null>(null);
  const mutation = useMutation({
    mutationFn: () =>
      createThread(project.id, {
        title: title.trim(),
        agent: null,
        model: null,
        effort: null,
        baseBranch: baseBranch.trim() || null,
        initialPrompt: prompt.trim() ? prompt.trim() : null,
      }),
    onSuccess: (response) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.threads });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectThreads(project.id),
      });
      onCreated(response.thread);
    },
  });

  const submit = () => {
    if (!title.trim()) {
      setValidation("Give the thread a title so it is identifiable in the tree.");
      return;
    }
    setValidation(null);
    mutation.mutate();
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={`New thread in ${project.name}`}
        description="The thread gets its own worktree and agent session. The first prompt is optional."
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
              Create thread
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
            <span className="text-[11px] font-medium text-muted">Title</span>
            <Input
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Fix the login redirect"
              autoFocus
              invalid={Boolean(validation)}
            />
          </label>
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              Base branch
            </span>
            <Input
              value={baseBranch}
              onChange={(event) => setBaseBranch(event.target.value)}
              placeholder={project.defaultBranch || "main"}
            />
          </label>
          <label className="block space-y-1.5">
            <span className="text-[11px] font-medium text-muted">
              First prompt <span className="text-faint">(optional)</span>
            </span>
            <Textarea
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              placeholder="Describe the task for the agent…"
            />
          </label>
          {validation ? (
            <p className="text-[11px] text-danger">{validation}</p>
          ) : null}
          {mutation.isError ? (
            <p role="alert" className="text-[11px] text-danger">
              {describeError(mutation.error, "Cannot create thread").description}
            </p>
          ) : null}
        </form>
      </DialogContent>
    </Dialog>
  );
}
