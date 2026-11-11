/** Shared hover/transition styling for every sidebar row button. */
export const rowTransition =
  "background-color var(--tb-duration-fast) var(--tb-ease-standard), color var(--tb-duration-fast) var(--tb-ease-standard)";

export const rowHover = {
  ":hover": {
    backgroundColor: "var(--mantine-color-default-hover)",
    color: "var(--mantine-color-text)",
  },
} as const;
