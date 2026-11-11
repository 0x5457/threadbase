import {
  MantineProvider,
  createTheme,
  localStorageColorSchemeManager,
} from "@mantine/core";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RouterProvider } from "react-router-dom";
import router from "./App";
import { AppNotifications } from "./components/app";
import { ThemeBridge, THEME_STORAGE_KEY } from "./lib/theme";
import { installVisualViewportSync } from "./lib/viewport";
import {
  breakpoints,
  colors,
  fontSizes,
  fontWeights,
  lineHeights,
  radii,
  shadows,
  spacing,
} from "./lib/design";
import "./index.css";

// Keep the shell above the mobile keyboard (see lib/viewport.ts).
installVisualViewportSync();

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
      staleTime: 5_000,
    },
  },
});

const theme = createTheme({
  // ── Font ───────────────────────────────────────────────────────
  fontFamily:
    '"Geist Variable", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  fontFamilyMonospace:
    '"GeistMono Variable", "SF Mono", ui-monospace, monospace',

  // ── Colors ─────────────────────────────────────────────────────
  primaryColor: "primary",
  primaryShade: 7,
  colors: {
    primary: colors.primary,
    success: colors.success,
    warning: colors.warning,
    danger: colors.danger,
    info: colors.info,
  },

  // ── Radius ─────────────────────────────────────────────────────
  defaultRadius: "md",
  radius: {
    xs: `${radii.sm}px`,
    sm: `${radii.md}px`,
    md: `${radii.lg}px`,
    lg: `${radii.xl}px`,
    xl: "16px",
  },

  // ── Spacing ────────────────────────────────────────────────────
  spacing: {
    xs: `${spacing[2]}px`,
    sm: `${spacing[4]}px`,
    md: `${spacing[8]}px`,
    lg: `${spacing[10]}px`,
    xl: `${spacing[12]}px`,
    "2xl": `${spacing[14]}px`,
    "3xl": `${spacing[15]}px`,
  },

  // ── Shadows ────────────────────────────────────────────────────
  shadows: {
    xs: shadows.sm,
    sm: shadows.md,
    md: shadows.lg,
    lg: "0 20px 25px -5px rgb(0 0 0 / 0.08), 0 8px 10px -6px rgb(0 0 0 / 0.04)",
    xl: "0 25px 50px -12px rgb(0 0 0 / 0.2)",
  },

  // ── Breakpoints ────────────────────────────────────────────────
  breakpoints: {
    xs: `${breakpoints.xs}px`,
    sm: `${breakpoints.sm}px`,
    md: `${breakpoints.md}px`,
    lg: `${breakpoints.lg}px`,
    xl: `${breakpoints.xl}px`,
  },

  // ── Typography ─────────────────────────────────────────────────
  fontSizes,
  lineHeights: {
    xs: String(lineHeights.tight),
    sm: String(lineHeights.normal),
    md: String(lineHeights.relaxed),
  },
  headings: {
    fontWeight: String(fontWeights.semibold),
    sizes: {
      h1: {
        fontSize: fontSizes["3xl"],
        lineHeight: String(lineHeights.tight),
        fontWeight: String(fontWeights.semibold),
      },
      h2: {
        fontSize: fontSizes["2xl"],
        lineHeight: String(lineHeights.tight),
        fontWeight: String(fontWeights.semibold),
      },
      h3: {
        fontSize: fontSizes.xl,
        lineHeight: String(lineHeights.tight),
        fontWeight: String(fontWeights.semibold),
      },
      h4: {
        fontSize: fontSizes.lg,
        lineHeight: String(lineHeights.tight),
        fontWeight: String(fontWeights.medium),
      },
    },
  },

  // ── Component defaults ─────────────────────────────────────────
  // Every Mantine component used in the app gets sensible defaults
  // so routes never need to repeat the same props.
  components: {
    Container: {
      defaultProps: { size: "md" },
    },
    Paper: {
      defaultProps: { radius: "md", p: "md" },
    },
    Card: {
      defaultProps: { radius: "md", p: "md", withBorder: true },
    },
    Button: {
      defaultProps: { size: "sm", radius: "md" },
      styles: {
        root: {
          fontWeight: fontWeights.medium,
          letterSpacing: "-0.01em",
        },
      },
    },
    ActionIcon: {
      defaultProps: { size: "md", variant: "subtle", radius: "md" },
    },
    Badge: {
      defaultProps: { size: "sm", radius: "sm", variant: "light" },
    },
    Input: {
      defaultProps: { size: "sm", radius: "md" },
      styles: {
        input: {
          fontSize: fontSizes.md,
        },
      },
    },
    TextInput: {
      defaultProps: { size: "sm", radius: "md" },
    },
    Textarea: {
      defaultProps: { size: "sm", radius: "md" },
    },
    Select: {
      defaultProps: { size: "sm", radius: "md" },
    },
    Menu: {
      defaultProps: { radius: "md", shadow: "md" },
    },
    Popover: {
      defaultProps: { radius: "md", shadow: "md" },
    },
    Modal: {
      defaultProps: { radius: "lg", shadow: "lg", padding: "xl" },
    },
    Drawer: {
      defaultProps: { radius: "lg", shadow: "lg", padding: "md" },
    },
    Tooltip: {
      defaultProps: { openDelay: 300, radius: "sm", withArrow: true },
    },
    Alert: {
      defaultProps: { radius: "md" },
    },
    Skeleton: {
      defaultProps: { radius: "md" },
    },
    Avatar: {
      defaultProps: { radius: "xl" },
    },
    Tabs: {
      defaultProps: { radius: "md" },
    },
    ScrollArea: {
      defaultProps: { scrollbarSize: 6 },
    },
  },

  // ── Other ──────────────────────────────────────────────────────
  autoContrast: true,
  cursorType: "pointer",
  // Honours prefers-reduced-motion: tests and users with the OS setting get
  // instant transitions instead of animated ones.
  respectReducedMotion: true,
});

const colorSchemeManager = localStorageColorSchemeManager({
  key: THEME_STORAGE_KEY,
});

const root = document.getElementById("root");
if (!root) {
  throw new Error("Missing #root element");
}

createRoot(root).render(
  <StrictMode>
    <MantineProvider
      colorSchemeManager={colorSchemeManager}
      defaultColorScheme="dark"
      theme={theme}
    >
      <ThemeBridge />
      <AppNotifications />
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </MantineProvider>
  </StrictMode>,
);
