import {
  AppShell,
  Drawer,
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
  IconTerminal2,
} from "@tabler/icons-react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  BrowserRouter,
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from "react-router";
import {
  ApiError,
  forkThread,
  getMe,
  getState,
  getThread,
  listTerminalSessions,
  listAllThreads,
  listInbox,
  listProjects,
  markInboxRead,
  updateThread,
  type InboxItem,
  type ThreadSummary,
} from "./api";
import type { InboxPageDto } from "./api/types/InboxPageDto";
import { FilesPanel } from "./components/FilesPanel";
import { DrawerResizeHandle } from "./components/DrawerResizeHandle";
import { InboxView } from "./components/InboxView";
import { MobileTabBar } from "./components/MobileTabBar";
import { RealChangesPanel } from "./components/RealChangesPanel";
import { RealThreadView } from "./components/RealThreadView";
import { ReviewDialog } from "./components/ReviewDialog";
import { SettingsView } from "./components/SettingsView";
import { TerminalDrawer } from "./components/terminal/TerminalDrawer";
import { ShareDialog } from "./components/ShareDialog";
import { SheetGrabber } from "./components/SheetGrabber";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TitleBar } from "./components/TitleBar";
import {
  MergeThreadDialog,
  RenameThreadDialog,
  RevertThreadDialog,
} from "./components/ThreadDialogs";
import { EmptyState } from "./components/EmptyState";
import { WorkspaceEmpty } from "./components/WorkspaceEmpty";
import { useOverlayBack } from "./lib/overlay-back";
import { pathFor, useRoute } from "./lib/router";

type View = "inbox" | "thread" | "settings" | "project";

function blurActiveInput() {
  const active = document.activeElement;
  // The terminal's input is xterm's helper <textarea>. Blurring it would end
  // the session's keyboard, so scroll-driven blur must never reach it.
  if (active instanceof HTMLElement && active.closest(".terminal-drawer")) return;
  if (
    active instanceof HTMLElement &&
    (active.isContentEditable ||
      active.tagName === "INPUT" ||
      active.tagName === "TEXTAREA")
  ) {
    active.blur();
  }
}

/** Optimistic unread page after a thread's inbox items were marked read. */
function withoutThreadInboxItems(
  page: InboxPageDto,
  threadId: string,
): InboxPageDto {
  const removed = page.items.filter((entry) => entry.threadId === threadId);
  if (removed.length === 0) return page;
  return {
    ...page,
    items: page.items.filter((entry) => entry.threadId !== threadId),
    unread: Math.max(
      0,
      page.unread - removed.filter((entry) => !entry.read).length,
    ),
  };
}

function Workspace() {
  const route = useRoute();
  const location = useLocation();
  const navigate = useNavigate();
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
  const pendingInboxReads = useRef<Set<string>>(new Set());
  const [navOpened, nav] = useDisclosure(false);
  const [changesOpened, changes] = useDisclosure(false);
  const [mergeOpened, merge] = useDisclosure(false);
  const [shareOpened, share] = useDisclosure(false);
  const [renameOpened, rename] = useDisclosure(false);
  const [revertOpened, revert] = useDisclosure(false);
  const [reviewOpened, review] = useDisclosure(false);
  const [navCollapsed, setNavCollapsed] = useState(() => {
    try {
      return window.localStorage.getItem("threadbase.nav-collapsed") === "1";
    } catch {
      return false;
    }
  });
  const [asideTab, setAsideTab] = useState<"changes" | "files">("changes");
  // The terminal drawer is workspace state, not thread state: switching thread
  // remounts it against that thread's shell.
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [terminalFull, setTerminalFull] = useState(false);
  const terminalThreadId = view === "thread" ? (selectedThread?.id ?? null) : null;
  // The server decides whether the feature exists at all; only administrators
  // can drive a shell. Entry points stay hidden otherwise.
  const stateQuery = useQuery({
    queryKey: ["state"],
    queryFn: getState,
    staleTime: 60_000,
  });
  const me = useQuery({ queryKey: ["me"], queryFn: getMe, staleTime: 60_000 });
  const terminalEnabled = Boolean(stateQuery.data?.terminal) && Boolean(me.data?.user.isAdmin);
  // A shell that outlived the drawer: the status bar keeps it discoverable.
  const terminalSessions = useQuery({
    queryKey: ["terminal-sessions", terminalThreadId],
    queryFn: () => listTerminalSessions(terminalThreadId),
    enabled: terminalEnabled,
    staleTime: 5_000,
    refetchInterval: terminalOpen ? false : 10_000,
  });
  const liveTerminal =
    terminalSessions.data?.sessions.find(
      (session) =>
        session.alive && (session.threadId ?? null) === terminalThreadId,
    ) ?? null;
  const [changesWidth, setChangesWidth] = useState(() => {
    try {
      const stored = Number(
        window.localStorage.getItem("threadbase.changes-width"),
      );
      return Number.isFinite(stored) && stored >= 360 ? stored : 520;
    } catch {
      return 520;
    }
  });
  const [onlineUserIds, setOnlineUserIds] = useState<string[]>([]);
  const queryClient = useQueryClient();
  const wide = useMediaQuery("(min-width: 1200px)");
  const phone = useMediaQuery("(max-width: 47.99em)");
  const tablet = useMediaQuery("(min-width: 48em) and (max-width: 74.99em)");
  const changesDrawerWidth = Math.max(
    360,
    Math.min(
      changesWidth,
      Math.min(760, Math.round(window.innerWidth * 0.8)),
    ),
  );
  const [paletteOpened, setPaletteOpened] = useState(false);
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
  });

  /** History push guard: clicking the open thread twice must not stack
   *  duplicate entries. */
  const go = (path: string, options?: { replace?: boolean }) => {
    if (path === location.pathname) return;
    void navigate(path, options);
  };

  // Back closes the topmost overlay instead of leaving the screen behind it.
  useOverlayBack(navOpened, nav.close);
  useOverlayBack(changesOpened, changes.close);
  useOverlayBack(mergeOpened, merge.close);
  useOverlayBack(shareOpened, share.close);
  useOverlayBack(renameOpened, rename.close);
  useOverlayBack(revertOpened, revert.close);
  useOverlayBack(reviewOpened, review.close);
  useOverlayBack(paletteOpened, () => {
    setPaletteOpened(false);
    spotlight.close();
  });

  // Ctrl+` mirrors the editor panel shortcut. It stays live while the terminal
  // has focus, which is the only way back out from a fullscreen shell.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "`" && event.ctrlKey && !event.metaKey && !event.altKey) {
        event.preventDefault();
        if (terminalEnabled) setTerminalOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [terminalEnabled]);

  useEffect(() => {
    if (!navOpened) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") nav.close();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navOpened]);

  // A route change invalidates the changes drawer. The nav is different: it
  // doubles as the project/thread browser on phones, so opening a project must
  // keep it open to reveal that project's threads.
  useEffect(() => {
    changes.close();
  }, [route.kind, routeThreadId]);

  // Crossing into the three-column layout replaces the drawer with the inline
  // aside; drop the drawer state so it does not reappear on the way back.
  useEffect(() => {
    if (wide) changes.close();
  }, [wide]);

  // The phone drawer must not survive a resize into the tablet rail.
  useEffect(() => {
    if (!phone) nav.close();
  }, [phone]);

  // iOS keeps the keyboard up while the transcript scrolls underneath it.
  // Blur the composer on a real drag, but never on the keyboard's own resize
  // scroll or when the pointer went down inside the composer.
  useEffect(() => {
    let armed = false;
    let timer = 0;
    const arm = (event: PointerEvent) => {
      const target = event.target;
      if (
        target instanceof Element &&
        target.closest(".composer, .terminal-drawer")
      ) {
        return;
      }
      armed = true;
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        armed = false;
      }, 1_500);
    };
    const onScroll = () => {
      if (!armed) return;
      armed = false;
      window.clearTimeout(timer);
      blurActiveInput();
    };
    document.addEventListener("pointerdown", arm, true);
    document.addEventListener("scroll", onScroll, true);
    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("pointerdown", arm, true);
      document.removeEventListener("scroll", onScroll, true);
    };
  }, []);

  const openChanges = () => {
    blurActiveInput();
    changes.open();
  };

  // The phone tab bar opens the sheet from the bottom; when the nav drawer is
  // over the thread, let it finish its close transition first, otherwise the
  // two overlays fight over the same history entry.
  const openChangesFromTabs = () => {
    if (!selectedThread) return;
    if (navOpened) {
      nav.close();
      window.setTimeout(openChanges, 220);
      return;
    }
    openChanges();
  };

  const toggleNav = () => {
    if (!navOpened) blurActiveInput();
    nav.toggle();
  };

  const resizeChanges = (width: number) => {
    const next = Math.max(
      360,
      Math.min(width, Math.min(760, Math.round(window.innerWidth * 0.8))),
    );
    setChangesWidth(next);
    try {
      window.localStorage.setItem(
        "threadbase.changes-width",
        String(next),
      );
    } catch {
      // Private mode: the width simply does not persist.
    }
  };

  const toggleNavCollapsed = () => {
    setNavCollapsed((current) => {
      const next = !current;
      try {
        window.localStorage.setItem(
          "threadbase.nav-collapsed",
          next ? "1" : "0",
        );
      } catch {
        // Private mode: the preference simply does not persist.
      }
      return next;
    });
  };


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
      .catch((cause: unknown) => {
        if (cancelled) return;
        setSelectedThread(null);
        if (!(cause instanceof ApiError) || cause.status !== 404) return;
        notifications.show({
          message: "Thread not found — showing its project",
          color: "red",
        });
        if (route.kind === "thread") {
          go(pathFor({ kind: "project", projectId: route.projectId }), {
            replace: true,
          });
        }
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
    go(pathFor({ kind: "thread", projectId: thread.projectId, threadId: thread.id }));
  };

  const openProject = (projectId: string) => {
    setSelectedThread(null);
    setOnlineUserIds([]);
    changes.close();
    go(pathFor({ kind: "project", projectId }));
  };

  const openInbox = () => {
    setSelectedThread(null);
    setOnlineUserIds([]);
    changes.close();
    nav.close();
    go(pathFor({ kind: "inbox" }));
  };

  const openSettings = () => {
    setSelectedThread(null);
    changes.close();
    nav.close();
    go(pathFor({ kind: "settings" }));
  };

  // Read bookkeeping runs in the background: the row leaves the unread cache
  // immediately, and a failed write puts it back and says why.
  const markInboxItemRead = (threadId: string) => {
    if (pendingInboxReads.current.has(threadId)) return;
    pendingInboxReads.current.add(threadId);
    const snapshot = queryClient.getQueriesData<InboxPageDto>({
      queryKey: ["inbox", "unread"],
    });
    queryClient.setQueriesData<InboxPageDto>(
      { queryKey: ["inbox", "unread"] },
      (page) => (page ? withoutThreadInboxItems(page, threadId) : page),
    );
    void markInboxRead(threadId)
      .then(() => {
        void queryClient.invalidateQueries({ queryKey: ["inbox"] });
      })
      .catch((cause: unknown) => {
        for (const [queryKey, data] of snapshot) {
          queryClient.setQueryData(queryKey, data);
        }
        notifications.show({
          message:
            cause instanceof Error ? cause.message : "Could not mark item read",
          color: "red",
        });
      })
      .finally(() => {
        pendingInboxReads.current.delete(threadId);
      });
  };

  // The click is one history push: `navigate` ignores an identical path, so a
  // double-click cannot stack the same thread URL. The route effect above
  // resolves the thread from cache or fetches it; the loader covers the gap.
  const openInboxItem = (item: InboxItem) => {
    if (selectedThreadId.current !== item.threadId) {
      setSelectedThread(
        allThreads.data?.threads.find(
          (thread) => thread.id === item.threadId,
        ) ?? null,
      );
    }
    setAsideTab("changes");
    setOnlineUserIds([]);
    changes.close();
    go(
      pathFor({
        kind: "thread",
        projectId: item.projectId,
        threadId: item.threadId,
      }),
    );
    markInboxItemRead(item.threadId);
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
      ...(terminalEnabled
        ? [
            {
              id: "terminal",
              group: "Navigate",
              label: "Open terminal",
              description: "A shell in this machine, shared with the thread",
              leftSection: <IconTerminal2 size={16} stroke={1.5} />,
              onClick: () => {
                setTerminalOpen(true);
                spotlight.close();
              },
            },
          ]
        : []),
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
        header={{
          height: {
            base: "calc(48px + env(safe-area-inset-top))",
            sm: "calc(38px + env(safe-area-inset-top))",
          },
        }}
        navbar={{
          width: tablet && navCollapsed ? 0 : 260,
          breakpoint: "sm",
          collapsed: { mobile: !navOpened },
        }}
        aside={{
          width: 420,
          breakpoint: "lg",
          collapsed: { mobile: true, desktop: !showAside },
        }}
        footer={{
          height: { base: 56, sm: 26 },
        }}
        padding={0}
        withBorder={false}
        className="shell"
        data-nav-collapsed={navCollapsed ? "true" : undefined}
        data-nav-open={navOpened ? "true" : undefined}
      >
        <TitleBar
          title={title}
          running={
            route.kind === "thread" && selectedThread?.status === "running"
          }
          participants={participants}
          onlineUserIds={onlineUserIds}
          unreadCount={inboxUnread.data?.unread ?? 0}
          canOpenChanges={showAside}
          canManage={Boolean(selectedThread) && route.kind === "thread"}
          isPinned={selectedThread?.pinned ?? false}
          navOpened={navOpened}
          navCollapsed={navCollapsed}
          onToggleNav={toggleNav}
          onToggleNavCollapsed={toggleNavCollapsed}
          canTerminal={terminalEnabled}
          terminalOpen={terminalOpen}
          onToggleTerminal={() => setTerminalOpen((open) => !open)}
          onOpenPalette={() => spotlight.open()}
          onOpenInbox={openInbox}
          onOpenChanges={openChanges}
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
            <InboxView onOpenThread={openInboxItem} />
          ) : view === "settings" ? (
            <SettingsView />
          ) : selectedThread ? (
            <RealThreadView
              key={selectedThread.id}
              threadId={selectedThread.id}
              onPresence={setOnlineUserIds}
              presencePanel={terminalOpen ? "terminal" : "transcript"}
            />
          ) : route.kind === "thread" ? (
            <EmptyState loading title="Opening thread" />
          ) : (
            <WorkspaceEmpty />
          )}
          {terminalOpen ? (
            <TerminalDrawer
              key={terminalThreadId ?? "home"}
              threadId={terminalThreadId}
              fullscreen={terminalFull}
              onFullscreen={setTerminalFull}
              onClose={() => {
                setTerminalOpen(false);
                setTerminalFull(false);
                // The shell outlives the drawer; refresh the status-bar chip
                // instead of waiting for the next poll.
                void queryClient.invalidateQueries({
                  queryKey: ["terminal-sessions"],
                });
              }}
            />
          ) : null}
        </AppShell.Main>
        {wide ? (
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
        ) : null}
        {phone ? (
          <MobileTabBar
            view={view}
            unreadCount={inboxUnread.data?.unread ?? 0}
            threadsOpened={navOpened}
            changesEnabled={Boolean(selectedThread)}
            onInbox={openInbox}
            onThreads={toggleNav}
            onChanges={openChangesFromTabs}
            onSettings={openSettings}
          />
        ) : (
          <StatusBar
            thread={selectedThread ?? undefined}
            workspace={view !== "thread" || selectedThread === null}
            terminal={
              terminalEnabled && liveTerminal && !terminalOpen
                ? { onClick: () => setTerminalOpen(true) }
                : undefined
            }
          />
        )}
        {/* Inside the shell, so the transformed shell's stacking context keeps
            the scrim below the navbar and header. */}
        {phone && navOpened ? (
          <button
            type="button"
            className="nav-scrim"
            tabIndex={-1}
            aria-label="Close navigation"
            onClick={nav.close}
          />
        ) : null}
      </AppShell>

      {!wide && showAside ? (
        <Drawer
          opened={changesOpened}
          onClose={changes.close}
          position={phone ? "bottom" : "right"}
          size={phone ? "92%" : changesDrawerWidth}
          padding={0}
          withCloseButton={false}
          className={phone ? "changes-drawer changes-sheet" : "changes-drawer"}
        >
          {!phone ? (
            <DrawerResizeHandle
              width={changesDrawerWidth}
              onResize={resizeChanges}
            />
          ) : null}
          <div className="sheet-stack">
            {phone ? <SheetGrabber onClose={changes.close} /> : null}
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
          </div>
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
          // Let the review's history entry unwind before the drawer pushes its
          // own; otherwise the drawer can be dismissed by the review's back.
          window.setTimeout(openChanges, 0);
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
        onSpotlightOpen={() => setPaletteOpened(true)}
        onSpotlightClose={() => setPaletteOpened(false)}
        searchProps={{
          placeholder: "Search threads or run a command…",
        }}
        className="spotlight"
      />
    </>
  );
}

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/inbox" element={<Workspace />} />
        <Route path="/settings" element={<Workspace />} />
        <Route path="/p/:projectId" element={<Workspace />} />
        <Route path="/p/:projectId/t/:threadId" element={<Workspace />} />
        <Route path="*" element={<Navigate to="/inbox" replace />} />
      </Routes>
    </BrowserRouter>
  );
}
