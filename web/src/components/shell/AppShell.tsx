import { useEffect, useState, type ReactNode } from "react";
import { UnstyledButton, useMantineTheme } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { useNavigate } from "react-router-dom";
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
    false,
  );
  const isOverlayDrawer = useMediaQuery(
    `(max-width: calc(${theme.breakpoints.lg} - 0.0625em))`,
    false,
  );
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [wasOverlayNav, setWasOverlayNav] = useState(isOverlayNav);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [drawerTab, setDrawerTab] = useState<DrawerTab>("changes");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const navigate = useNavigate();

  // Entering the overlay breakpoint hides the rail; leaving it shows the rail
  // again. Nothing here overrides the manual toggle inside one breakpoint.
  // Adjust during render (React's "derive state from props" pattern) instead
  // of a cascading setState effect.
  if (isOverlayNav !== wasOverlayNav) {
    setWasOverlayNav(isOverlayNav);
    setSidebarOpen(!isOverlayNav);
  }

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();
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
      if (modifier && !event.altKey && !event.shiftKey && key === "t") {
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
      if (modifier && !event.altKey && !event.shiftKey && key === "p") {
        event.preventDefault();
        setDrawerTab("files");
        setDrawerOpen(true);
        return;
      }
      if (modifier && event.shiftKey && !event.altKey && key === "m") {
        event.preventDefault();
        setDrawerTab("comments");
        setDrawerOpen(true);
        return;
      }
      if (modifier && event.altKey && key === "m") {
        event.preventDefault();
        setDrawerTab("comments");
        setDrawerOpen(true);
        return;
      }
      if (event.shiftKey && event.key === "Escape") {
        setDrawerOpen(false);
        setSidebarOpen((open) => (isOverlayNav ? false : open));
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isOverlayNav, navigate]);

  useEffect(() => {
    const onOpenComments = () => {
      setDrawerTab("comments");
      setDrawerOpen(true);
    };
    window.addEventListener("threadbase:open-comments", onOpenComments);
    return () =>
      window.removeEventListener("threadbase:open-comments", onOpenComments);
  }, []);

  const openDrawer = (tab: DrawerTab = "changes") => {
    setDrawerTab(tab);
    setDrawerOpen(true);
  };

  // The sidebar only overlays the content on narrow viewports. On desktop it is
  // a persistent column, so navigating must not collapse it.
  const closeSidebar = () => {
    if (isOverlayNav) {
      setSidebarOpen(false);
    }
  };

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
          onToggleDrawer={() => setDrawerOpen((open) => !open)}
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
            onClick={() => setSidebarOpen(false)}
            style={{
              position: "absolute",
              inset: 0,
              zIndex: 30,
              background: "rgba(0, 0, 0, 0.5)",
            }}
          />
        ) : null}

        <aside
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
                  transition: "transform 150ms ease",
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
                  transition: "width 150ms ease",
                  width: sidebarOpen ? 280 : 0,
                  overflow: "hidden",
                }),
          }}
        >
          {sidebar(closeSidebar, { overlay: isOverlayNav })}
        </aside>

        <main
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
            onClose={() => setDrawerOpen(false)}
            desktop={!isOverlayDrawer}
          />
        ) : null}
      </div>

      <div style={{ height: 30, flexShrink: 0 }}>
        <StatusBar
          {...statusBar}
          offline={statusBar.offline ?? false}
          onOpenChanges={() => openDrawer("changes")}
        />
      </div>

      <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} />
    </div>
  );
}
