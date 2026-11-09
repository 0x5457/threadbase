import { AlertTriangle, FolderGit2, FolderPlus } from "lucide-react";
import { Link, useOutletContext } from "react-router-dom";
import { useProjects, useWorkspaceThreads } from "@/api/queries";
import type { ProjectDto } from "@/api/types/ProjectDto";
import { Button, EmptyState, ErrorState, Skeleton } from "@/components/ui";
import { formatRelativeTime } from "@/lib/format";
import type { WorkspaceOutletContext } from "./workspace-context";

function ProjectRow({ project }: { project: ProjectDto }) {
  return (
    <Link
      to={`/p/${encodeURIComponent(project.id)}`}
      className="group flex items-center gap-3 px-3 py-3 transition-colors hover:bg-hover"
    >
      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-raised text-muted">
        <FolderGit2 className="size-4" aria-hidden />
      </span>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <span className="truncate text-[13px] font-medium text-fg">
            {project.name}
          </span>
          {!project.available ? (
            <AlertTriangle
              className="size-3.5 shrink-0 text-warning"
              aria-label="Project path is unavailable"
            />
          ) : null}
        </div>
        <p className="truncate font-mono text-[10.5px] text-faint">
          {project.path}
        </p>
      </div>
      <div className="hidden shrink-0 items-center gap-4 text-[10.5px] text-faint sm:flex">
        <span>{project.activeThreads} threads</span>
        {project.waitingApprovals > 0 ? (
          <span className="text-warning">
            {project.waitingApprovals} waiting
          </span>
        ) : null}
        <span className="w-16 text-right">
          {formatRelativeTime(project.lastActivityAt)}
        </span>
      </div>
    </Link>
  );
}

export function WorkspacePage() {
  const { openNewProject } = useOutletContext<WorkspaceOutletContext>();
  const projects = useProjects();
  const threads = useWorkspaceThreads();
  const projectList = projects.data?.projects ?? [];

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <div className="mx-auto w-full max-w-4xl px-6 py-8">
          <div className="flex items-center justify-between">
            <h1 className="text-[18px] font-semibold tracking-[-0.02em] text-fg">
              Projects
            </h1>
            <Button variant="primary" size="sm" onClick={openNewProject}>
              <FolderPlus aria-hidden />
              Add project
            </Button>
          </div>

          <div className="mt-5">
            {projects.isLoading ? (
              <div className="space-y-1">
                {Array.from({ length: 4 }).map((_, index) => (
                  <Skeleton key={index} className="h-14 w-full" />
                ))}
              </div>
            ) : projects.isError ? (
              <ErrorState
                error={projects.error}
                title="Cannot load projects"
                onRetry={() => void projects.refetch()}
              />
            ) : projectList.length === 0 ? (
              <EmptyState
                icon={FolderPlus}
                title="No projects"
                action={
                  <Button variant="primary" size="sm" onClick={openNewProject}>
                    <FolderPlus aria-hidden />
                    Add project
                  </Button>
                }
                compact
              />
            ) : (
              <div className="divide-y divide-border/60 border-y border-border/60">
                {projectList.map((project) => (
                  <ProjectRow key={project.id} project={project} />
                ))}
              </div>
            )}
            {threads.isError ? (
              <p className="mt-3 text-[10.5px] text-warning">
                {threads.error instanceof Error
                  ? threads.error.message
                  : "Thread counts unavailable."}
              </p>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
