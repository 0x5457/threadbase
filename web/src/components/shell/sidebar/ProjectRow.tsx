/** The project row itself: name, thread count, status and chevron. */
import { FolderGit2 } from "lucide-react";
import { Badge, UnstyledButton } from "@mantine/core";
import type { ProjectDto } from "@/api/types/ProjectDto";
import { RowChevron, TreeRow } from "./TreeRow";


export function ProjectRow({
  project,
  expanded,
  selected,
  onToggle,
  onSelect,
  onLocate,
}: {
  project: ProjectDto;
  expanded: boolean;
  selected: boolean;
  onToggle: () => void;
  onSelect: () => void;
  onLocate?: () => void;
}) {
  return (
    <TreeRow
      level={1}
      depth={0}
      selected={selected}
      expanded={expanded}
      onSelect={onSelect}
    >
      <RowChevron
        expanded={expanded}
        label={project.name}
        onClick={(event) => {
          event.stopPropagation();
          onToggle();
        }}
      />
      <FolderGit2
        size={14}
        aria-hidden
        style={{ flexShrink: 0, color: "var(--mantine-color-dimmed)" }}
      />
      <span
        style={{
          minWidth: 0,
          flex: 1,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          fontWeight: 500,
        }}
      >
        {project.name}
      </span>
      {project.waitingApprovals > 0 ? (
        <Badge color="yellow" variant="light" size="xs">
          {project.waitingApprovals}
        </Badge>
      ) : null}
      {!project.available ? (
        <UnstyledButton
          type="button"
          title="Project path is unavailable"
          onClick={(event) => {
            event.stopPropagation();
            onLocate?.();
          }}
          style={{
            flexShrink: 0,
            borderRadius: "var(--mantine-radius-md)",
            padding: "0 4px",
            fontSize: "var(--mantine-font-size-md)",
            color: "var(--mantine-color-yellow-6)",
          }}
          styles={{
            root: {
              ":hover": {
                backgroundColor: "var(--mantine-color-default-hover)",
                color: "var(--mantine-color-yellow-7)",
              },
            },
          }}
        >
          Locate…
        </UnstyledButton>
      ) : null}
      {project.activeThreads > 0 ? (
        <span
          style={{
            fontSize: "var(--mantine-font-size-md)",
            fontVariantNumeric: "tabular-nums",
            color: "var(--mantine-color-dimmed)",
          }}
        >
          {project.activeThreads}
        </span>
      ) : null}
    </TreeRow>
  );
}
