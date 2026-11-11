/** Tree row primitives: the roving-focus row and its expand chevron. */
import { ChevronRight } from "lucide-react";
import { UnstyledButton } from "@mantine/core";
import type { MouseEvent, ReactNode } from "react";
import { activateRow } from "./treeNav";
import { rowHover, rowTransition } from "./rowStyles";

export function TreeRow({
  level,
  depth,
  selected,
  expanded,
  onSelect,
  children,
}: {
  level: number;
  depth: number;
  selected: boolean;
  expanded?: boolean;
  onSelect: () => void;
  children: ReactNode;
}) {
  return (
    <UnstyledButton
      component="div"
      role="treeitem"
      aria-level={level}
      aria-selected={selected}
      aria-expanded={expanded}
      tabIndex={0}
      data-tree-row
      data-depth={depth}
      data-expanded={expanded}
      onClick={onSelect}
      onKeyDown={(event) => activateRow(event, onSelect)}
      h={{ base: 40, md: 34 }}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 4,
        borderRadius: "var(--mantine-radius-md)",
        paddingRight: 4,
        paddingLeft: 4 + depth * 16,
        fontSize: "var(--mantine-font-size-md)",
        cursor: "default",
        outline: "none",
        transition: rowTransition,
        backgroundColor: selected
          ? "var(--mantine-color-default-hover)"
          : undefined,
        color: selected
          ? "var(--mantine-color-text)"
          : "var(--mantine-color-dimmed)",
      }}
      styles={{ root: rowHover }}
    >
      {children}
    </UnstyledButton>
  );
}

export function RowChevron({
  expanded,
  onClick,
  label,
}: {
  expanded: boolean;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  label: string;
}) {
  return (
    <UnstyledButton
      type="button"
      aria-label={expanded ? `Collapse ${label}` : `Expand ${label}`}
      data-tree-chevron
      onClick={onClick}
      w={{ base: 28, md: 16 }}
      h={{ base: 28, md: 16 }}
      style={{
        display: "flex",
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "center",
        borderRadius: "var(--mantine-radius-md)",
        color: "var(--mantine-color-dimmed)",
      }}
      styles={{ root: rowHover }}
    >
      <ChevronRight
        size={12}
        aria-hidden
        style={{
          transform: expanded ? "rotate(90deg)" : "none",
          transition: "transform 120ms ease",
        }}
      />
    </UnstyledButton>
  );
}
