import { Box, Group, Text, UnstyledButton } from "@mantine/core";
import type { ReactNode } from "react";
import { layout } from "@/lib/design";

export interface ListRowProps {
  /** Leading icon, avatar, or visual marker. */
  icon?: ReactNode;
  /** Main content (title + optional subtitle). */
  children: ReactNode;
  /** Right-aligned meta content (timestamps, counts, badges, avatars). */
  meta?: ReactNode;
  /** Selected state. */
  selected?: boolean;
  /** Custom left padding in px (for tree indentation). */
  indent?: number;
  /** Whether to show a bottom border. */
  bordered?: boolean;
  /** onClick handler. */
  onClick?: () => void;
  /** href for anchor variant. */
  href?: string;
  /** Additional className. */
  className?: string;
  /** Data attribute for testing. */
  "data-testid"?: string;
}

const rowStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: layout.icon.xs,
  minHeight: layout.rowHeight,
  textDecoration: "none",
  color: "inherit",
  width: "100%",
};

/**
 * Unified list row used across Workspace, Project, Inbox, and Settings.
 *
 * Specifications (from design tokens + audit §3):
 * – height: 40px minimum (content may expand slightly)
 * – horizontal padding: 12px
 * – icon gap: 8px
 * – bottom border: 1px hairline
 * – hover: default-hover background
 * – selected: default-hover background
 * – transition: 150ms
 */
export function ListRow({
  icon,
  children,
  meta,
  selected = false,
  indent = 0,
  bordered = true,
  onClick,
  href,
  className,
  "data-testid": testId,
}: ListRowProps) {
  const pl = layout.rowPaddingX + indent;
  const borderStyle = bordered
    ? "1px solid var(--mantine-color-default-border)"
    : undefined;
  const bgStyle = selected
    ? "var(--mantine-color-default-hover)"
    : undefined;

  if (href) {
    return (
      <Box
        component="a"
        href={href}
        data-testid={testId}
        className={className}
        px={layout.rowPaddingX}
        py={6}
        pl={pl}
        style={{
          ...rowStyle,
          cursor: "pointer",
          borderBottom: borderStyle,
          background: bgStyle,
          transition:
            "background-color var(--tb-duration-fast) var(--tb-ease-standard)",
        }}
        onMouseEnter={(e: React.MouseEvent<HTMLElement>) => {
          if (!selected) {
            e.currentTarget.style.backgroundColor =
              "var(--mantine-color-default-hover)";
          }
        }}
        onMouseLeave={(e: React.MouseEvent<HTMLElement>) => {
          if (!selected) {
            e.currentTarget.style.backgroundColor = "transparent";
          }
        }}
      >
        <RowInner icon={icon} content={children} meta={meta} />
      </Box>
    );
  }

  if (onClick) {
    return (
      <UnstyledButton
        onClick={onClick}
        data-testid={testId}
        className={className}
        px={layout.rowPaddingX}
        py={6}
        pl={pl}
        style={{
          ...rowStyle,
          borderBottom: borderStyle,
          background: bgStyle,
          transition:
            "background-color var(--tb-duration-fast) var(--tb-ease-standard)",
        }}
        onMouseEnter={(e: React.MouseEvent<HTMLElement>) => {
          if (!selected) {
            e.currentTarget.style.backgroundColor =
              "var(--mantine-color-default-hover)";
          }
        }}
        onMouseLeave={(e: React.MouseEvent<HTMLElement>) => {
          if (!selected) {
            e.currentTarget.style.backgroundColor = "transparent";
          }
        }}
      >
        <RowInner icon={icon} content={children} meta={meta} />
      </UnstyledButton>
    );
  }

  return (
    <Box
      data-testid={testId}
      className={className}
      px={layout.rowPaddingX}
      py={6}
      pl={pl}
      style={{
        ...rowStyle,
        borderBottom: borderStyle,
        background: bgStyle,
      }}
    >
      <RowInner icon={icon} content={children} meta={meta} />
    </Box>
  );
}

function RowInner({
  icon,
  content,
  meta,
}: {
  icon?: ReactNode;
  content: ReactNode;
  meta?: ReactNode;
}) {
  return (
    <>
      {icon ? (
        <Box style={{ flexShrink: 0, display: "flex", alignItems: "center" }}>
          {icon}
        </Box>
      ) : null}
      <Box style={{ flex: 1, minWidth: 0, overflow: "hidden" }}>{content}</Box>
      {meta ? (
        <Group gap={8} wrap="nowrap" style={{ flexShrink: 0 }}>
          {meta}
        </Group>
      ) : null}
    </>
  );
}

/** Compact text stack for row title + subtitle. */
export function RowText({
  title,
  subtitle,
  titleSize = "sm",
  subtitleSize = "xs",
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  titleSize?: "xs" | "sm" | "md";
  subtitleSize?: "xs" | "sm";
}) {
  return (
    <Box
      style={{ display: "flex", flexDirection: "column", gap: 1, minWidth: 0 }}
    >
      <Text size={titleSize} fw={500} truncate="end">
        {title}
      </Text>
      {subtitle ? (
        <Text size={subtitleSize} c="dimmed" truncate="end">
          {subtitle}
        </Text>
      ) : null}
    </Box>
  );
}
