/** Section label above a tree group (Pinned / Projects / Threads). */
import type { ComponentType, ReactNode } from "react";

export function SidebarGroupLabel({
  children,
  icon: Icon,
}: {
  children: ReactNode;
  icon: ComponentType<{ className?: string; size?: number }>;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "8px 8px 4px",
        fontSize: "var(--mantine-font-size-xs)",
        fontWeight: 500,
        textTransform: "uppercase",
        letterSpacing: "0.14em",
        color: "var(--mantine-color-dimmed)",
      }}
    >
      <Icon size={12} />
      {children}
    </div>
  );
}
