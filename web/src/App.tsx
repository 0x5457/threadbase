import {
  AppShell,
  Button,
  Center,
  Drawer,
  Loader,
  useMantineColorScheme,
} from "@mantine/core";
import { useDebouncedValue, useDisclosure, useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import {
  Spotlight,
  spotlight,
  type SpotlightActionData,
} from "@mantine/spotlight";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useThreadPlacement } from "./lib/use-placement";
import { useSelectedThread } from "./lib/use-selected-thread";
import {
  IconInbox,
  IconLayoutList,
  IconMessage,
  IconMoon,
  IconSettings,
  IconSun,
  IconTerminal2,
} from "@tabler/icons-react";
import {
  lazy,
  startTransition,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  BrowserRouter,
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from "react-router";
import {
  forkThread,
  getMe,
  getState,
  getThread,
  listTerminalSessions,
  listInbox,
  listProjects,
  markInboxRead,
  searchWorkspace,
  updateThread,
  type InboxItem,
  type Project,
  type SearchHit,
  type TerminalTarget,
  type ThreadSummary,
} from "./api";
import type { InboxPageDto } from "./api/types/InboxPageDto";
import type { AsideTab, AsideView } from "./components/AsidePanels";
import { DrawerResizeHandle } from "./components/DrawerResizeHandle";
import { InboxView } from "./components/InboxView";
import { MobileTabBar } from "./components/MobileTabBar";
import { ReviewDialog } from "./components/ReviewDialog";
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
import { whenIdle } from "./lib/idle";
import { pathFor, useRoute } from "./lib/router";
import { parseTmuxSession, withSessionParam } from "./lib/sessions";

// The thread, settings and diff surfaces pull in the editor, CRDT, terminal
// and highlighter stacks. Keeping them out of the shell chunk is what makes
// the first paint after sign-in cheap.
const RealThreadView = lazy(() =>
  import("./components/RealThreadView").then((module) => ({
    default: module.RealThreadView,
  })),
);
const SubagentThreadView = lazy(() =>
  import("./components/SubagentThreadView").then((module) => ({
    default: module.SubagentThreadView,
  })),
);
const SettingsView = lazy(() =>
  import("./components/SettingsView").then((module) => ({
    default: module.SettingsView,
  })),
);
const AsidePanels = lazy(() =>
  import("./components/AsidePanels").then((module) => ({
    default: module.AsidePanels,
  })),
);
const TerminalDrawer = lazy(() =>
  import("./components/terminal/TerminalDrawer").then((module) => ({
    default: module.TerminalDrawer,
  })),
);
const SessionsView = lazy(() =>
  import("./components/sessions/SessionsView").then((module) => ({
    default: module.SessionsView,
  })),
);
const SessionSwitcher = lazy(() =>
  import("./components/sessions/SessionSwitcher").then((module) => ({
    default: module.SessionSwitcher,
  })),
);

type View = "inbox" | "sessions" | "thread" | "settings" | "project";

/** Placeholder for a code-split surface that is still downloading. */
function ChunkLoading() {
  return (
    <Center h="100%" py="xl">
      <Loader size="sm" />
    </Center>
  );
}

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

type PaletteInputs = {
  threads: ThreadSummary[];
  projects: Project[];
  hits: SearchHit[] | undefined;
  query: string;
  debounced: string;
  dark: boolean;
  terminalEnabled: boolean;
  onThread: (thread: ThreadSummary) => void;
  onSearchHit: (hit: SearchHit) => void;
  onInbox: () => void;
  onSessions: () => void;
  onSettings: () => void;
  onTerminal: () => void;
  onSwitchSession: () => void;
  onToggleTheme: () => void;
  close: () => void;
};

/** The command palette as a pure function of its inputs, so the shell stays a
 *  thin layer over its data. `debounced` matching `query` is the guard that
 *  keeps stale search results out of the list. */
function buildPaletteActions({
  threads,
  projects,
  hits: searchHits,
  query: rawQuery,
  debounced,
  dark,
  terminalEnabled,
  onThread,
  onSearchHit,
  onInbox,
  onSessions,
  onSettings,
  onTerminal,
  onSwitchSession,
  onToggleTheme,
  close,
}: PaletteInputs): SpotlightActionData[] {
  const names = new Map(projects.map((project) => [project.id, project.name]));
  const text = rawQuery.trim();
  const query = text.toLowerCase();
  const matches = (value: string | undefined) =>
    !query || (value ?? "").toLowerCase().includes(query);
  const threadActions = (query ? [] : threads.slice(0, 20))
    .filter(
      (thread) =>
        matches(thread.title) ||
        matches(names.get(thread.projectId)) ||
        matches(thread.agent),
    )
    .map((thread) => ({
      id: `thread:${thread.id}`,
      group: "Threads",
      label: thread.title,
      description: `${names.get(thread.projectId) ?? "Project"} · ${thread.agent} · ${thread.status.replaceAll("_", " ")}`,
      leftSection: <IconMessage size={15} stroke={1.5} />,
      onClick: () => {
        onThread(thread);
        close();
      },
    }));

  const hits =
    text && searchHits && debounced === text
      ? searchHits.map((hit) => {
          const threadLabel =
            hit.kind === "thread" ? hit.threadTitle : hit.snippet;
          return {
            id: `hit:${hit.kind}:${hit.refId}`,
            group:
              hit.kind === "comment"
                ? "Comments"
                : hit.kind === "thread"
                  ? "Thread titles"
                  : "Messages",
            label: (threadLabel || hit.threadTitle)
              .replace(/\s+/g, " ")
              .trim(),
            description: `${hit.threadTitle} · ${hit.projectName}`,
            leftSection: <IconMessage size={15} stroke={1.5} />,
            onClick: () => {
              void onSearchHit(hit);
            },
          };
        })
      : [];

  const nav = [
    {
      id: "inbox",
      group: "Navigate",
      label: "Go to Inbox",
      description: "Approvals, questions and mentions",
      leftSection: <IconInbox size={16} stroke={1.5} />,
      onClick: () => {
        onInbox();
        close();
      },
    },
    ...(terminalEnabled
      ? [
          {
            id: "terminal",
            group: "Navigate",
            label: "Open terminal",
            description: "The current thread's shell, shared with the team",
            leftSection: <IconTerminal2 size={16} stroke={1.5} />,
            onClick: () => {
              onTerminal();
              close();
            },
          },
          {
            id: "sessions",
            group: "Navigate",
            label: "Go to Sessions",
            description: "Threads and tmux shells on this machine",
            leftSection: <IconLayoutList size={16} stroke={1.5} />,
            onClick: () => {
              onSessions();
              close();
            },
          },
          {
            id: "switch-session",
            group: "Navigate",
            label: "Switch session",
            description: "Jump to a thread or tmux shell",
            leftSection: <IconLayoutList size={16} stroke={1.5} />,
            onClick: () => {
              onSwitchSession();
              close();
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
        onSettings();
        close();
      },
    },
  ].filter(
    (action) => matches(action.label) || matches(action.description),
  );

  return [
    ...nav,
    ...hits,
    ...threadActions,
    ...(text
      ? []
      : [
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
              onToggleTheme();
              close();
            },
          },
        ]),
  ];
}

function Workspace() {
  const route = useRoute();
  const location = useLocation();
  const navigate = useNavigate();
  // The URL owns navigation; everything below derives from it so reloads,
  // deep links and the back button behave the same as in-app clicks.
  const view: View =
    route.kind === "sessions"
      ? "sessions"
      : route.kind === "settings"
        ? "settings"
        : route.kind === "project"
          ? "project"
          : route.kind === "thread" || route.kind === "subagent"
            ? "thread"
            : "inbox";
  /** History push guard: clicking the open thread twice must not stack
   *  duplicate entries. The refs keep `go` stable across navigations so the
   *  memoized sidebar tree is not invalidated by every route change. */
  const pathnameRef = useRef(location.pathname + location.search);
  const navigateRef = useRef(navigate);
  useEffect(() => {
    pathnameRef.current = location.pathname + location.search;
    navigateRef.current = navigate;
  }, [location.pathname, location.search, navigate]);
  const go = useCallback(
    (path: string, options?: { replace?: boolean }) => {
      if (path === pathnameRef.current) return;
      pathnameRef.current = path;
      void navigateRef.current(path, options);
    },
    [],
  );
  // The open thread resolves from the URL during render, backed by the polled
  // list; the hook also fetches a deep-linked thread the list has not seen.
  const { routeThreadId, selectedThread, setSelectedThread, allThreads } =
    useSelectedThread(route, go);
  const pendingInboxReads = useRef<Set<string>>(new Set());
  const [navOpened, nav] = useDisclosure(false);
  // A phone mounts the nav tree once — after the shell goes idle, or on the
  // first open — instead of putting the whole project tree on the initial
  // screen's critical path.
  const [navEverOpened, setNavEverOpened] = useState(false);
  useEffect(() => {
    if (navOpened) setNavEverOpened(true);
  }, [navOpened]);
  const [changesOpened, changes] = useDisclosure(false);
  const [mergeOpened, merge] = useDisclosure(false);
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
  const [asideCollapsed, setAsideCollapsed] = useState(() => {
    try {
      const stored = window.localStorage.getItem("threadbase.aside-collapsed");
      // First visit: keep the transcript full-width. The 420px diff aside used
      // to open on every worktree thread and squeeze the conversation, so it
      // now opens on demand (⌘⇧B or the header toggle) and remembers the choice.
      return stored === null ? true : stored === "1";
    } catch {
      return true;
    }
  });
  const [asideTab, setAsideTab] = useState<AsideTab>("changes");
  const [asideView, setAsideView] = useState<AsideView>({ kind: "current" });
  // The file browser is mounted on first use only; until then it costs a
  // directory request and DOM that a thread open does not need.
  const [filesOpened, setFilesOpened] = useState(false);
  // History is likewise mounted on first use, and keeps its scroll when the
  // user switches back to Changes.
  const [historyOpened, setHistoryOpened] = useState(false);
  // The terminal drawer is workspace state, not thread state: switching thread
  // remounts it against that thread's shell.
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [terminalFull, setTerminalFull] = useState(false);
  const [sessionSwitcherOpen, setSessionSwitcherOpen] = useState(false);
  // A host tmux session is an overlay, so it lives in the query string rather
  // than in component state: `?session=tmux:<socket>:<$N>` survives reloads,
  // is shareable, and leaves the surface underneath untouched.
  const tmuxRef = useMemo(
    () => parseTmuxSession(location.search),
    [location.search],
  );
  const terminalThreadId = view === "thread" ? (selectedThread?.id ?? null) : null;
  const terminalTarget = useMemo<TerminalTarget>(
    () =>
      tmuxRef
        ? { kind: "host", socket: tmuxRef.socket, id: tmuxRef.id }
        : { kind: "workbench", threadId: terminalThreadId },
    [terminalThreadId, tmuxRef],
  );
  const terminalVisible = terminalOpen || tmuxRef !== null;
  // The server decides whether the feature exists at all; only administrators
  // can drive a shell. Entry points stay hidden otherwise.
  const stateQuery = useQuery({
    queryKey: ["state"],
    queryFn: getState,
    staleTime: 60_000,
  });
  const me = useQuery({ queryKey: ["me"], queryFn: getMe, staleTime: 60_000 });
  const terminalEnabled =
    Boolean(stateQuery.data?.terminal) &&
    Boolean(
      me.data &&
        (me.data.user.isAdmin || me.data.user.access === "member"),
    );
  const hostTmuxEnabled =
    terminalEnabled && Boolean(stateQuery.data?.terminalHost);
  // A shell that outlived the drawer: the status bar keeps it discoverable.
  const terminalSessions = useQuery({
    queryKey: ["terminal-sessions", terminalThreadId],
    queryFn: () => listTerminalSessions(terminalThreadId),
    enabled: terminalEnabled,
    staleTime: 5_000,
    refetchInterval: terminalVisible ? false : 10_000,
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
  const [paletteQuery, setPaletteQuery] = useState("");
  const [debouncedQuery] = useDebouncedValue(paletteQuery.trim(), 180);
  const { colorScheme, toggleColorScheme } = useMantineColorScheme();
  const dark = colorScheme === "dark";

  // The meta tag tints mobile browser chrome. It must follow the app's manual
  // scheme, not `prefers-color-scheme`, because the theme choice is local.
  useEffect(() => {
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", dark ? "#1c1f24" : "#f4f4f5");
  }, [dark]);

  // Warm the two route chunks once the shell is idle. Opening a thread is the
  // main next step from Inbox, and Settings is one tab away; neither should
  // wait for a download on a phone.
  useEffect(() => {
    const cancelThread = whenIdle(
      () => void import("./components/RealThreadView"),
    );
    const cancelSettings = whenIdle(
      () => void import("./components/SettingsView"),
    );
    return () => {
      cancelThread();
      cancelSettings();
    };
  }, []);

  // A phone builds the project tree the first time the drawer opens, which is
  // why that tap used to cost a full React mount. Warm it while the shell is
  // idle; the drawer stays hidden until it is opened, and the sidebar's
  // queries are already cached by the shell.
  useEffect(() => {
    if (!phone || navEverOpened) return;
    return whenIdle(() => setNavEverOpened(true));
  }, [navEverOpened, phone]);

  // The diff stack is only reachable on the wide layout's aside; load it after
  // the transcript so opening a thread never waits for the highlighter.
  useEffect(() => {
    if (!wide) return;
    return whenIdle(() => void import("./components/AsidePanels"));
  }, [wide]);

  const projects = useQuery({
    queryKey: ["projects"],
    queryFn: listProjects,
    staleTime: 10_000,
  });
  const inboxUnread = useQuery({
    queryKey: ["inbox-unread"],
    queryFn: () => listInbox("unread", undefined, 1),
    staleTime: 5_000,
    // The shell owns the badge's cadence; the sidebar and title bar share this
    // observer instead of each scheduling their own timer.
    refetchInterval: 5_000,
    refetchIntervalInBackground: false,
  });
  const search = useQuery({
    queryKey: ["search", debouncedQuery],
    queryFn: () => searchWorkspace(debouncedQuery, { limit: 20 }),
    enabled: paletteOpened && debouncedQuery.length >= 2,
    staleTime: 5_000,
  });

  // Back closes the topmost overlay instead of leaving the screen behind it.
  useOverlayBack(navOpened, nav.close);
  useOverlayBack(changesOpened, changes.close);
  useOverlayBack(mergeOpened, merge.close);
  useOverlayBack(renameOpened, rename.close);
  useOverlayBack(revertOpened, revert.close);
  useOverlayBack(reviewOpened, review.close);
  useOverlayBack(sessionSwitcherOpen, () => setSessionSwitcherOpen(false));
  useOverlayBack(paletteOpened, () => {
    setPaletteOpened(false);
    spotlight.close();
  });

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

  const setAsideOpen = useCallback((open: boolean) => {
    setAsideCollapsed(!open);
    try {
      window.localStorage.setItem(
        "threadbase.aside-collapsed",
        open ? "0" : "1",
      );
    } catch {
      // Private mode: the preference simply does not persist.
    }
  }, []);

  const collapseAside = useCallback(
    () => setAsideOpen(false),
    [setAsideOpen],
  );
  const toggleAside = useCallback(
    () => setAsideOpen(asideCollapsed),
    [asideCollapsed, setAsideOpen],
  );
  const showFiles = useCallback(() => {
    setFilesOpened(true);
    setAsideTab("files");
  }, []);
  const showHistory = useCallback(() => {
    setHistoryOpened(true);
    setAsideTab("history");
  }, []);
  const showChanges = useCallback(() => {
    setAsideView({ kind: "current" });
    setAsideTab("changes");
  }, []);

  const showTurn = useCallback((turnId: string) => {
    setAsideView((current) => ({
      kind: "turn",
      turnId,
      label:
        current.kind === "commit"
          ? (current.commit.turnPrompt ?? current.commit.shortSha)
          : "this turn",
    }));
    setAsideTab("changes");
  }, []);
  // Opening a different thread must not leave the panel on a commit that
  // belongs to the previous worktree.
  useEffect(() => {
    setAsideView({ kind: "current" });
  }, [selectedThread?.id]);

  const openChanges = () => {
    if (!hasWorktree) return;
    setAsideView({ kind: "current" });
    setAsideTab("changes");
    blurActiveInput();
    // The desktop panel is persistent, not a drawer: reveal it in place.
    if (wide) {
      setAsideOpen(true);
      return;
    }
    changes.open();
  };

  const openHistory = useCallback(() => {
    if (!selectedThread?.worktreePath) return;
    blurActiveInput();
    showHistory();
    if (wide) {
      setAsideOpen(true);
      return;
    }
    changes.open();
  }, [changes.open, selectedThread?.worktreePath, setAsideOpen, showHistory, wide]);

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
    const next = !navCollapsed;
    setNavCollapsed(next);
    try {
      window.localStorage.setItem(
        "threadbase.nav-collapsed",
        next ? "1" : "0",
      );
    } catch {
      // Private mode: the preference simply does not persist.
    }
  };


  const subagentRoute = route.kind === "subagent";
  // Prefer the polling thread list: it carries fresher subagent statuses than
  // the object the route resolved once.
  const liveThread =
    (routeThreadId
      ? allThreads.data?.threads.find((thread) => thread.id === routeThreadId)
      : null) ?? selectedThread;
  const selectedSubagent = subagentRoute
    ? (liveThread?.subagents.find(
        (subagent) => String(subagent.id) === route.subagentId,
      ) ?? null)
    : null;
  const showAside =
    view === "thread" && selectedThread !== null && !subagentRoute;
  const hasWorktree = Boolean(selectedThread?.worktreePath);
  const threadProject = projects.data?.projects.find(
    (project) => project.id === selectedThread?.projectId,
  );
  // A thread in a project without a setup command has nothing to run; keeping
  // the menu item would leave a control that can only fail.
  const canRunSetup = Boolean(threadProject?.setupCommand?.trim());

  // ⌘/Ctrl+Shift+B hides or reveals the changes panel without leaving the
  // thread; the preference persists like the nav width.
  useEffect(() => {
    if (!showAside) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || !event.shiftKey) return;
      if (event.key.toLowerCase() !== "b") return;
      event.preventDefault();
      setAsideOpen(!asideCollapsed);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [asideCollapsed, setAsideOpen, showAside]);
  const title =
    view === "sessions"
      ? "Sessions"
      : view === "inbox"
        ? "Inbox"
        : view === "settings"
          ? "Settings"
          : selectedSubagent
            ? [selectedSubagent.title, selectedSubagent.description]
                .filter(Boolean)
                .join(" · ")
            : (selectedThread?.title ?? "Workspace");
  const participants =
    view === "thread" ? (selectedThread?.participants ?? []) : [];

  const openSubagent = useCallback(
    (thread: ThreadSummary, subagent: ThreadSummary["subagents"][number]) => {
      setSelectedThread(thread);
      setAsideTab("changes");
      setOnlineUserIds([]);
      changes.close();
      go(
        pathFor({
          kind: "subagent",
          projectId: thread.projectId,
          threadId: thread.id,
          subagentId: String(subagent.id),
        }),
      );
    },
    [changes.close, go],
  );

  const openThread = useCallback(
    (thread: ThreadSummary) => {
      setSelectedThread(thread);
      setAsideTab("changes");
      setOnlineUserIds([]);
      changes.close();
      go(pathFor({ kind: "thread", projectId: thread.projectId, threadId: thread.id }));
    },
    [changes.close, go],
  );

  const openThreadById = useCallback(
    async (threadId: string, shell = false) => {
      try {
        const thread =
          allThreads.data?.threads.find((entry) => entry.id === threadId) ??
          (await getThread(threadId)).thread;
        openThread(thread);
        if (shell) setTerminalOpen(true);
      } catch (cause) {
        notifications.show({
          message:
            cause instanceof Error ? cause.message : "Could not open that thread",
          color: "red",
        });
      }
    },
    [allThreads.data?.threads, openThread],
  );

  const openTmux = useCallback(
    (socket: string, id: string) => {
      setTerminalOpen(false);
      setTerminalFull(false);
      const search = withSessionParam(location.search, { socket, id });
      void navigate(`${location.pathname}${search ? `?${search}` : ""}`);
    },
    [location.pathname, location.search, navigate],
  );

  const closeTmux = useCallback(() => {
    setTerminalFull(false);
    const search = withSessionParam(location.search, null);
    void navigate(`${location.pathname}${search ? `?${search}` : ""}`, {
      replace: true,
    });
  }, [location.pathname, location.search, navigate]);

  const openSessions = useCallback(() => {
    startTransition(() => {
      setSelectedThread(null);
      setOnlineUserIds([]);
      changes.close();
      nav.close();
      go(pathFor({ kind: "sessions" }));
    });
  }, [changes.close, go, nav.close]);

  // Ctrl+` mirrors the editor panel shortcut. It stays live while the terminal
  // has focus, which is the only way back out from a fullscreen shell.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "`" && event.ctrlKey && !event.metaKey && !event.altKey) {
        event.preventDefault();
        if (!terminalEnabled) return;
        if (tmuxRef) closeTmux();
        else setTerminalOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeTmux, terminalEnabled, tmuxRef]);

  const openSearchHit = useCallback(
    async (hit: SearchHit) => {
      spotlight.close();
      const cached = allThreads.data?.threads.find(
        (thread) => thread.id === hit.threadId,
      );
      if (cached) {
        openThread(cached);
        return;
      }
      try {
        const { thread } = await getThread(hit.threadId);
        openThread(thread);
      } catch (cause) {
        notifications.show({
          message:
            cause instanceof Error ? cause.message : "Could not open that result",
          color: "red",
        });
      }
    },
    [allThreads.data, openThread],
  );

  const openProject = useCallback(
    (projectId: string) => {
      startTransition(() => {
        setSelectedThread(null);
        setOnlineUserIds([]);
        changes.close();
        go(pathFor({ kind: "project", projectId }));
      });
    },
    [changes.close, go],
  );

  const openInbox = useCallback(() => {
    startTransition(() => {
      setSelectedThread(null);
      setOnlineUserIds([]);
      changes.close();
      nav.close();
      go(pathFor({ kind: "inbox" }));
    });
  }, [changes.close, go, nav.close]);

  const openSettings = useCallback(() => {
    // Tearing down a long transcript and the diff panel is the most expensive
    // navigation in the app; let React render Settings concurrently instead of
    // blocking the click on the teardown.
    startTransition(() => {
      setSelectedThread(null);
      changes.close();
      nav.close();
      go(pathFor({ kind: "settings" }));
    });
  }, [changes.close, go, nav.close]);

  // Read bookkeeping runs in the background: the row leaves the unread cache
  // immediately, and a failed write puts it back and says why.
  const markInboxItemRead = useCallback((threadId: string) => {
    if (pendingInboxReads.current.has(threadId)) return;
    pendingInboxReads.current.add(threadId);
    const snapshot = queryClient.getQueriesData<InboxPageDto>({
      queryKey: ["inbox-unread"],
    });
    queryClient.setQueriesData<InboxPageDto>(
      { queryKey: ["inbox-unread"] },
      (page) => (page ? withoutThreadInboxItems(page, threadId) : page),
    );
    void markInboxRead(threadId)
      .then(() => {
        void queryClient.invalidateQueries({ queryKey: ["inbox"] });
        void queryClient.invalidateQueries({ queryKey: ["inbox-unread"] });
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
  }, [queryClient]);

  // The click is one history push: `navigate` ignores an identical path, so a
  // double-click cannot stack the same thread URL. The route effect above
  // resolves the thread from cache or fetches it; the loader covers the gap.
  const openInboxItem = useCallback(
    (item: InboxItem) => {
      if (selectedThread?.id !== item.threadId) {
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
    },
    [
      allThreads.data,
      changes.close,
      go,
      markInboxItemRead,
      selectedThread?.id,
    ],
  );

  const place = useThreadPlacement();

  const refreshThreadLists = (thread?: ThreadSummary) => {
    void queryClient.invalidateQueries({ queryKey: ["threads-all"] });
    void queryClient.invalidateQueries({ queryKey: ["projects"] });
    if (thread) {
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

  const actions = useMemo<SpotlightActionData[]>(
    () =>
      buildPaletteActions({
        threads: allThreads.data?.threads ?? [],
        projects: projects.data?.projects ?? [],
        hits: search.data?.hits,
        query: paletteQuery,
        debounced: debouncedQuery,
        dark,
        terminalEnabled,
        onThread: openThread,
        onSearchHit: openSearchHit,
        onInbox: openInbox,
        onSessions: openSessions,
        onSettings: openSettings,
        onTerminal: () => {
          if (tmuxRef) closeTmux();
          setTerminalOpen(true);
        },
        onSwitchSession: () => setSessionSwitcherOpen(true),
        onToggleTheme: toggleColorScheme,
        close: spotlight.close,
      }),
    [
      allThreads.data,
      dark,
      debouncedQuery,
      openInbox,
      openSearchHit,
      openSessions,
      openSettings,
      openThread,
      paletteQuery,
      projects.data,
      search.data,
      spotlight.close,
      terminalEnabled,
      tmuxRef,
      toggleColorScheme,
    ],
  );

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
          collapsed: {
            mobile: true,
            desktop: !showAside || !hasWorktree || asideCollapsed,
          },
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
            subagentRoute
              ? selectedSubagent?.status === "running"
              : route.kind === "thread" && selectedThread?.status === "running"
          }
          participants={participants}
          onlineUserIds={onlineUserIds}
          unreadCount={inboxUnread.data?.unread ?? 0}
          canOpenChanges={showAside && hasWorktree}
          asideCollapsed={asideCollapsed}
          onToggleAside={toggleAside}
          hasWorktree={hasWorktree}
          threadOpen={view === "thread" && selectedThread !== null}
          canRunSetup={canRunSetup}
          branch={selectedThread?.branch ?? ""}
          baseBranch={selectedThread?.baseBranch ?? ""}
          canManage={Boolean(selectedThread) && route.kind === "thread"}
          isPinned={selectedThread?.pinned ?? false}
          allowedActions={
            route.kind === "thread" ? selectedThread?.allowedActions : []
          }
          navOpened={navOpened}
          navCollapsed={navCollapsed}
          onToggleNav={toggleNav}
          onToggleNavCollapsed={toggleNavCollapsed}
          canTerminal={terminalEnabled}
          terminalOpen={terminalVisible}
          onToggleTerminal={() => {
            if (tmuxRef) closeTmux();
            else setTerminalOpen((open) => !open);
          }}
          canSessions={terminalEnabled}
          sessionsActive={view === "sessions"}
          onOpenSessions={openSessions}
          onOpenPalette={() => spotlight.open()}
          onOpenInbox={openInbox}
          onOpenChanges={openChanges}
          onOpenHistory={openHistory}
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
          onArchive={(archived) => {
            if (!selectedThread) return;
            void runAction(async () => {
              await updateThread(selectedThread.id, { archived });
              if (archived) openInbox();
              refreshThreadLists(selectedThread);
              notifications.show({
                color: "teal",
                message: (
                  <span className="undo-toast">
                    {archived ? "Thread archived" : "Thread unarchived"}
                    <button
                      type="button"
                      className="undo-toast-action"
                      onClick={() => {
                        void runAction(async () => {
                          await updateThread(selectedThread.id, {
                            archived: !archived,
                          });
                          refreshThreadLists(selectedThread);
                        });
                      }}
                    >
                      Undo
                    </button>
                  </span>
                ),
              });
            });
          }}
          onPlacement={(action) => {
            if (!selectedThread) return;
            void runAction(() => place(selectedThread, action));
          }}
        />
        {/* Mantine keeps a collapsed navbar's children mounted, so on a phone
            the whole project tree used to be built behind the closed drawer on
            every screen. Mount it after idle (or on first open); tablets and
            desktops always show it. */}
        {wide || tablet || navOpened || navEverOpened ? (
          <Sidebar
            phone={phone}
            onNavigate={nav.close}
            onOpenInbox={openInbox}
            onOpenSessions={openSessions}
            onOpenSettings={openSettings}
            showSessions={terminalEnabled}
            activeView={view}
            selectedThreadId={selectedThread?.id ?? null}
            selectedSubagentId={subagentRoute ? route.subagentId : null}
            activeProjectId={
              route.kind === "project" ||
              route.kind === "thread" ||
              route.kind === "subagent"
                ? route.projectId
                : null
            }
            onSelectThread={openThread}
            onSelectSubagent={openSubagent}
            onSelectProject={openProject}
          />
        ) : null}
        <AppShell.Main className="main">
          {view === "sessions" ? (
            <Suspense fallback={<ChunkLoading />}>
              <SessionsView
                hostEnabled={hostTmuxEnabled}
                onOpenThread={(threadId) => void openThreadById(threadId)}
                onOpenThreadShell={(threadId) =>
                  void openThreadById(threadId, true)
                }
                onOpenTmux={openTmux}
              />
            </Suspense>
          ) : view === "inbox" ? (
            <InboxView onOpenThread={openInboxItem} />
          ) : view === "settings" ? (
            <Suspense fallback={<ChunkLoading />}>
              <SettingsView />
            </Suspense>
          ) : selectedThread && subagentRoute ? (
            <Suspense fallback={<EmptyState loading title="Opening subagent" />}>
              {selectedSubagent ? (
                <SubagentThreadView
                  key={`${selectedThread.id}:${selectedSubagent.id}`}
                  threadId={selectedThread.id}
                  subagent={selectedSubagent}
                  parentTitle={selectedThread.title}
                  onBack={() => openThread(selectedThread)}
                />
              ) : (
                <EmptyState
                  title="Subagent not found"
                  action={
                    <Button
                      variant="default"
                      size="compact-sm"
                      onClick={() => openThread(selectedThread)}
                    >
                      Back to thread
                    </Button>
                  }
                >
                  This thread no longer reports that subagent.
                </EmptyState>
              )}
            </Suspense>
          ) : selectedThread ? (
            <Suspense fallback={<EmptyState loading title="Opening thread" />}>
              <RealThreadView
                key={selectedThread.id}
                threadId={selectedThread.id}
                onPresence={setOnlineUserIds}
                presencePanel={terminalVisible ? "terminal" : "transcript"}
              />
            </Suspense>
          ) : route.kind === "thread" ? (
            <EmptyState loading title="Opening thread" />
          ) : (
            <WorkspaceEmpty />
          )}
          {terminalVisible ? (
            <Suspense fallback={null}>
              <TerminalDrawer
                key={
                  (terminalTarget.kind === "host"
                    ? `host:${terminalTarget.socket}:${terminalTarget.id}`
                    : `thread:${terminalTarget.threadId ?? "home"}`) +
                  (phone ? ":phone" : ":wide")
                }
                target={terminalTarget}
                fullscreen={terminalFull}
                onFullscreen={setTerminalFull}
                onBrowse={
                  terminalEnabled
                    ? () => setSessionSwitcherOpen(true)
                    : undefined
                }
                onOpenSessions={openSessions}
                onClose={() => {
                  if (tmuxRef) {
                    closeTmux();
                  } else {
                    setTerminalOpen(false);
                    setTerminalFull(false);
                  }
                  // The shell outlives the drawer; refresh the status-bar chip
                  // and the Sessions surface instead of waiting for the next
                  // poll.
                  void queryClient.invalidateQueries({
                    queryKey: ["terminal-sessions"],
                  });
                  void queryClient.invalidateQueries({
                    queryKey: ["sessions"],
                  });
                }}
              />
            </Suspense>
          ) : null}
        </AppShell.Main>
        {wide ? (
          <AppShell.Aside p={0} className="aside">
            {/* A collapsed aside is `display: none`, but Mantine keeps its
                children mounted. That mounted panel polled the thread, picked
                its first file and parsed that diff on every thread switch —
                invisible work that froze the tab outright when the file was
                large. Mount the panels only while the aside is on screen. */}
            {selectedThread && hasWorktree && !asideCollapsed ? (
              <Suspense fallback={<ChunkLoading />}>
                <AsidePanels
                  key={selectedThread.id}
                  threadId={selectedThread.id}
                  tab={asideTab}
                  view={asideView}
                  filesOpened={filesOpened}
                  historyOpened={historyOpened}
                  showTurn={showTurn}
                  showChanges={showChanges}
                  showFiles={showFiles}
                  showHistory={openHistory}
                  onSelectCommit={(commit) => {
                    setAsideView({ kind: "commit", commit });
                    setAsideTab("changes");
                  }}
                  onClose={collapseAside}
                />
              </Suspense>
            ) : null}
          </AppShell.Aside>
        ) : null}
        {phone ? (
          <MobileTabBar
            view={view}
            unreadCount={inboxUnread.data?.unread ?? 0}
            threadsOpened={navOpened}
            changesEnabled={
              Boolean(selectedThread) && hasWorktree && !subagentRoute
            }
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
              !terminalVisible && terminalEnabled && liveTerminal
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

      {!wide && showAside && hasWorktree ? (
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
              <Suspense fallback={<ChunkLoading />}>
                <AsidePanels
                  key={selectedThread.id}
                  threadId={selectedThread.id}
                  tab={asideTab}
                  view={asideView}
                  filesOpened={filesOpened}
                  historyOpened={historyOpened}
                  showTurn={showTurn}
                  showChanges={showChanges}
                  showFiles={showFiles}
                  showHistory={openHistory}
                  onSelectCommit={(commit) => {
                    setAsideView({ kind: "commit", commit });
                    setAsideTab("changes");
                  }}
                  onClose={changes.close}
                />
              </Suspense>
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

      {terminalEnabled && sessionSwitcherOpen ? (
        <Suspense fallback={null}>
          <SessionSwitcher
            opened
            onClose={() => setSessionSwitcherOpen(false)}
            onOpenThread={(threadId) => void openThreadById(threadId)}
            onOpenThreadShell={(threadId) => void openThreadById(threadId, true)}
            onOpenTmux={openTmux}
          />
        </Suspense>
      ) : null}

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
          setAsideView({ kind: "current" });
          setAsideTab("changes");
          // Let the review's history entry unwind before the drawer pushes its
          // own; otherwise the drawer can be dismissed by the review's back.
          window.setTimeout(openChanges, 0);
        }}
      />

      <Spotlight
        actions={actions}
        shortcut={["mod + K"]}
        query={paletteQuery}
        onQueryChange={setPaletteQuery}
        filter={(_query, actions) => actions}
        highlightQuery
        nothingFound={
          paletteQuery.trim().length >= 2 && search.isFetching
            ? "Searching…"
            : "Nothing here."
        }
        onSpotlightOpen={() => {
          setPaletteOpened(true);
          setPaletteQuery("");
        }}
        onSpotlightClose={() => {
          setPaletteOpened(false);
          setPaletteQuery("");
        }}
        searchProps={{
          placeholder: "Search threads, messages or run a command…",
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
        <Route path="/sessions" element={<Workspace />} />
        <Route path="/settings" element={<Workspace />} />
        <Route path="/p/:projectId" element={<Workspace />} />
        <Route path="/p/:projectId/t/:threadId" element={<Workspace />} />
        <Route
          path="/p/:projectId/t/:threadId/s/:subagentId"
          element={<Workspace />}
        />
        <Route path="*" element={<Navigate to="/inbox" replace />} />
      </Routes>
    </BrowserRouter>
  );
}
