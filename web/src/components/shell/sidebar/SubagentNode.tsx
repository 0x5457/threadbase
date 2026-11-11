/** One subagent row plus its nested children. */
import { Bot } from "lucide-react";
import type { SubagentDto } from "@/api/types/SubagentDto";
import { StatusDot } from "./StatusDot";
import { RowChevron, TreeRow } from "./TreeRow";


export interface SubagentNodeProps {
  subagent: SubagentDto;
  depth: number;
  childrenMap: Map<string, SubagentDto[]>;
  selectedSubagentId?: string;
  collapsedSubagents: Set<string>;
  onToggleSubagent: (subagentId: string) => void;
  onSelectSubagent: (subagent: SubagentDto) => void;
}

export function SubagentNode({
  subagent,
  depth,
  childrenMap,
  selectedSubagentId,
  collapsedSubagents,
  onToggleSubagent,
  onSelectSubagent,
}: SubagentNodeProps) {
  const children = childrenMap.get(subagent.id) ?? [];
  const expanded = !collapsedSubagents.has(subagent.id);
  const selected = selectedSubagentId === subagent.id;
  const activate = () => onSelectSubagent(subagent);

  return (
    <div role="none">
      <TreeRow
        level={depth + 1}
        depth={depth}
        selected={selected}
        expanded={children.length > 0 ? expanded : undefined}
        onSelect={activate}
      >
        {children.length > 0 ? (
          <RowChevron
            expanded={expanded}
            label={subagent.title}
            onClick={(event) => {
              event.stopPropagation();
              onToggleSubagent(subagent.id);
            }}
          />
        ) : (
          <span style={{ width: 16, flexShrink: 0 }} aria-hidden />
        )}
        <StatusDot
          status={subagent.status}
          pulse={subagent.status === "running"}
        />
        <Bot
          size={14}
          aria-hidden
          style={{
            marginLeft: 2,
            flexShrink: 0,
            color: "var(--mantine-color-dimmed)",
          }}
        />
        <span
          style={{
            minWidth: 0,
            flex: 1,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {subagent.title}
        </span>
      </TreeRow>
      {expanded && children.length > 0 ? (
        <div
          style={{
            position: "relative",
            marginLeft: 12,
            borderLeft: "1px solid var(--mantine-color-default-border)",
            paddingLeft: 4,
          }}
        >
          {children.map((child) => (
            <SubagentNode
              key={child.id}
              subagent={child}
              depth={depth + 1}
              childrenMap={childrenMap}
              selectedSubagentId={selectedSubagentId}
              collapsedSubagents={collapsedSubagents}
              onToggleSubagent={onToggleSubagent}
              onSelectSubagent={onSelectSubagent}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}
