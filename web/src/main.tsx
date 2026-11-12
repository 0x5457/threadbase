import "@fontsource-variable/geist";
import "@mantine/core/styles.css";
import "@mantine/notifications/styles.css";
import "@mantine/spotlight/styles.css";
import {
  ActionIcon,
  Badge,
  Button,
  Card,
  MantineProvider,
  NavLink,
  ScrollArea,
  Tooltip,
  createTheme,
  localStorageColorSchemeManager,
} from "@mantine/core";
import { Notifications } from "@mantine/notifications";
import { useMediaQuery } from "@mantine/hooks";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { AuthGate } from "./components/AuthGate";
import { InviteAcceptPage } from "./components/InviteAcceptPage";
import { installVisualViewportSync } from "./lib/viewport";
import "./styles/base.css";
import "./styles/header.css";
import "./styles/sidebar.css";
import "./styles/thread.css";
import "./styles/approval.css";
import "./styles/agent-log.css";
import "./styles/composer.css";
import "./styles/panels.css";
import "./styles/comments.css";
import "./styles/tree.css";
import "./styles/thoughts.css";
import "./styles/typography.css";
import "./styles/diff.css";
import "./styles/motion.css";
import "./styles/empty.css";
import "./styles/responsive.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { refetchOnWindowFocus: false, retry: 1, staleTime: 5_000 },
  },
});

const theme = createTheme({
  fontFamily:
    '"Geist Variable", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  fontFamilyMonospace:
    'ui-monospace, "SFMono-Regular", "JetBrains Mono", Menlo, monospace',
  primaryColor: "brand",
  primaryShade: { light: 6, dark: 4 },
  defaultRadius: "sm",
  colors: {
    brand: [
      "#fff4f0",
      "#ffe4dc",
      "#ffc9bb",
      "#fca78f",
      "#f28362",
      "#e96545",
      "#d85540",
      "#b9432f",
      "#973524",
      "#7a2a1c",
    ],
    gray: [
      "#fbfbf8",
      "#f5f5f1",
      "#efefea",
      "#e7e8e5",
      "#dcddd7",
      "#ced1cb",
      "#b8bbb4",
      "#9a9c97",
      "#6f7276",
      "#242529",
    ],
    dark: [
      "#c1c5cc",
      "#acb2be",
      "#7e8086",
      "#3a3f46",
      "#30343a",
      "#25292e",
      "#1f2227",
      "#191c21",
      "#171a1e",
      "#131519",
    ],
  },
  // Dense-workspace scale, one step up from the previous one. The old values
  // (10/11/13/14/16) put body copy at 13px and metadata at 10px, which read as
  // too small on a 1440-wide layout.
  fontSizes: {
    xs: "11.5px",
    sm: "13px",
    md: "14.5px",
    lg: "16px",
    xl: "19px",
  },
  lineHeights: {
    xs: "1.4",
    sm: "1.45",
    md: "1.55",
    lg: "1.5",
    xl: "1.3",
  },
  components: {
    Button: Button.extend({
      defaultProps: { size: "xs", radius: "sm", fw: 400 },
    }),
    ActionIcon: ActionIcon.extend({
      defaultProps: { size: 24, variant: "subtle", color: "gray", radius: "sm" },
    }),
    Card: Card.extend({
      defaultProps: { withBorder: true, radius: "sm", padding: "xs" },
    }),
    Badge: Badge.extend({
      defaultProps: { size: "xs", radius: "sm" },
    }),
    Tooltip: Tooltip.extend({
      defaultProps: { openDelay: 300, withArrow: true },
    }),
    NavLink: NavLink.extend({
      defaultProps: { variant: "subtle", color: "gray" },
    }),
    ScrollArea: ScrollArea.extend({
      defaultProps: { type: "hover", scrollbarSize: 6 },
    }),
  },
});

const colorSchemeManager = localStorageColorSchemeManager({
  key: "threadbase.theme",
});

function inviteToken() {
  return /^\/invite\/([^/]+)\/?$/.exec(window.location.pathname)?.[1] ?? null;
}

installVisualViewportSync();

/** Toasts sit over the composer on phones; keep them at the top instead. */
function NotificationHost() {
  const phone = useMediaQuery("(max-width: 47.99em)");
  return <Notifications position={phone ? "top-center" : "bottom-right"} />;
}

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root");

createRoot(root).render(
  <StrictMode>
    <MantineProvider
      theme={theme}
      defaultColorScheme="light"
      colorSchemeManager={colorSchemeManager}
    >
      <QueryClientProvider client={queryClient}>
        <AuthGate>
          {inviteToken() ? (
            <InviteAcceptPage token={inviteToken() as string} />
          ) : (
            <App />
          )}
        </AuthGate>
        <NotificationHost />
      </QueryClientProvider>
    </MantineProvider>
  </StrictMode>,
);
