/** Shared hover/transition styling for every sidebar row button. */
export const rowTransition = "background-color 120ms ease, color 120ms ease";

export const rowHover = {
  ":hover": {
    backgroundColor: "var(--mantine-color-default-hover)",
    color: "var(--mantine-color-text)",
  },
} as const;
