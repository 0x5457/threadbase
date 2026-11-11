/**
 * Threadbase Design Tokens
 *
 * Single source of truth for the entire visual system.
 * All values are static constants consumed by the Mantine theme
 * and by CSS custom properties in index.css.
 *
 * Goal: zero inline pixel values anywhere in components.
 */

// ─── Colors ──────────────────────────────────────────────────────

export const colors = {
  /** Neutral slate for primary actions, headings, chrome. */
  primary: [
    "#f8f9fa",
    "#f1f3f5",
    "#e9ecef",
    "#dee2e6",
    "#ced4da",
    "#adb5bd",
    "#868e96",
    "#495057",
    "#343a40",
    "#16181d",
  ] as const,

  /** Semantic: success (green, but muted / neutralised). */
  success: [
    "#f0fdf4",
    "#dcfce7",
    "#bbf7d0",
    "#86efac",
    "#4ade80",
    "#22c55e",
    "#16a34a",
    "#15803d",
    "#166534",
    "#14532d",
  ] as const,

  /** Semantic: warning (amber). */
  warning: [
    "#fffbeb",
    "#fef3c7",
    "#fde68a",
    "#fcd34d",
    "#fbbf24",
    "#f59e0b",
    "#d97706",
    "#b45309",
    "#92400e",
    "#78350f",
  ] as const,

  /** Semantic: danger (red, but not garish). */
  danger: [
    "#fef2f2",
    "#fee2e2",
    "#fecaca",
    "#fca5a5",
    "#f87171",
    "#ef4444",
    "#dc2626",
    "#b91c1c",
    "#991b1b",
    "#7f1d1d",
  ] as const,

  /** Semantic: info (blue, reserved for links / external references only). */
  info: [
    "#eff6ff",
    "#dbeafe",
    "#bfdbfe",
    "#93c5fd",
    "#60a5fa",
    "#3b82f6",
    "#2563eb",
    "#1d4ed8",
    "#1e40af",
    "#1e3a8a",
  ] as const,
} as const;

// ─── Spacing ─────────────────────────────────────────────────────

export const spacing = {
  0: 0,
  1: 2,
  2: 4,
  3: 6,
  4: 8,
  5: 10,
  6: 12,
  7: 14,
  8: 16,
  9: 20,
  10: 24,
  11: 28,
  12: 32,
  13: 40,
  14: 48,
  15: 64,
  16: 80,
  17: 96,
  18: 128,
} as const;

// ─── Typography ──────────────────────────────────────────────────

export const fontSizes = {
  /** 11px — timestamps, captions, monospaced metadata. */
  xs: "11px",
  /** 12px — secondary text, badges, small buttons. */
  sm: "12px",
  /** 13px — body, labels, menu items. */
  md: "13px",
  /** 14px — emphasis body, table cells, larger buttons. */
  lg: "14px",
  /** 16px — small headings, section titles. */
  xl: "16px",
  /** 20px — page headings. */
  "2xl": "20px",
  /** 24px — hero / h1. */
  "3xl": "24px",
} as const;

export const lineHeights = {
  /** 1.25 — headings, tight UI text. */
  tight: 1.25,
  /** 1.4 — body, readable paragraphs. */
  normal: 1.4,
  /** 1.6 — long-form, relaxed. */
  relaxed: 1.6,
} as const;

export const fontWeights = {
  normal: 400,
  medium: 500,
  semibold: 600,
  bold: 700,
} as const;

// ─── Border Radius ───────────────────────────────────────────────

export const radii = {
  /** 0px — tables, data grids. */
  none: 0,
  /** 4px — small tags, inline pills. */
  sm: 4,
  /** 6px — buttons, inputs, small cards. */
  md: 6,
  /** 8px — cards, panels, popovers. */
  lg: 8,
  /** 12px — dialogs, modals, large cards. */
  xl: 12,
  /** 9999px — avatars, circular elements. */
  full: 9999,
} as const;

// ─── Shadows ─────────────────────────────────────────────────────

export const shadows = {
  /** Subtle elevation: dropdowns, tooltips. */
  sm: "0 1px 2px 0 rgb(0 0 0 / 0.05)",
  /** Default elevation: popovers, cards on hover. */
  md: "0 4px 6px -1px rgb(0 0 0 / 0.08), 0 2px 4px -2px rgb(0 0 0 / 0.04)",
  /** High elevation: modals, drawers. */
  lg: "0 10px 15px -3px rgb(0 0 0 / 0.08), 0 4px 6px -4px rgb(0 0 0 / 0.04)",
} as const;

// ─── Transitions ─────────────────────────────────────────────────

export const transitions = {
  /** Fast — hover states, opacity. */
  fast: "150ms cubic-bezier(0.4, 0, 0.2, 1)",
  /** Default — layout changes, colors. */
  default: "200ms cubic-bezier(0.4, 0, 0.2, 1)",
  /** Slow — modal/drawer entrances. */
  slow: "300ms cubic-bezier(0.4, 0, 0.2, 1)",
  /** Spring — playful micro-interactions. */
  spring: "400ms cubic-bezier(0.34, 1.56, 0.64, 1)",
} as const;

// ─── Breakpoints (px) ────────────────────────────────────────────

export const breakpoints = {
  xs: 480,
  sm: 768,
  md: 992,
  lg: 1200,
  xl: 1440,
} as const;

// ─── Layout ──────────────────────────────────────────────────────

export const layout = {
  /** TitleBar / StatusBar height. */
  barHeight: 44,
  /** Sidebar width when open. */
  sidebarWidth: 260,
  /** Right drawer width on desktop. */
  drawerWidth: 360,
  /** Page content max-width. */
  pageMaxWidth: 768,
  /** Global gutter. */
  gutter: 16,
  /** List row height. */
  rowHeight: 36,
  /** List row horizontal padding. */
  rowPaddingX: 12,
  /** Avatar sizes. */
  avatar: { xs: 16, sm: 20, md: 24, lg: 28, xl: 32 },
  /** Icon sizes (lucide). */
  icon: { xs: 12, sm: 14, md: 16, lg: 18, xl: 20 },
} as const;
