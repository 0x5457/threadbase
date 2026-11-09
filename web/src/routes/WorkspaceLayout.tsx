import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
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
  useWorkspaceState,
  useWorkspaceThreads,
} from "@/api/queries";
import type { ApprovalDecision } from "@/api/types/ApprovalDecision";
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { SubagentDto } from "@/api/types/SubagentDto";
import type { ThreadDto } from "@/api/types/ThreadDto";
import { AppShell } from "@/components/shell/AppShell";
import { SidebarTree } from "@/components/shell/SidebarTree";
import { NewProjectDialog } from "@/components/workspace/NewProjectDialog";
import { ShareDialog } from "@/components/workspace/ShareDialog";
import { NewThreadDialog } from "@/components/workspace/NewThreadDialog";
import { useOnlineStatus } from "@/lib/hooks";
import { usePresence, usePresenceStore } from "@/lib/presence";
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
  const workspaceState = useWorkspaceState();
  const bundle = useThreadBundle(params.threadId);
  const presence = usePresenceStore();
  const hiddenPresence = me.data?.user.hiddenPresence === true;
  const presenceProjectId =
    params.projectId ?? bundle.data?.bundle.thread.projectId;
  usePresence(presenceProjectId, params.threadId, hiddenPresence);
  const [newProjectOpen, setNewProjectOpen] = useState(false);
  const [newThreadProjectId, setNewThreadProjectId] = useState<string | null>(
    null,
  );
  const [shareOpen, setShareOpen] = useState(false);
  const [followUserId, setFollowUserId] = useState<string | null>(null);
  const [followUserName, setFollowUserName] = useState<string | null>(null);

  const projectList = projects.data?.projects ?? [];
  const threadList = threads.data?.threads ?? [];
  const activeThread =
    bundle.data?.bundle.thread ??
    threadList.find((thread) => thread.id === params.threadId) ??
    null;
  const onlineIds = new Set<string>(presence.online);
  const participants = (activeThread?.participants ?? []).map((participant) => ({
    ...participant,
    online: onlineIds.has(participant.user.id),
  }));
  const followedFocus = followUserId
    ? presence.focuses.find((focus) => focus.userId === followUserId)
    : undefined;
  const followedName = followUserId
    ? (followUserName ??
      participants.find((participant) => participant.user.id === followUserId)
        ?.user.displayName ??
      "member")
    : null;
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

  const share = () => {
    if (activeThread) {
      setShareOpen(true);
    }
  };

  const openNewThread = (projectId: string) => {
    setNewThreadProjectId(projectId);
  };

  useEffect(() => {
    if (!followedFocus?.threadId || !followedFocus.projectId) {
      return;
    }
    if (followedFocus.threadId !== params.threadId) {
      navigate(
        `/p/${encodeURIComponent(followedFocus.projectId)}/t/${encodeURIComponent(
          followedFocus.threadId,
        )}`,
      );
    }
  }, [followedFocus?.threadId, followedFocus?.projectId, navigate, params.threadId]);

  useEffect(() => {
    if (
      followUserId &&
      presence.online.length > 0 &&
      !presence.online.includes(followUserId)
    ) {
      const timer = window.setTimeout(() => {
        setFollowUserId(null);
        setFollowUserName(null);
        toast.message("The followed member went offline");
      }, 0);
      return () => window.clearTimeout(timer);
    }
  }, [followUserId, presence.online]);

  useEffect(() => {
    if (!followUserId) {
      return;
    }
    const stop = () => {
      setFollowUserId(null);
      setFollowUserName(null);
    };
    const timer = window.setTimeout(() => {
      window.addEventListener("pointerdown", stop, { once: true });
    }, 0);
    window.addEventListener("wheel", stop, { passive: true, once: true });
    window.addEventListener("keydown", stop, { once: true });
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("pointerdown", stop);
      window.removeEventListener("wheel", stop);
      window.removeEventListener("keydown", stop);
    };
  }, [followUserId]);

  useEffect(() => {
    if (
      workspaceState.data?.settings.browserNotifications &&
      typeof Notification !== "undefined" &&
      Notification.permission === "default"
    ) {
      void Notification.requestPermission();
    }
  }, [workspaceState.data?.settings.browserNotifications]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();
      if (modifier && event.shiftKey && !event.altKey && key === "s" && activeThread) {
        event.preventDefault();
        setShareOpen(true);
      }
      if (modifier && event.altKey && !event.shiftKey && key === "l") {
        event.preventDefault();
        const selection = window.getSelection()?.toString().trim();
        const url = new URL(window.location.href);
        if (selection) {
          url.searchParams.set("selection", selection.slice(0, 500));
        }
        void navigator.clipboard.writeText(url.toString());
        toast.success(selection ? "Link to selection copied" : "Link copied");
      }
      if (modifier && event.altKey && !event.shiftKey && key === "f") {
        event.preventDefault();
        const candidates = presence.focuses
          .map((focus) => focus.userId)
          .filter((id) => id !== me.data?.user.id);
        if (candidates.length > 0) {
          const currentIndex = followUserId
            ? candidates.indexOf(followUserId)
            : -1;
          const next = candidates[(currentIndex + 1) % candidates.length];
          const person = participants.find(
            (participant) => participant.user.id === next,
          );
          setFollowUserId(next);
          setFollowUserName(person?.user.displayName ?? "member");
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [activeThread, followUserId, me.data?.user.id, participants, presence.focuses]);

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
          participants,
          followingName: followedName,
          onFollow: (userId) => {
            if (followUserId === userId) {
              setFollowUserId(null);
              setFollowUserName(null);
              return;
            }
            const person = participants.find(
              (participant) => participant.user.id === userId,
            );
            setFollowUserId(userId);
            setFollowUserName(person?.user.displayName ?? "member");
          },
          onStopFollowing: () => {
            setFollowUserId(null);
            setFollowUserName(null);
          },
          approvalsCount: activeThreadPending,
          shareDisabled: !activeThread,
          onShare: activeThread ? share : undefined,
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
          threadId: activeThread?.id,
          canWrite:
            me.data?.user.isAdmin === true ||
            bundle.data?.bundle.members.some(
              (member) =>
                member.user.id === me.data?.user.id &&
                (member.role === "owner" || member.role === "editor"),
            ) === true,
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

      {bundle.data ? (
        <ShareDialog
          open={shareOpen}
          onOpenChange={setShareOpen}
          bundle={bundle.data.bundle}
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
