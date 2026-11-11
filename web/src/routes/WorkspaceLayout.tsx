import {
  Button,
  Group,
  ScrollArea,
  Stack,
  Text,
  TextInput,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useState } from "react";
import { Outlet, useLocation, useNavigate, useParams } from "react-router-dom";
import {
  createThread,
  decideApproval,
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
import { ResponsiveModal } from "@/components/app";
import { AppShell } from "@/components/shell/AppShell";
import { SidebarTree } from "@/components/shell/SidebarTree";
import { SubagentStatusBadge } from "@/components/thread/SubagentStatusBadge";
import { NewProjectDialog } from "@/components/workspace/NewProjectDialog";
import { ShareDialog } from "@/components/workspace/ShareDialog";
import { useOnlineStatus } from "@/lib/hooks";
import {
  isInsideOverlay,
  isTypingTarget,
  modKeyPressed,
} from "@/lib/shortcuts";
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
  if (pathname.startsWith("/invite/")) {
    return { title: "Invite" };
  }
  if (pathname.endsWith("/compare")) {
    return { title: "Compare", subtitle: projectName };
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
  if (pathname !== "/") {
    return { title: "Not found" };
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

  // Signing out is a tailnet concern: the identity comes from `tailscale serve`
  // on every request, so this only clears local caches.
  const logoutMutation = useMutation({
    mutationFn: async () => undefined,
    onSuccess: () => {
      queryClient.clear();
      window.location.assign("/");
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
      notifications.show({
        message: "Approval updated",
        color: "green",
      });
    },
    onError: (error) => {
      notifications.show({
        message:
          error instanceof Error ? error.message : "Could not update approval.",
        color: "red",
      });
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
    onError: (error) =>
      notifications.show({
        message: String(error),
        color: "red",
      }),
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
    onError: (error) =>
      notifications.show({
        message: String(error),
        color: "red",
      }),
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
      notifications.show({
        message: "The followed member went offline",
      });
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
      if (
        event.defaultPrevented ||
        isTypingTarget(event.target) ||
        isInsideOverlay(event.target)
      ) {
        return;
      }
      const modifier = modKeyPressed(event);
      const key = event.key.toLowerCase();
      if (modifier && event.shiftKey && !event.altKey && key === "a") {
        event.preventDefault();
        setNewProjectOpen(true);
        return;
      }
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
        notifications.show({
          message: selection ? "Link to selection copied" : "Link copied",
          color: "green",
        });
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
        sidebar={(closeSidebar, { overlay }) => (
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
            onClose={overlay ? closeSidebar : undefined}
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

      <ResponsiveModal
        opened={Boolean(locateProjectId)}
        onClose={() => {
          setLocateProjectId(null);
          setLocatePath("");
        }}
        title="Locate project"
        size="sm"
      >
        <TextInput
          autoFocus
          aria-label="Project path"
          value={locatePath}
          onChange={(event) => setLocatePath(event.target.value)}
          placeholder="/path/to/repo"
        />
        <Group justify="flex-end" gap="xs" mt="sm">
          <Button
            variant="default"
            size="sm"
            className="tb-action-40"
            onClick={() => {
              setLocateProjectId(null);
              setLocatePath("");
            }}
          >
            Cancel
          </Button>
          <Button
            variant="filled"
            size="sm"
            className="tb-action-40"
            loading={locateMutation.isPending}
            disabled={!locatePath.trim()}
            onClick={() => locateMutation.mutate()}
          >
            Locate
          </Button>
        </Group>
      </ResponsiveModal>

      <ResponsiveModal
        opened={subthreadPickerOpen}
        onClose={() => setSubthreadPickerOpen(false)}
        title="View all subthreads"
        size="sm"
      >
        <Text size="sm" c="dimmed" mb="sm">
          Every child session in this thread. Click one to open its transcript.
        </Text>
        <TextInput
          value={subthreadFilter}
          onChange={(event) => setSubthreadFilter(event.target.value)}
          placeholder="Filter subthreads…"
          autoFocus
        />
        <ScrollArea h={{ base: "50dvh", sm: 288 }} mt="xs">
          <Stack gap={2}>
            {(bundle.data?.bundle.subagents ?? [])
              .filter((subagent) =>
                subagent.title
                  .toLowerCase()
                  .includes(subthreadFilter.trim().toLowerCase()),
              )
              .map((subagent) => (
                <Button
                  key={subagent.id}
                  variant="subtle"
                  color="gray"
                  justify="flex-start"
                  fullWidth
                  leftSection={
                    <span
                      aria-hidden
                      style={{
                        width: 6,
                        height: 6,
                        borderRadius: "50%",
                        background: "var(--mantine-color-gray-filled)",
                      }}
                    />
                  }
                  rightSection={
                    <SubagentStatusBadge status={subagent.status} />
                  }
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
                  <Text size="sm" truncate="end">
                    {subagent.title}
                  </Text>
                </Button>
              ))}
            {(bundle.data?.bundle.subagents.length ?? 0) === 0 ? (
              <Text size="sm" c="dimmed" px="xs" py="sm">
                No subthreads in this thread.
              </Text>
            ) : null}
          </Stack>
        </ScrollArea>
        <Group justify="flex-end" mt="xs">
          <Button
            variant="default"
            size="xs"
            className="tb-action-40"
            onClick={() => setSubthreadPickerOpen(false)}
          >
            Close
          </Button>
        </Group>
      </ResponsiveModal>

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
