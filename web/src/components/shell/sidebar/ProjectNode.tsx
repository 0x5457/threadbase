/** One project row plus its threads and "new thread" affordance. */
import { Plus } from "lucide-react";
import { UnstyledButton } from "@mantine/core";
import type { ProjectDto } from "@/api/types/ProjectDto";
import { useState } from "react";
import type { SubagentDto } from "@/api/types/SubagentDto";
import type { ThreadDto } from "@/api/types/ThreadDto";
import { ThreadNode } from "./ThreadNode";
import { ProjectRow } from "./ProjectRow";
import { rowHover } from "./rowStyles";


export interface ProjectNodeProps {
  project: ProjectDto;
  rootThreads: ThreadDto[];
  childThreads: Map<string, ThreadDto[]>;
  subagentsByThread: Map<string, SubagentDto[]>;
  subagentChildren: Map<string, SubagentDto[]>;
  selectedProjectId?: string;
  selectedThreadId?: string;
  selectedSubagentId?: string;
  threadsPerGroup: number;
  collapsedProjects: Set<string>;
  collapsedThreads: Set<string>;
  onToggleProject: (projectId: string) => void;
  onToggleThread: (threadId: string) => void;
  onSelectProject: (projectId: string) => void;
  onSelectThread: (thread: ThreadDto) => void;
  onSelectSubagent: (subagent: SubagentDto) => void;
  onNewThread: (projectId: string) => void;
  onLocateProject?: (projectId: string) => void;
}

export function ProjectNode({
  project,
  rootThreads,
  childThreads,
  subagentsByThread,
  subagentChildren,
  selectedProjectId,
  selectedThreadId,
  selectedSubagentId,
  threadsPerGroup,
  collapsedProjects,
  collapsedThreads,
  onToggleProject,
  onToggleThread,
  onSelectProject,
  onSelectThread,
  onSelectSubagent,
  onNewThread,
  onLocateProject,
}: ProjectNodeProps) {
  const [showAll, setShowAll] = useState(false);
  const expanded = !collapsedProjects.has(project.id);
  const addThread = (
    <UnstyledButton
      type="button"
      onClick={(event) => {
        event.stopPropagation();
        onNewThread(project.id);
      }}
      h={{ base: 36, md: 28 }}
      style={{
        display: "flex",
        width: "100%",
        alignItems: "center",
        gap: 8,
        borderRadius: "var(--mantine-radius-md)",
        paddingLeft: 24,
        paddingRight: 8,
        textAlign: "left",
        fontSize: "var(--mantine-font-size-md)",
        color: "var(--mantine-color-dimmed)",
      }}
      styles={{ root: rowHover }}
    >
      <Plus size={12} aria-hidden />
      New Thread
    </UnstyledButton>
  );

  if (rootThreads.length === 0) {
    return (
      <div role="none">
        <ProjectRow
          project={project}
          expanded={expanded}
          selected={selectedProjectId === project.id && !selectedThreadId}
          onToggle={() => onToggleProject(project.id)}
          onSelect={() => onSelectProject(project.id)}
          onLocate={() => onLocateProject?.(project.id)}
        />
        {expanded ? (
          <div
            style={{
              position: "relative",
              marginLeft: 12,
              borderLeft: "1px solid var(--mantine-color-default-border)",
              paddingLeft: 4,
            }}
          >
            <p
              style={{
                margin: 0,
                padding: "4px 8px",
                fontSize: "var(--mantine-font-size-md)",
                color: "var(--mantine-color-dimmed)",
              }}
            >
              No threads yet
            </p>
            {addThread}
          </div>
        ) : null}
      </div>
    );
  }

  const limit = showAll ? Number.POSITIVE_INFINITY : threadsPerGroup;
  const visible = rootThreads.slice(0, limit);
  const remaining = rootThreads.length - visible.length;

  return (
    <div role="none">
      <ProjectRow
        project={project}
        expanded={expanded}
        selected={selectedProjectId === project.id && !selectedThreadId}
        onToggle={() => onToggleProject(project.id)}
        onSelect={() => onSelectProject(project.id)}
        onLocate={() => onLocateProject?.(project.id)}
      />
      {expanded ? (
        <div
          style={{
            position: "relative",
            marginLeft: 12,
            borderLeft: "1px solid var(--mantine-color-default-border)",
            paddingLeft: 4,
          }}
        >
          {addThread}
          {visible.map((thread) => (
            <ThreadNode
              key={thread.id}
              thread={thread}
              depth={1}
              projectName={project.name}
              showProjectName={false}
              childThreads={childThreads}
              subagentsByThread={subagentsByThread}
              subagentChildren={subagentChildren}
              selectedThreadId={selectedThreadId}
              selectedSubagentId={selectedSubagentId}
              threadsPerGroup={threadsPerGroup}
              collapsedThreads={collapsedThreads}
              onToggleThread={onToggleThread}
              onSelectThread={onSelectThread}
              onSelectSubagent={onSelectSubagent}
            />
          ))}
          {remaining > 0 ? (
            <UnstyledButton
              type="button"
              onClick={() => setShowAll(true)}
              h={{ base: 36, md: 28 }}
              style={{
                display: "flex",
                width: "100%",
                alignItems: "center",
                borderRadius: "var(--mantine-radius-md)",
                paddingLeft: 24,
                paddingRight: 8,
                textAlign: "left",
                fontSize: "var(--mantine-font-size-md)",
                color: "var(--mantine-color-dimmed)",
              }}
              styles={{ root: rowHover }}
            >
              Show {remaining} more
            </UnstyledButton>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
