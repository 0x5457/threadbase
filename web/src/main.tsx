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
import { installVisualViewportSync } from "./lib/viewport";
import "./styles/base.css";
import "./styles/header.css";
import "./styles/sidebar.css";
import "./styles/thread.css";
import "./styles/approval.css";
import "./styles/agent-log.css";
import "./styles/composer.css";
import "./styles/panels.css";
import "./styles/terminal.css";
import "./styles/comments.css";
import "./styles/tree.css";
import "./styles/thoughts.css";
import "./styles/typography.css";
import "./styles/diff.css";
import "./styles/motion.css";
import "./styles/empty.css";
import "./styles/frame.css";
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
  // Zed's One Light/One Dark palettes drive Mantine as well, so toasts,
  // buttons and badges share the same green/yellow/red/blue semantics as the
  // rest of the UI. Shades 4/6 carry the accent for dark/light respectively.
  colors: {
    brand: [
      "#eef1fc",
      "#e2e6fb",
      "#c7d0f7",
      "#a9b6f2",
      "#74ade8",
      "#7b8fe9",
      "#5c78e2",
      "#4a63c9",
      "#3c50a4",
      "#2f3f82",
    ],
    gray: [
      "#fafafa",
      "#ebebec",
      "#dfdfe0",
      "#c9c9ca",
      "#a2a3a7",
      "#7e8086",
      "#58585a",
      "#414247",
      "#2f3034",
      "#242529",
    ],
    dark: [
      "#dce0e5",
      "#a9afbc",
      "#878a98",
      "#6b7078",
      "#585d66",
      "#4e535c",
      "#454a56",
      "#3b414d",
      "#2f343e",
      "#282c33",
    ],
    teal: [
      "#eaf4e8",
      "#dfeadb",
      "#c8dcc1",
      "#aad3a0",
      "#8ac07d",
      "#6faf5f",
      "#669f59",
      "#528147",
      "#3f6637",
      "#2c4a27",
    ],
    green: [
      "#eaf4e8",
      "#dfeadb",
      "#c8dcc1",
      "#aad3a0",
      "#8ac07d",
      "#6faf5f",
      "#669f59",
      "#528147",
      "#3f6637",
      "#2c4a27",
    ],
    red: [
      "#fdeceb",
      "#fbdfd9",
      "#f6c6bd",
      "#f0a49a",
      "#ea858b",
      "#e07276",
      "#d36151",
      "#b74e41",
      "#963d32",
      "#752e26",
    ],
    yellow: [
      "#fdf6e3",
      "#faf2e6",
      "#f4e7d1",
      "#ebd8a4",
      "#e5c87a",
      "#dcb94a",
      "#d3b020",
      "#b8941a",
      "#967718",
      "#725a13",
    ],
    blue: [
      "#eef1fc",
      "#e2e6fb",
      "#c7d0f7",
      "#a9b6f2",
      "#74ade8",
      "#7b8fe9",
      "#5c78e2",
      "#4a63c9",
      "#3c50a4",
      "#2f3f82",
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
          <App />
        </AuthGate>
        <NotificationHost />
      </QueryClientProvider>
    </MantineProvider>
  </StrictMode>,
);
