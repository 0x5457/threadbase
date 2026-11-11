/** One thread row plus its nested subthreads. */
import { Bookmark, MessageSquareText } from "lucide-react";
import { Badge, Box, UnstyledButton } from "@mantine/core";
import { useState } from "react";
import type { SubagentDto } from "@/api/types/SubagentDto";
import type { ThreadDto } from "@/api/types/ThreadDto";
import { AvatarStack } from "@/components/app";
import { StatusDot } from "./StatusDot";
import { RowChevron, TreeRow } from "./TreeRow";
import { SubagentNode } from "./SubagentNode";
import { rowHover } from "./rowStyles";


export interface ThreadNodeProps {
  thread: ThreadDto;
  depth: number;
  projectName: string;
  showProjectName: boolean;
  childThreads: Map<string, ThreadDto[]>;
  subagentsByThread: Map<string, SubagentDto[]>;
  subagentChildren: Map<string, SubagentDto[]>;
  selectedThreadId?: string;
  selectedSubagentId?: string;
  threadsPerGroup: number;
  collapsedThreads: Set<string>;
  onToggleThread: (threadId: string) => void;
  onSelectThread: (thread: ThreadDto) => void;
  onSelectSubagent: (subagent: SubagentDto) => void;
}

export function ThreadNode({
  thread,
  depth,
  projectName,
  showProjectName,
  childThreads,
  subagentsByThread,
  subagentChildren,
  selectedThreadId,
  selectedSubagentId,
  threadsPerGroup,
  collapsedThreads,
  onToggleThread,
  onSelectThread,
  onSelectSubagent,
}: ThreadNodeProps) {
  const [showAll, setShowAll] = useState(false);
  const children = childThreads.get(thread.id) ?? [];
  const subagentRoots = (subagentsByThread.get(thread.id) ?? []).filter(
    (subagent) =>
      !subagent.parentId ||
      !(subagentsByThread.get(thread.id) ?? []).some(
        (candidate) => candidate.id === subagent.parentId,
      ),
  );
  const childCount = children.length + subagentRoots.length;
  const expanded = !collapsedThreads.has(thread.id);
  const selected = selectedThreadId === thread.id && !selectedSubagentId;
  const limit = showAll ? Number.POSITIVE_INFINITY : threadsPerGroup;
  const visibleThreads = children.slice(0, limit);
  const visibleSubagents = subagentRoots.slice(
    0,
    Math.max(0, limit - visibleThreads.length),
  );
  const remaining =
    children.length +
    subagentRoots.length -
    visibleThreads.length -
    visibleSubagents.length;

  const people = thread.participants.map((participant) => ({
    id: participant.user.id,
    displayName: participant.user.displayName,
    avatarUrl: participant.user.avatarUrl,
    color: participant.user.color,
  }));

  const activate = () => onSelectThread(thread);

  return (
    <div role="none">
      <TreeRow
        level={depth + 1}
        depth={depth}
        selected={selected}
        expanded={childCount > 0 ? expanded : undefined}
        onSelect={activate}
      >
        {childCount > 0 ? (
          <RowChevron
            expanded={expanded}
            label={thread.title}
            onClick={(event) => {
              event.stopPropagation();
              onToggleThread(thread.id);
            }}
          />
        ) : (
          <span style={{ width: 16, flexShrink: 0 }} aria-hidden />
        )}
        <StatusDot status={thread.status} pulse={thread.status === "running"} />
        <MessageSquareText
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
            ...(selected || thread.unread ? { fontWeight: 500 } : {}),
          }}
        >
          {thread.title}
        </span>
        {thread.pinned ? (
          <Bookmark
            size={12}
            aria-label="Pinned"
            style={{ flexShrink: 0, color: "var(--mantine-color-dimmed)" }}
          />
        ) : null}
        {thread.pendingApprovals > 0 ? (
          <Badge color="yellow" variant="light" size="xs">
            {thread.pendingApprovals}
          </Badge>
        ) : null}
        {people.length > 0 ? (
          <Box style={{ flexShrink: 0 }}>
            <AvatarStack people={people} max={2} size="xs" />
          </Box>
        ) : null}
        {thread.unread ? (
          <span
            style={{
              width: 6,
              height: 6,
              flexShrink: 0,
              borderRadius: "50%",
              backgroundColor: "var(--mantine-color-blue-filled)",
            }}
            aria-label="Unread"
          />
        ) : null}
      </TreeRow>
      {showProjectName && depth === 0 ? (
        <p
          aria-hidden
          style={{
            margin: 0,
            padding: "0 8px 4px 36px",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            fontSize: "var(--mantine-font-size-md)",
            color: "var(--mantine-color-dimmed)",
          }}
        >
          {projectName}
        </p>
      ) : null}
      {expanded && childCount > 0 ? (
        <div
          style={{
            position: "relative",
            marginLeft: 12,
            borderLeft: "1px solid var(--mantine-color-default-border)",
            paddingLeft: 4,
          }}
        >
          {visibleThreads.map((child) => (
            <ThreadNode
              key={child.id}
              thread={child}
              depth={depth + 1}
              projectName={projectName}
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
          {visibleSubagents.map((subagent) => (
            <SubagentNode
              key={subagent.id}
              subagent={subagent}
              depth={depth + 1}
              childrenMap={subagentChildren}
              selectedSubagentId={selectedSubagentId}
              collapsedSubagents={collapsedThreads}
              onToggleSubagent={onToggleThread}
              onSelectSubagent={onSelectSubagent}
            />
          ))}
          {remaining > 0 ? (
            <UnstyledButton
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                setShowAll(true);
              }}
              h={{ base: 44, md: 40 }}
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
