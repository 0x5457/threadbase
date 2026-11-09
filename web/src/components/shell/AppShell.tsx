import { useEffect, useState, type ReactNode } from "react";
import { useMediaQuery } from "@/lib/hooks";
import { useNavigate } from "react-router-dom";
import { cn } from "@/lib/utils";
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
  "open" | "activeTab" | "onTabChange" | "onClose" | "overlay"
>;

export interface AppShellProps {
  sidebar: (closeSidebar: () => void) => ReactNode;
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
  const isMobileSidebar = useMediaQuery("(max-width: 900px)");
  const isOverlayDrawer = useMediaQuery("(max-width: 1100px)");
  const [sidebarOpen, setSidebarOpen] = useState(
    () => !window.matchMedia("(max-width: 900px)").matches,
  );
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [drawerTab, setDrawerTab] = useState<DrawerTab>("changes");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const navigate = useNavigate();

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
        setSidebarOpen((open) => (isMobileSidebar ? false : open));
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isMobileSidebar, navigate]);

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

  const closeSidebar = () => setSidebarOpen(false);

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden bg-canvas">
      <TitleBar
        {...titleBar}
        sidebarOpen={sidebarOpen}
        drawerOpen={drawerOpen}
        onToggleSidebar={() => setSidebarOpen((open) => !open)}
        onOpenSearch={() => setPaletteOpen(true)}
        onOpenApprovals={() => openDrawer("approvals")}
        onToggleDrawer={() => setDrawerOpen((open) => !open)}
      />

      <div className="relative flex min-h-0 flex-1 overflow-hidden">
        {isMobileSidebar && sidebarOpen ? (
          <button
            type="button"
            aria-label="Close sidebar"
            onClick={() => setSidebarOpen(false)}
            className="absolute inset-0 z-30 bg-black/50"
          />
        ) : null}

        <aside
          aria-label="Workspace sidebar"
          className={cn(
            "h-full bg-panel",
            isMobileSidebar
              ? cn(
                  "absolute inset-y-0 left-0 z-40 w-[280px] border-r border-border shadow-2xl transition-transform duration-150",
                  !sidebarOpen && "-translate-x-full pointer-events-none",
                )
              : cn(
                  "shrink-0 border-r border-border transition-[width] duration-150",
                  sidebarOpen ? "w-[280px]" : "w-0 overflow-hidden border-r-0",
                ),
          )}
          aria-hidden={!sidebarOpen}
          inert={!sidebarOpen ? true : undefined}
        >
          {sidebar(closeSidebar)}
        </aside>

        <main className="relative min-h-0 min-w-0 flex-1 overflow-hidden">
          {children}
        </main>

        {isOverlayDrawer && drawerOpen ? (
          <button
            type="button"
            aria-label="Close right drawer"
            onClick={() => setDrawerOpen(false)}
            className="absolute inset-0 z-30 bg-black/40"
          />
        ) : null}

        {drawerOpen ? (
          <RightDrawer
            {...drawer}
            open={drawerOpen}
            activeTab={drawerTab}
            onTabChange={setDrawerTab}
            onClose={() => setDrawerOpen(false)}
            overlay={isOverlayDrawer}
          />
        ) : null}
      </div>

      <StatusBar
        {...statusBar}
        offline={statusBar.offline ?? false}
        onOpenChanges={() => openDrawer("changes")}
      />

      <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} />
    </div>
  );
}
