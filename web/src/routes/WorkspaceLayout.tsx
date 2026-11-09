import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Outlet, useLocation, useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";
import {
  decideApproval,
  logout as logoutRequest,
} from "@/api/endpoints";
import {
  queryKeys,
  useInbox,
  useMe,
  useProjects,
  useThreadBundle,
  useWorkspaceThreads,
} from "@/api/queries";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { SubagentDto } from "@/api/types/SubagentDto";
import type { ThreadDto } from "@/api/types/ThreadDto";
import { AppShell } from "@/components/shell/AppShell";
import { SidebarTree } from "@/components/shell/SidebarTree";
import { NewProjectDialog } from "@/components/workspace/NewProjectDialog";
import { NewThreadDialog } from "@/components/workspace/NewThreadDialog";
import { useOnlineStatus } from "@/lib/hooks";
import type { WorkspaceOutletContext } from "./workspace-context";

function pageTitle(
  pathname: string,
  thread: ThreadDto | null,
  projectName: string | undefined,
  subagent: SubagentDto | undefined,
): { title: string; subtitle?: string } {
  if (pathname === "/inbox") {
    return { title: "Inbox" };
  }
  if (pathname === "/settings") {
    return { title: "Settings" };
  }
  if (thread) {
    const subtitleParts = [projectName].filter(Boolean);
    if (subagent) {
      subtitleParts.push(subagent.title);
    } else {
      subtitleParts.push(thread.branch);
    }
    return {
      title: thread.title,
      subtitle: subtitleParts.join(" / "),
    };
  }
  if (projectName) {
    return { title: projectName, subtitle: "Project" };
  }
  return { title: "Projects", subtitle: "Workspace" };
}

export function WorkspaceLayout() {
  const params = useParams();
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const online = useOnlineStatus();

  const me = useMe();
  const projects = useProjects();
  const threads = useWorkspaceThreads();
  const inbox = useInbox("unread");
  const bundle = useThreadBundle(params.threadId);
  const [newProjectOpen, setNewProjectOpen] = useState(false);
  const [newThreadProjectId, setNewThreadProjectId] = useState<string | null>(
    null,
  );

  const projectList = projects.data?.projects ?? [];
  const threadList = threads.data?.threads ?? [];
  const activeThread =
    bundle.data?.bundle.thread ??
    threadList.find((thread) => thread.id === params.threadId) ??
    null;
  const activeProject =
    bundle.data?.bundle.project ??
    projectList.find((project) => project.id === params.projectId) ??
    (activeThread
      ? projectList.find((project) => project.id === activeThread.projectId)
      : undefined) ??
    null;
  const activeSubagent = bundle.data?.bundle.subagents.find(
    (subagent) => subagent.id === params.subagentId,
  );

  const runningCount = threadList.filter(
    (thread) => thread.status === "running" || thread.status === "preparing",
  ).length;
  const waitingCount = threadList.reduce(
    (total, thread) => total + thread.pendingApprovals,
    0,
  );
  const activeThreadPending = bundle.data
    ? bundle.data.bundle.approvals.filter(
        (approval) => approval.status === "pending",
      ).length
    : (activeThread?.pendingApprovals ?? 0);

  const title = pageTitle(
    location.pathname,
    activeThread,
    activeProject?.name,
    activeSubagent,
  );

  const logoutMutation = useMutation({
    mutationFn: logoutRequest,
    onSuccess: () => {
      queryClient.clear();
      navigate("/login", { replace: true });
    },
    onError: (error) => {
      toast.error(
        error instanceof Error ? error.message : "Could not sign out.",
      );
    },
  });

  const decisionMutation = useMutation({
    mutationFn: (input: {
      approvalId: string;
      decision: ApprovalDecision;
    }) => {
      if (!params.threadId) {
        throw new Error("No active thread");
      }
      return decideApproval(
        params.threadId,
        input.approvalId,
        input.decision,
      );
    },
    onSuccess: () => {
      if (params.threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(params.threadId),
        });
      }
      void queryClient.invalidateQueries({ queryKey: queryKeys.threads });
      toast.success("Approval updated");
    },
    onError: (error) => {
      toast.error(
        error instanceof Error ? error.message : "Could not update approval.",
      );
    },
  });

  const decide = (approval: ApprovalDto, decision: ApprovalDecision) => {
    decisionMutation.mutate({ approvalId: approval.id, decision });
  };

  const newThreadProject = newThreadProjectId
    ? projectList.find((project) => project.id === newThreadProjectId)
    : undefined;

  const share = async () => {
    if (!activeThread) {
      return;
    }
    try {
      await navigator.clipboard.writeText(window.location.href);
      toast.success("Thread link copied");
    } catch {
      toast.error("Could not copy the link to the clipboard.");
    }
  };

  const openNewThread = (projectId: string) => {
    setNewThreadProjectId(projectId);
  };

  const outletContext: WorkspaceOutletContext = {
    openNewProject: () => setNewProjectOpen(true),
    openNewThread,
  };

  const sidebarError = projects.error ?? threads.error;

  return (
    <>
      <AppShell
        titleBar={{
          title: title.title,
          subtitle: title.subtitle,
          participants: activeThread?.participants ?? [],
          approvalsCount: activeThreadPending,
          shareDisabled: !activeThread,
          onShare: activeThread ? () => void share() : undefined,
        }}
        sidebar={(closeSidebar) => (
          <SidebarTree
            projects={projectList}
            threads={threadList}
            subagents={bundle.data?.bundle.subagents ?? []}
            selectedProjectId={params.projectId}
            selectedThreadId={params.threadId}
            selectedSubagentId={params.subagentId}
            loading={projects.isLoading || threads.isLoading}
            error={sidebarError}
            onRetry={() => {
              void projects.refetch();
              void threads.refetch();
            }}
            inboxCount={inbox.data?.unread ?? 0}
            user={me.data?.user}
            userLoading={me.isLoading}
            onSelectProject={(projectId) => navigate(`/p/${projectId}`)}
            onSelectThread={(thread) =>
              navigate(
                `/p/${encodeURIComponent(thread.projectId)}/t/${encodeURIComponent(
                  thread.id,
                )}`,
              )
            }
            onSelectSubagent={(subagent) => {
              const projectId = params.projectId ?? activeThread?.projectId;
              if (!projectId) {
                return;
              }
              navigate(
                `/p/${encodeURIComponent(
                  projectId,
                )}/t/${encodeURIComponent(subagent.threadId)}/s/${encodeURIComponent(
                  subagent.id,
                )}`,
              );
            }}
            onNewProject={() => setNewProjectOpen(true)}
            onNewThread={openNewThread}
            onOpenInbox={() => navigate("/inbox")}
            onOpenSettings={() => navigate("/settings")}
            onLogout={() => logoutMutation.mutate()}
            onNavigate={closeSidebar}
          />
        )}
        statusBar={{
          project: activeProject,
          thread: activeThread,
          runningCount,
          waitingCount,
          projectCount: projectList.length,
          threadCount: threadList.length,
          offline: !online,
        }}
        drawer={{
          bundle: bundle.data?.bundle,
          loading: bundle.isLoading,
          error: bundle.error,
          onRetry: () => void bundle.refetch(),
          decidingApprovalId: decisionMutation.isPending
            ? (decisionMutation.variables?.approvalId ?? null)
            : null,
          onDecideApproval: decide,
        }}
      >
        <Outlet context={outletContext} />
      </AppShell>

      {newProjectOpen ? (
        <NewProjectDialog
          open={newProjectOpen}
          onOpenChange={setNewProjectOpen}
          onCreated={(project) => {
            setNewProjectOpen(false);
            void queryClient.invalidateQueries({
              queryKey: queryKeys.projects,
            });
            navigate(`/p/${project.id}`);
          }}
        />
      ) : null}

      {newThreadProject ? (
        <NewThreadDialog
          open
          project={newThreadProject}
          onOpenChange={(open) => {
            if (!open) {
              setNewThreadProjectId(null);
            }
          }}
          onCreated={(thread) => {
            setNewThreadProjectId(null);
            navigate(
              `/p/${encodeURIComponent(thread.projectId)}/t/${encodeURIComponent(
                thread.id,
              )}`,
            );
          }}
        />
      ) : null}
    </>
  );
}
