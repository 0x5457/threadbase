/** Brand row: product name, new-project action and the overlay close button. */
import { FolderPlus, X } from "lucide-react";
import { ActionIcon, Tooltip } from "@mantine/core";
import { modKeyLabel } from "@/lib/platform";

export interface SidebarHeaderProps {
  onNewProject: () => void;
  onClose?: () => void;
}

export function SidebarHeader({ onNewProject, onClose }: SidebarHeaderProps) {
  return (
    <div
      style={{
        display: "flex",
        height: 44,
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "space-between",
        padding: "0 12px",
      }}
    >
      <span
        style={{
          fontSize: "var(--mantine-font-size-md)",
          fontWeight: 600,
          letterSpacing: "-0.01em",
          color: "var(--mantine-color-text)",
        }}
      >
        Threadbase
      </span>
      <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
        <Tooltip label={`New project (${modKeyLabel()}⇧A)`}>
          <ActionIcon
            variant="default"
            size="lg"
            aria-label="New project"
            onClick={onNewProject}
          >
            <FolderPlus size={16} />
          </ActionIcon>
        </Tooltip>
        {onClose ? (
          <ActionIcon
            variant="subtle"
            color="gray"
            size="lg"
            aria-label="Close sidebar"
            onClick={onClose}
          >
            <X size={16} />
          </ActionIcon>
        ) : null}
      </div>
    </div>
  );
}
