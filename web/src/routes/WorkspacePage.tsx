import { useOutletContext } from "react-router-dom";
import { Link } from "react-router-dom";
import {
  AlertTriangle,
  ArrowUpRight,
  FolderGit2,
  FolderPlus,
  GitBranch,
  MessageSquare,
  ShieldCheck,
} from "lucide-react";
import { useProjects, useWorkspaceThreads } from "@/api/queries";
import type { ProjectDto } from "@/api/types/ProjectDto";
import {
  Badge,
  Button,
  EmptyState,
  ErrorState,
  Skeleton,
} from "@/components/ui";
import { formatRelativeTime } from "@/lib/format";
import type { WorkspaceOutletContext } from "./workspace-context";

function ProjectCard({ project }: { project: ProjectDto }) {
  return (
    <Link
      to={`/p/${encodeURIComponent(project.id)}`}
      className="group flex flex-col gap-3 rounded-xl border border-border bg-panel p-3.5 transition-colors hover:border-border-strong hover:bg-surface"
    >
      <div className="flex items-start gap-2.5">
        <span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg border border-border bg-raised text-muted">
          <FolderGit2 className="size-4" aria-hidden />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <h2 className="truncate text-[13px] font-semibold text-fg">
              {project.name}
            </h2>
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
        <ArrowUpRight className="size-3.5 shrink-0 text-faint transition-colors group-hover:text-fg" />
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        <Badge variant="outline" size="sm">
          <GitBranch className="size-2.5" aria-hidden />
          {project.defaultBranch}
        </Badge>
        <Badge variant="neutral" size="sm">
          <MessageSquare className="size-2.5" aria-hidden />
          {project.activeThreads} active
        </Badge>
        {project.waitingApprovals > 0 ? (
          <Badge variant="warning" size="sm">
            <ShieldCheck className="size-2.5" aria-hidden />
            {project.waitingApprovals} waiting
          </Badge>
        ) : null}
        {!project.available ? (
          <Badge variant="danger" size="sm">
            path unavailable
          </Badge>
        ) : null}
      </div>

      <div className="mt-auto flex items-center justify-between border-t border-border/70 pt-2 text-[10.5px] text-faint">
        <span className="truncate">
          {project.remoteUrl ?? "No remote configured"}
        </span>
        <span className="shrink-0">
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
  const threadList = threads.data?.threads ?? [];
  const running = threadList.filter(
    (thread) => thread.status === "running" || thread.status === "preparing",
  ).length;
  const waiting = threadList.reduce(
    (total, thread) => total + thread.pendingApprovals,
    0,
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        <div className="mx-auto w-full max-w-5xl px-6 py-7">
          <div className="flex flex-wrap items-end justify-between gap-3">
            <div className="space-y-1">
              <h1 className="text-[20px] font-semibold tracking-[-0.02em] text-fg">
                Projects
              </h1>
              <p className="text-[12px] text-muted">
                Local repositories. Open one to see its threads.
              </p>
            </div>
            <Button variant="primary" size="md" onClick={openNewProject}>
              <FolderPlus aria-hidden />
              Add project
            </Button>
          </div>

          <div className="mt-5 grid grid-cols-3 gap-2">
            <SummaryTile label="Projects" value={projectList.length} />
            <SummaryTile label="Running" value={running} />
            <SummaryTile label="Waiting approvals" value={waiting} />
          </div>

          <div className="mt-6">
            {projects.isLoading ? (
              <div className="grid gap-2.5 sm:grid-cols-2">
                {Array.from({ length: 4 }).map((_, index) => (
                  <Skeleton key={index} className="h-32 w-full rounded-xl" />
                ))}
              </div>
            ) : projects.isError ? (
              <div className="rounded-xl border border-border bg-panel">
                <ErrorState
                  error={projects.error}
                  title="Cannot load projects"
                  onRetry={() => void projects.refetch()}
                />
              </div>
            ) : projectList.length === 0 ? (
              <div className="rounded-xl border border-dashed border-border bg-panel/60">
                <EmptyState
                  icon={FolderPlus}
                  title="No projects yet"
                  description="Add a local git repository to create the first thread."
                  action={
                    <Button variant="primary" size="sm" onClick={openNewProject}>
                      <FolderPlus aria-hidden />
                      Add project
                    </Button>
                  }
                />
              </div>
            ) : (
              <div className="grid gap-2.5 sm:grid-cols-2">
                {projectList.map((project) => (
                  <ProjectCard key={project.id} project={project} />
                ))}
              </div>
            )}
            {threads.isError ? (
              <p className="mt-3 text-[11px] text-warning">
                Thread counts are unavailable:{" "}
                {threads.error instanceof Error
                  ? threads.error.message
                  : "the workspace thread list failed to load."}
              </p>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}

function SummaryTile({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-lg border border-border bg-panel px-3 py-2">
      <p className="text-[10px] font-medium uppercase tracking-wider text-faint">
        {label}
      </p>
      <p className="mt-0.5 text-[18px] font-semibold tabular-nums text-fg">
        {value}
      </p>
    </div>
  );
}
