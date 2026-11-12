import {
  AppShell,
  Center,
  Drawer,
  Loader,
  useMantineColorScheme,
} from "@mantine/core";
import { useDisclosure, useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import {
  Spotlight,
  spotlight,
  type SpotlightActionData,
} from "@mantine/spotlight";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  IconInbox,
  IconMessage,
  IconMoon,
  IconSettings,
  IconSun,
} from "@tabler/icons-react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  forkThread,
  getThread,
  listAllThreads,
  listInbox,
  listProjects,
  markInboxRead,
  updateThread,
  type InboxItem,
  type ThreadSummary,
} from "./api";
import { FilesPanel } from "./components/FilesPanel";
import { InboxView } from "./components/InboxView";
import { RealChangesPanel } from "./components/RealChangesPanel";
import { RealThreadView } from "./components/RealThreadView";
import { ReviewDialog } from "./components/ReviewDialog";
import { SettingsView } from "./components/SettingsView";
import { ShareDialog } from "./components/ShareDialog";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TitleBar } from "./components/TitleBar";
import {
  MergeThreadDialog,
  RenameThreadDialog,
  RevertThreadDialog,
} from "./components/ThreadDialogs";
import { WorkspaceEmpty } from "./components/WorkspaceEmpty";
import { isKnownPath, navigate, pathFor, useRoute } from "./lib/router";

type View = "inbox" | "thread" | "settings" | "project";

export function App() {
  const route = useRoute();
  // The URL owns navigation; everything below derives from it so reloads,
  // deep links and the back button behave the same as in-app clicks.
  const view: View =
    route.kind === "settings"
      ? "settings"
      : route.kind === "project"
        ? "project"
        : route.kind === "thread"
          ? "thread"
          : "inbox";
  const routeThreadId = route.kind === "thread" ? route.threadId : null;
  const [selectedThread, setSelectedThread] = useState<ThreadSummary | null>(
    null,
  );
  const selectedThreadId = useRef<string | null>(null);
  selectedThreadId.current = selectedThread?.id ?? null;
  const [navOpened, nav] = useDisclosure(false);
  const [changesOpened, changes] = useDisclosure(false);
  const [mergeOpened, merge] = useDisclosure(false);
  const [shareOpened, share] = useDisclosure(false);
  const [renameOpened, rename] = useDisclosure(false);
  const [revertOpened, revert] = useDisclosure(false);
  const [reviewOpened, review] = useDisclosure(false);
  const [asideTab, setAsideTab] = useState<"changes" | "files">("changes");
  const [onlineUserIds, setOnlineUserIds] = useState<string[]>([]);
  const queryClient = useQueryClient();
  const wide = useMediaQuery("(min-width: 1200px)");
  const phone = useMediaQuery("(max-width: 48em)");
  const { colorScheme, toggleColorScheme } = useMantineColorScheme();
  const dark = colorScheme === "dark";

  const allThreads = useQuery({
    queryKey: ["threads-all"],
    queryFn: listAllThreads,
    staleTime: 10_000,
  });
  const projects = useQuery({
    queryKey: ["projects"],
    queryFn: listProjects,
    staleTime: 10_000,
  });
  const inboxUnread = useQuery({
    queryKey: ["inbox", "unread"],
    queryFn: () => listInbox("unread"),
    staleTime: 5_000,
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
  });

  // Give the first screen a real URL instead of leaving the bar at "/".
  useEffect(() => {
    if (window.location.pathname === "/" || !isKnownPath(window.location.pathname)) {
      navigate(pathFor({ kind: "inbox" }), { replace: true });
    }
  }, []);

  // A thread URL is authoritative: resolve it from the cached list, or fetch it
  // directly so a deep link works before any list has loaded.
  useEffect(() => {
    if (routeThreadId === null) {
      if (selectedThreadId.current !== null) setSelectedThread(null);
      return;
    }
    if (selectedThreadId.current === routeThreadId) return;
    const known = allThreads.data?.threads.find(
      (thread) => thread.id === routeThreadId,
    );
    if (known) {
      setSelectedThread(known);
      return;
    }
    let cancelled = false;
    void getThread(routeThreadId)
      .then(({ thread }) => {
        if (!cancelled) setSelectedThread(thread);
      })
      .catch(() => {
        if (!cancelled) setSelectedThread(null);
      });
    return () => {
      cancelled = true;
    };
  }, [routeThreadId, allThreads.data]);

  const showAside = view === "thread" && selectedThread !== null;
  const title =
    view === "inbox"
      ? "Inbox"
      : view === "settings"
        ? "Settings"
        : (selectedThread?.title ?? "Workspace");
  const participants =
    view === "thread" ? (selectedThread?.participants ?? []) : [];

  const openThread = (thread: ThreadSummary) => {
    setSelectedThread(thread);
    setAsideTab("changes");
    setOnlineUserIds([]);
    changes.close();
    navigate(pathFor({ kind: "thread", projectId: thread.projectId, threadId: thread.id }));
  };

  const openProject = (projectId: string) => {
    setSelectedThread(null);
    setOnlineUserIds([]);
    changes.close();
    navigate(pathFor({ kind: "project", projectId }));
  };

  const openInbox = () => {
    setSelectedThread(null);
    setOnlineUserIds([]);
    changes.close();
    navigate(pathFor({ kind: "inbox" }));
  };

  const openSettings = () => {
    setSelectedThread(null);
    changes.close();
    navigate(pathFor({ kind: "settings" }));
  };

  const openInboxItem = async (item: InboxItem) => {
    try {
      const [{ thread }] = await Promise.all([
        getThread(item.threadId),
        markInboxRead(item.threadId).catch(() => undefined),
      ]);
      void queryClient.invalidateQueries({ queryKey: ["inbox"] });
      openThread(thread);
    } catch (cause) {
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not open thread",
        color: "red",
      });
    }
  };

  const refreshThreadLists = (thread?: ThreadSummary) => {
    void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
    void queryClient.invalidateQueries({ queryKey: ["projects"] });
    if (thread) {
      void queryClient.invalidateQueries({ queryKey: ["threads", thread.projectId] });
      void queryClient.invalidateQueries({ queryKey: ["thread", thread.id] });
      void queryClient.invalidateQueries({ queryKey: ["thread-events", thread.id] });
    }
  };

  const patchSelected = async (
    patch: Parameters<typeof updateThread>[1],
  ) => {
    if (!selectedThread) return null;
    const { thread } = await updateThread(selectedThread.id, patch);
    setSelectedThread(thread);
    refreshThreadLists(thread);
    return thread;
  };

  const runAction = async (action: () => Promise<void>) => {
    try {
      await action();
    } catch (cause) {
      notifications.show({
        message: cause instanceof Error ? cause.message : "Request failed",
        color: "red",
      });
    }
  };

  const actions = useMemo<SpotlightActionData[]>(() => {
    const names = new Map(
      (projects.data?.projects ?? []).map((project) => [
        project.id,
        project.name,
      ]),
    );
    const threadActions = (allThreads.data?.threads ?? [])
      .slice(0, 20)
      .map((thread) => ({
        id: `thread:${thread.id}`,
        group: "Threads",
        label: thread.title,
        description: `${names.get(thread.projectId) ?? "Project"} · ${thread.agent} · ${thread.status.replaceAll("_", " ")}`,
        leftSection: <IconMessage size={15} stroke={1.5} />,
        onClick: () => {
          openThread(thread);
          spotlight.close();
        },
      }));

    return [
      {
        id: "inbox",
        group: "Navigate",
        label: "Go to Inbox",
        description: "Approvals, questions and mentions",
        leftSection: <IconInbox size={16} stroke={1.5} />,
        onClick: () => {
          openInbox();
          spotlight.close();
        },
      },
      {
        id: "settings",
        group: "Navigate",
        label: "Settings",
        description: "Workspace, agents and appearance",
        leftSection: <IconSettings size={16} stroke={1.5} />,
        onClick: () => {
          openSettings();
          spotlight.close();
        },
      },
      ...threadActions,
      {
        id: "theme",
        group: "Appearance",
        label: `Switch to ${dark ? "light" : "dark"} mode`,
        leftSection: dark ? (
          <IconSun size={16} stroke={1.5} />
        ) : (
          <IconMoon size={16} stroke={1.5} />
        ),
        onClick: () => {
          toggleColorScheme();
          spotlight.close();
        },
      },
    ];
  }, [allThreads.data, dark, projects.data, toggleColorScheme]);

  return (
    <>
      <AppShell
        mode="static"
        header={{ height: { base: 48, sm: 38 } }}
        navbar={{
          width: 260,
          breakpoint: "sm",
          collapsed: { mobile: !navOpened },
        }}
        aside={{
          width: 420,
          breakpoint: "lg",
          collapsed: { mobile: true, desktop: !showAside },
        }}
        footer={{ height: { base: 30, sm: 26 } }}
        padding={0}
        withBorder={false}
        className="shell"
      >
        <TitleBar
          title={title}
          participants={participants}
          onlineUserIds={onlineUserIds}
          unreadCount={inboxUnread.data?.unread ?? 0}
          canOpenChanges={showAside}
          canManage={Boolean(selectedThread) && route.kind === "thread"}
          isPinned={selectedThread?.pinned ?? false}
          navOpened={navOpened}
          onToggleNav={nav.toggle}
          onOpenPalette={() => spotlight.open()}
          onOpenInbox={openInbox}
          onOpenChanges={changes.open}
          onReviews={() => {
            if (!selectedThread) {
              notifications.show({
                message: "Open a thread before reviewing",
                color: "yellow",
              });
              return;
            }
            review.open();
          }}
          onShare={() => {
            if (!selectedThread) {
              notifications.show({
                message: "Open a thread before sharing",
                color: "yellow",
              });
              return;
            }
            share.open();
          }}
          onMerge={() => {
            if (!selectedThread) {
              notifications.show({
                message: "Open a thread before merging",
                color: "yellow",
              });
              return;
            }
            merge.open();
          }}
          onRename={() => {
            if (selectedThread) rename.open();
          }}
          onTogglePin={() => {
            if (!selectedThread) return;
            void runAction(async () => {
              await patchSelected({ pinned: !selectedThread.pinned });
            });
          }}
          onFork={() => {
            if (!selectedThread) return;
            void runAction(async () => {
              const { thread } = await forkThread(selectedThread.id);
              openThread(thread);
              refreshThreadLists(thread);
              notifications.show({ message: "Thread forked", color: "teal" });
            });
          }}
          onRevert={() => {
            if (selectedThread) revert.open();
          }}
          onArchive={() => {
            if (!selectedThread) return;
            void runAction(async () => {
              await updateThread(selectedThread.id, { archived: true });
              openInbox();
              refreshThreadLists(selectedThread);
              notifications.show({ message: "Thread archived", color: "teal" });
            });
          }}
        />
        <Sidebar
          onNavigate={nav.close}
          onOpenInbox={openInbox}
          onOpenSettings={openSettings}
          activeView={view}
          selectedThreadId={selectedThread?.id ?? null}
          activeProjectId={
            route.kind === "project" || route.kind === "thread"
              ? route.projectId
              : null
          }
          onSelectThread={openThread}
          onSelectProject={openProject}
        />
        <AppShell.Main className="main">
          {view === "inbox" ? (
            <InboxView onOpenThread={(item) => void openInboxItem(item)} />
          ) : view === "settings" ? (
            <SettingsView />
          ) : selectedThread ? (
            <RealThreadView
              key={selectedThread.id}
              threadId={selectedThread.id}
              onPresence={setOnlineUserIds}
            />
          ) : route.kind === "thread" ? (
            <Center h="60vh">
              <Loader size="sm" />
            </Center>
          ) : (
            <WorkspaceEmpty />
          )}
        </AppShell.Main>
        <AppShell.Aside p={0} className="aside">
          {selectedThread ? (
            asideTab === "changes" ? (
              <RealChangesPanel
                key={selectedThread.id}
                threadId={selectedThread.id}
                onShowFiles={() => setAsideTab("files")}
              />
            ) : (
              <FilesPanel
                key={selectedThread.id}
                threadId={selectedThread.id}
                onShowChanges={() => setAsideTab("changes")}
              />
            )
          ) : null}
        </AppShell.Aside>
        <StatusBar
          thread={selectedThread ?? undefined}
          workspace={view !== "thread" || selectedThread === null}
        />
      </AppShell>

      {!wide && showAside ? (
        <Drawer
          opened={changesOpened}
          onClose={changes.close}
          position="right"
          size={phone ? "100%" : "min(92vw, 520px)"}
          padding={0}
          withCloseButton={false}
          className="changes-drawer"
        >
          {selectedThread ? (
            asideTab === "changes" ? (
              <RealChangesPanel
                key={selectedThread.id}
                threadId={selectedThread.id}
                onClose={changes.close}
                onShowFiles={() => setAsideTab("files")}
              />
            ) : (
              <FilesPanel
                key={selectedThread.id}
                threadId={selectedThread.id}
                onClose={changes.close}
                onShowChanges={() => setAsideTab("changes")}
              />
            )
          ) : null}
        </Drawer>
      ) : null}

      <RenameThreadDialog
        opened={renameOpened}
        thread={selectedThread}
        onClose={rename.close}
        onRenamed={(thread) => {
          setSelectedThread(thread);
          refreshThreadLists(thread);
        }}
      />

      <RevertThreadDialog
        opened={revertOpened}
        thread={selectedThread}
        onClose={revert.close}
        onReverted={() => {
          if (selectedThread) refreshThreadLists(selectedThread);
        }}
      />

      <MergeThreadDialog
        opened={mergeOpened}
        thread={selectedThread}
        onClose={merge.close}
        onMerged={() => {
          if (selectedThread) refreshThreadLists(selectedThread);
        }}
      />

      <ReviewDialog
        threadId={selectedThread?.id ?? null}
        opened={reviewOpened}
        onClose={review.close}
        onOpenChanges={() => {
          review.close();
          setAsideTab("changes");
          changes.open();
        }}
      />

      <ShareDialog
        threadId={selectedThread?.id ?? null}
        opened={shareOpened}
        onClose={share.close}
      />

      <Spotlight
        actions={actions}
        shortcut={["mod + K"]}
        highlightQuery
        nothingFound="Nothing here."
        searchProps={{
          placeholder: "Search threads or run a command…",
        }}
        className="spotlight"
      />
    </>
  );
}
