import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { Outlet, useLocation, useNavigate, useParams } from "react-router-dom";
import { toast } from "sonner";
import {
  createThread,
  decideApproval,
  logout as logoutRequest,
  updateProject,
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
import { Button, Dialog, DialogContent, Input } from "@/components/ui";
import { NewProjectDialog } from "@/components/workspace/NewProjectDialog";
import { ShareDialog } from "@/components/workspace/ShareDialog";
import { useOnlineStatus } from "@/lib/hooks";
import { browserNotify } from "@/lib/live";
import { usePresence, usePresenceStore } from "@/lib/presence";
import type {
  NewThreadOptions,
  WorkspaceOutletContext,
} from "./workspace-context";

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
  const localFocus = usePresenceStore((state) => state.local);
  const hiddenPresence = me.data?.user.hiddenPresence === true;
  const presenceProjectId =
    params.projectId ?? bundle.data?.bundle.thread.projectId;
  usePresence(
    presenceProjectId,
    params.threadId,
    hiddenPresence,
    localFocus,
  );
  const [newProjectOpen, setNewProjectOpen] = useState(false);
  const [locateProjectId, setLocateProjectId] = useState<string | null>(null);
  const [locatePath, setLocatePath] = useState("");
  const [shareOpen, setShareOpen] = useState(false);
  const [subthreadPickerOpen, setSubthreadPickerOpen] = useState(false);
  const [subthreadFilter, setSubthreadFilter] = useState("");
  const [followUserId, setFollowUserId] = useState<string | null>(null);
  const [followUserName, setFollowUserName] = useState<string | null>(null);

  const projectList = projects.data?.projects ?? [];
  const threadList = useMemo(
    () => threads.data?.threads ?? [],
    [threads.data?.threads],
  );
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
      note?: string;
    }) => {
      if (!params.threadId) {
        throw new Error("No active thread");
      }
      return decideApproval(
        params.threadId,
        input.approvalId,
        input.decision,
        input.note ?? null,
      );
    },
    onSuccess: () => {
      if (params.threadId) {
        void queryClient.invalidateQueries({
          queryKey: queryKeys.threadBundle(params.threadId),
        });
      }
      void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
      toast.success("Approval updated");
    },
    onError: (error) => {
      toast.error(
        error instanceof Error ? error.message : "Could not update approval.",
      );
    },
  });

  const decide = (
    approval: ApprovalDto,
    decision: ApprovalDecision,
    note?: string,
  ) => {
    decisionMutation.mutate({ approvalId: approval.id, decision, note });
  };

  const share = () => {
    if (activeThread) {
      setShareOpen(true);
    }
  };

  const locateMutation = useMutation({
    mutationFn: () =>
      updateProject(locateProjectId ?? "", {
        name: null,
        remoteUrl: null,
        path: locatePath.trim() || null,
      }),
    onSuccess: () => {
      setLocateProjectId(null);
      setLocatePath("");
      void queryClient.invalidateQueries({ queryKey: queryKeys.projects });
      void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
    },
    onError: (error) => toast.error(String(error)),
  });

  const createThreadMutation = useMutation({
    mutationFn: ({
      projectId,
      options,
    }: {
      projectId: string;
      options?: NewThreadOptions;
    }) => {
      const project = projectList.find((entry) => entry.id === projectId);
      return createThread(projectId, {
        title: null,
        agent:
          options?.agent ?? workspaceState.data?.settings.defaultAgent ?? "codex",
        model: options?.model ?? workspaceState.data?.settings.defaultModel ?? null,
        effort: null,
        baseBranch:
          options?.baseBranch ?? project?.defaultBranch ?? null,
        initialPrompt: null,
      });
    },
    onSuccess: (response) => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
      void queryClient.invalidateQueries({
        queryKey: queryKeys.projectThreads(response.thread.projectId, false),
      });
      navigate(
        `/p/${encodeURIComponent(response.thread.projectId)}/t/${encodeURIComponent(
          response.thread.id,
        )}`,
      );
    },
    onError: (error) => toast.error(String(error)),
  });

  const openNewThread = (projectId: string, options?: NewThreadOptions) => {
    if (!createThreadMutation.isPending) {
      createThreadMutation.mutate({ projectId, options });
    }
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
    if (!followUserId || presence.online.includes(followUserId)) {
      return;
    }
    // Give the member 30 seconds to reconnect before ending the follow.
    const timer = window.setTimeout(() => {
      setFollowUserId(null);
      setFollowUserName(null);
      toast.message("The followed member went offline");
    }, 30_000);
    return () => window.clearTimeout(timer);
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

  const seenInbox = useRef(new Set<string>());
  const inboxInitialized = useRef(false);
  useEffect(() => {
    const items = inbox.data?.items ?? [];
    if (!inboxInitialized.current) {
      for (const item of items) {
        seenInbox.current.add(item.id);
      }
      inboxInitialized.current = true;
      return;
    }
    const fresh = items.filter(
      (item) => !item.read && !seenInbox.current.has(item.id),
    );
    for (const item of items) {
      seenInbox.current.add(item.id);
    }
    if (fresh.length === 0) {
      return;
    }
    browserNotify(
      "Threadbase",
      fresh[0].summary,
      workspaceState.data?.settings.browserNotifications ?? false,
    );
  }, [inbox.data, workspaceState.data?.settings.browserNotifications]);

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
      if (modifier && !event.altKey && !event.shiftKey && key === "j") {
        event.preventDefault();
        setSubthreadPickerOpen(true);
        return;
      }
      if (event.ctrlKey && key === "tab" && !event.metaKey && !event.altKey) {
        event.preventDefault();
        const ordered = [...threadList].sort(
          (left, right) => Number(right.updatedAt) - Number(left.updatedAt),
        );
        if (ordered.length > 0) {
          const index = ordered.findIndex(
            (thread) => thread.id === activeThread?.id,
          );
          const direction = event.shiftKey ? -1 : 1;
          const next =
            ordered[(index + direction + ordered.length) % ordered.length];
          if (next) {
            navigate(
              `/p/${encodeURIComponent(next.projectId)}/t/${encodeURIComponent(
                next.id,
              )}`,
            );
          }
        }
        return;
      }
      if (modifier && event.altKey && !event.shiftKey && key === "o" && activeThread) {
        event.preventDefault();
        navigate(`/p/${encodeURIComponent(activeThread.projectId)}`);
        return;
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
  }, [
    activeThread,
    followUserId,
    me.data?.user.id,
    navigate,
    participants,
    presence.focuses,
    threadList,
  ]);

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
            onLocateProject={(projectId) => {
              setLocateProjectId(projectId);
              setLocatePath("");
            }}
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

      <Dialog
        open={Boolean(locateProjectId)}
        onOpenChange={(open) => {
          if (!open) {
            setLocateProjectId(null);
            setLocatePath("");
          }
        }}
      >
        <DialogContent
          title="Locate project"
          footer={
            <>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setLocateProjectId(null)}
              >
                Cancel
              </Button>
              <Button
                size="sm"
                variant="primary"
                loading={locateMutation.isPending}
                disabled={!locatePath.trim()}
                onClick={() => locateMutation.mutate()}
              >
                Locate
              </Button>
            </>
          }
        >
          <Input
            autoFocus
            aria-label="Project path"
            value={locatePath}
            onChange={(event) => setLocatePath(event.target.value)}
            placeholder="/path/to/repo"
          />
        </DialogContent>
      </Dialog>

      <Dialog
        open={subthreadPickerOpen}
        onOpenChange={setSubthreadPickerOpen}
      >
        <DialogContent
          title="View all subthreads"
          description="Every child session in this thread. Click one to open its transcript."
        >
          <Input
            value={subthreadFilter}
            onChange={(event) => setSubthreadFilter(event.target.value)}
            placeholder="Filter subthreads…"
            autoFocus
          />
          <ul className="mt-2 max-h-72 divide-y divide-border overflow-y-auto rounded-md border border-border">
            {(bundle.data?.bundle.subagents ?? [])
              .filter((subagent) =>
                subagent.title
                  .toLowerCase()
                  .includes(subthreadFilter.trim().toLowerCase()),
              )
              .map((subagent) => (
                <li key={subagent.id}>
                  <button
                    type="button"
                    className="flex w-full items-center gap-2 px-3 py-2 text-left text-[11.5px] text-fg hover:bg-hover"
                    onClick={() => {
                      setSubthreadPickerOpen(false);
                      navigate(
                        `/p/${encodeURIComponent(
                          bundle.data?.bundle.project.id ?? "",
                        )}/t/${encodeURIComponent(
                          bundle.data?.bundle.thread.id ?? "",
                        )}/s/${encodeURIComponent(subagent.id)}`,
                      );
                    }}
                  >
                    <span className="size-1.5 rounded-full bg-border-strong" />
                    <span className="min-w-0 flex-1 truncate">
                      {subagent.title}
                    </span>
                    <span className="text-[9.5px] text-faint">
                      {subagent.status}
                    </span>
                  </button>
                </li>
              ))}
            {(bundle.data?.bundle.subagents.length ?? 0) === 0 ? (
              <li className="px-3 py-4 text-[10.5px] text-faint">
                No subthreads in this thread.
              </li>
            ) : null}
          </ul>
          <div className="mt-2 flex justify-end">
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setSubthreadPickerOpen(false)}
            >
              Close
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {bundle.data ? (
        <ShareDialog
          open={shareOpen}
          onOpenChange={setShareOpen}
          bundle={bundle.data.bundle}
        />
      ) : null}


    </>
  );
}
