import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  Group,
  Kbd,
  Modal,
  Stack,
  Text,
  UnstyledButton,
  useMantineTheme,
} from "@mantine/core";
import { useNavigate } from "react-router-dom";
import { useMediaQuery } from "@/lib/hooks";
import { modKeyLabel } from "@/lib/platform";
import {
  isInsideOverlay,
  isTypingTarget,
  modKeyPressed,
  SHORTCUTS,
} from "@/lib/shortcuts";
import { CommandPalette } from "./CommandPalette";
import { RightDrawer, type DrawerTab, type RightDrawerProps } from "./RightDrawer";
import { StatusBar, type StatusBarProps } from "./StatusBar";
import { TitleBar, type TitleBarProps } from "./TitleBar";

type TitleBarSlot = Omit<
  TitleBarProps,
  | "onToggleSidebar"
  | "onOpenSearch"
  | "onOpenApprovals"
  | "onToggleDrawer"
  | "sidebarOpen"
  | "drawerOpen"
>;

type StatusBarSlot = Omit<StatusBarProps, "onOpenChanges">;

type DrawerSlot = Omit<
  RightDrawerProps,
  "open" | "activeTab" | "onTabChange" | "onClose" | "desktop" | "phone"
>;

export interface AppShellProps {
  sidebar: (
    closeSidebar: () => void,
    options: { overlay: boolean },
  ) => ReactNode;
  titleBar: TitleBarSlot;
  statusBar: StatusBarSlot;
  drawer: DrawerSlot;
  children: ReactNode;
}

export function AppShell({
  sidebar,
  titleBar,
  statusBar,
  drawer,
  children,
}: AppShellProps) {
  const theme = useMantineTheme();
  // Breakpoints come from the Mantine theme (md = 992px, lg = 1200px) instead
  // of ad-hoc pixel values: 768–1024 keeps the rail, the drawer overlays;
  // 1280+ shows both columns.
  const isOverlayNav = useMediaQuery(
    `(max-width: calc(${theme.breakpoints.md} - 0.0625em))`,
  );
  const isOverlayDrawer = useMediaQuery(
    `(max-width: calc(${theme.breakpoints.lg} - 0.0625em))`,
  );
  // Phones get a full-width drawer sheet instead of a 420px side panel.
  const isPhone = useMediaQuery(
    `(max-width: calc(${theme.breakpoints.sm} - 0.0625em))`,
  );
  // Start closed on phones/tablets, open on desktop, so the first paint is
  // already the right layout instead of toggling after mount.
  const [sidebarOpen, setSidebarOpen] = useState(!isOverlayNav);
  const [wasOverlayNav, setWasOverlayNav] = useState(isOverlayNav);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [drawerTab, setDrawerTab] = useState<DrawerTab>("changes");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const navigate = useNavigate();
  const sidebarRef = useRef<HTMLElement>(null);
  const sidebarReturnFocus = useRef<HTMLElement | null>(null);
  const drawerReturnFocus = useRef<HTMLElement | null>(null);
  const openDrawer = useCallback(
    (tab: DrawerTab = "changes") => {
      if (!drawerOpen) {
        drawerReturnFocus.current =
          document.activeElement as HTMLElement | null;
      }
      setDrawerTab(tab);
      setDrawerOpen(true);
    },
    [drawerOpen],
  );

  const closeSidebar = useCallback(() => {
    if (!isOverlayNav) {
      return;
    }
    const toggle = document.querySelector<HTMLElement>(
      'header [aria-label="Hide sidebar"]',
    );
    // Focus before the sheet becomes inert: Chrome moves focus to <body> when
    // the focused subtree turns inert, and a later rAF would race that move.
    toggle?.focus();
    setSidebarOpen(false);
  }, [isOverlayNav]);

  const closeDrawer = useCallback(() => {
    setDrawerOpen(false);
    requestAnimationFrame(() => {
      drawerReturnFocus.current?.focus?.();
      drawerReturnFocus.current = null;
    });
  }, []);


  // Entering the overlay breakpoint hides the rail; leaving it shows the rail
  // again. Nothing here overrides the manual toggle inside one breakpoint.
  // Adjust during render (React's "derive state from props" pattern) instead
  // of a cascading setState effect.
  if (isOverlayNav !== wasOverlayNav) {
    setWasOverlayNav(isOverlayNav);
    setSidebarOpen(!isOverlayNav);
  }

  // Overlay sidebar is a sheet: move focus into it on open and give it back
  // to the toggle on close. Desktop keeps the persistent rail focus-neutral.
  useEffect(() => {
    if (!isOverlayNav) {
      return;
    }
    if (sidebarOpen) {
      sidebarReturnFocus.current = document.activeElement as HTMLElement | null;
      const frame = requestAnimationFrame(() => {
        const target =
          sidebarRef.current?.querySelector<HTMLElement>("[data-tree-row]") ??
          sidebarRef.current?.querySelector<HTMLElement>("button") ??
          null;
        target?.focus();
      });
      return () => cancelAnimationFrame(frame);
    }
    const previous = sidebarReturnFocus.current;
    const toggle =
      document.querySelector<HTMLElement>(
        'header [aria-label="Show sidebar"]',
      ) ?? null;
    const target =
      previous?.isConnected && !sidebarRef.current?.contains(previous)
        ? previous
        : toggle;
    target?.focus();
    sidebarReturnFocus.current = null;
  }, [isOverlayNav, sidebarOpen]);

  // Escape must win over tooltips (they sit above the sheet and would eat the
  // first press). Capture phase also keeps the sheet closing in one keystroke.
  useEffect(() => {
    if (!isOverlayNav || !sidebarOpen) {
      return;
    }
    const onEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.shiftKey || drawerOpen) {
        return;
      }
      const target = event.target as Element | null;
      if (
        target?.closest('[role="dialog"], [role="menu"], [role="listbox"]')
      ) {
        return;
      }
      event.preventDefault();
      closeSidebar();
    };
    window.addEventListener("keydown", onEscape, true);
    return () => window.removeEventListener("keydown", onEscape, true);
  }, [isOverlayNav, sidebarOpen, drawerOpen, closeSidebar]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = modKeyPressed(event);
      const key = event.key.toLowerCase();
      // Typing and open overlays own their keys. Search is the one global
      // shortcut that is still expected while editing; Escape is local.
      if (
        event.defaultPrevented ||
        ((isTypingTarget(event.target) || isInsideOverlay(event.target)) &&
          key !== "escape" &&
          !(modifier && !event.altKey && !event.shiftKey && key === "k"))
      ) {
        return;
      }
      if (!modifier && !event.altKey && key === "?") {
        event.preventDefault();
        setHelpOpen(true);
        return;
      }
      if (modifier && !event.altKey && !event.shiftKey && key === "k") {
        event.preventDefault();
        setPaletteOpen((open) => !open);
        return;
      }
      if (modifier && event.shiftKey && !event.altKey && key === "p") {
        event.preventDefault();
        setPaletteOpen(true);
        return;
      }
      if (modifier && event.shiftKey && !event.altKey && key === "j") {
        event.preventDefault();
        navigate("/inbox");
        return;
      }
      if (event.altKey && event.shiftKey && key === "d") {
        event.preventDefault();
        setDrawerTab("changes");
        setDrawerOpen(true);
        return;
      }
      if (event.altKey && event.shiftKey && !modifier && key === "f") {
        event.preventDefault();
        openDrawer("files");
        return;
      }
      if (modifier && event.shiftKey && !event.altKey && key === "m") {
        event.preventDefault();
        openDrawer("comments");
        return;
      }
      if (event.shiftKey && event.key === "Escape") {
        if (drawerOpen) {
          closeDrawer();
        }
        setSidebarOpen((open) => (isOverlayNav ? false : open));
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    isOverlayNav,
    navigate,
    openDrawer,
    closeDrawer,
    closeSidebar,
    drawerOpen,
    sidebarOpen,
  ]);

  useEffect(() => {
    const onOpenComments = () => {
      drawerReturnFocus.current = document.activeElement as HTMLElement | null;
      setDrawerTab("comments");
      setDrawerOpen(true);
    };
    window.addEventListener("threadbase:open-comments", onOpenComments);
    return () =>
      window.removeEventListener("threadbase:open-comments", onOpenComments);
  }, []);


  // The sidebar only overlays the content on narrow viewports. On desktop it is
  // a persistent column, so navigating must not collapse it.

  return (
    <div
      style={{
        display: "flex",
        height: "100%",
        minHeight: 0,
        flexDirection: "column",
        overflow: "hidden",
        background: "var(--mantine-color-body)",
      }}
    >
      {/* TitleBar/StatusBar fill their slot height (h="100%"). */}
      <div style={{ height: 44, flexShrink: 0 }}>
        <TitleBar
          {...titleBar}
        sidebarOpen={sidebarOpen}
        drawerOpen={drawerOpen}
        onToggleSidebar={() => setSidebarOpen((open) => !open)}
        onOpenSearch={() => setPaletteOpen(true)}
        onOpenApprovals={() => openDrawer("approvals")}
          onToggleDrawer={() =>
            drawerOpen ? closeDrawer() : openDrawer(drawerTab)
          }
        />
      </div>

      <div
        style={{
          position: "relative",
          display: "flex",
          minHeight: 0,
          flex: 1,
          overflow: "hidden",
        }}
      >
        {isOverlayNav && sidebarOpen ? (
          <UnstyledButton
            aria-label="Close sidebar"
            onClick={closeSidebar}
            style={{
              position: "absolute",
              inset: 0,
              zIndex: 30,
              background: "rgba(0, 0, 0, 0.5)",
            }}
          />
        ) : null}

        <aside
          ref={sidebarRef}
          aria-label="Workspace sidebar"
          aria-hidden={!sidebarOpen}
          inert={!sidebarOpen ? true : undefined}
          style={{
            height: "100%",
            background: "var(--mantine-color-body)",
            ...(isOverlayNav
              ? {
                  position: "absolute",
                  top: 0,
                  bottom: 0,
                  left: 0,
                  zIndex: 40,
                  width: 280,
                  borderRight:
                    "1px solid var(--mantine-color-default-border)",
                  boxShadow: "var(--mantine-shadow-xl)",
                  transition:
                    "transform var(--tb-duration-fast) var(--tb-ease-standard)",
                  transform: sidebarOpen
                    ? "translateX(0)"
                    : "translateX(-100%)",
                  pointerEvents: sidebarOpen ? undefined : "none",
                }
              : {
                  flexShrink: 0,
                  borderRight: sidebarOpen
                    ? "1px solid var(--mantine-color-default-border)"
                    : "none",
                  transition: "width var(--tb-duration-fast) var(--tb-ease-standard)",
                  width: sidebarOpen ? 280 : 0,
                  overflow: "hidden",
                }),
          }}
        >
          {sidebar(closeSidebar, { overlay: isOverlayNav })}
        </aside>

        <main
          inert={isOverlayNav && sidebarOpen ? true : undefined}
          style={{
            position: "relative",
            minHeight: 0,
            minWidth: 0,
            flex: 1,
            overflow: "hidden",
          }}
        >
          {children}
        </main>

        {drawerOpen ? (
          <RightDrawer
            {...drawer}
            open={drawerOpen}
            activeTab={drawerTab}
            onTabChange={setDrawerTab}
            onClose={closeDrawer}
            desktop={!isOverlayDrawer}
            phone={isPhone}
          />
        ) : null}
      </div>

      <div style={{ height: 40, flexShrink: 0 }}>
        <StatusBar
          {...statusBar}
          offline={statusBar.offline ?? false}
          onOpenChanges={() => openDrawer("changes")}
        />
      </div>

      <Modal
        opened={helpOpen}
        onClose={() => setHelpOpen(false)}
        title="Keyboard shortcuts"
        size="md"
      >
        <Stack gap={6}>
          {SHORTCUTS.map((shortcut) => (
            <Group key={shortcut.keys} justify="space-between" wrap="nowrap">
              <Text size="sm">{shortcut.label}</Text>
              <Group gap={4} wrap="nowrap">
                {shortcut.keys.split("+").map((part) => (
                  <Kbd key={`${shortcut.keys}-${part}`}>
                    {part === "mod" ? modKeyLabel() : part}
                  </Kbd>
                ))}
              </Group>
            </Group>
          ))}
        </Stack>
      </Modal>

      <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} />
    </div>
  );
}
