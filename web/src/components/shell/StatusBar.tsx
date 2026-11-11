import { Box, Button, Text, Tooltip } from "@mantine/core";
import {
  AlertCircle,
  FileDiff,
  FolderGit2,
  Folder,
  GitBranch,
} from "lucide-react";
import type { ProjectDto } from "@/api/types/ProjectDto";
import type { ThreadDto } from "@/api/types/ThreadDto";
import { basename } from "@/lib/format";

export interface StatusBarProps {
  project?: ProjectDto | null;
  thread?: ThreadDto | null;
  runningCount: number;
  waitingCount: number;
  projectCount: number;
  threadCount: number;
  offline?: boolean;
  onOpenChanges: () => void;
}

export function StatusBar({
  project,
  thread,
  runningCount,
  waitingCount,
  projectCount,
  threadCount,
  offline = false,
  onOpenChanges,
}: StatusBarProps) {
  const workIn = thread?.worktreePath
    ? basename(thread.worktreePath)
    : thread
      ? "Isolated worktree"
      : null;

  return (
    <footer
      style={{
        zIndex: 20,
        display: "flex",
        height: 32,
        flexShrink: 0,
        alignItems: "center",
        gap: 12,
        borderTop: "1px solid var(--mantine-color-default-border)",
        background: "var(--mantine-color-body)",
        paddingInline: 12,
      }}
    >
      <div
        style={{
          display: "flex",
          minWidth: 0,
          alignItems: "center",
          gap: 10,
        }}
      >
        <Tooltip label={project?.path ?? "No project selected"} inline>
          <span
            style={{
              display: "flex",
              minWidth: 0,
              alignItems: "center",
              gap: 4,
            }}
          >
            <Folder
              size={12}
              aria-hidden
              style={{ flexShrink: 0, color: "var(--mantine-color-dimmed)" }}
            />
            <span
              style={{
                maxWidth: 160,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
            >
              {project?.name ?? "Threadbase"}
            </span>
          </span>
        </Tooltip>
        {thread ? (
          <>
            <span
              aria-hidden
              style={{
                width: 1,
                height: 12,
                background: "var(--mantine-color-default-border)",
              }}
            />
            <span
              style={{
                display: "flex",
                alignItems: "center",
                gap: 4,
              }}
            >
              <GitBranch
                size={12}
                aria-hidden
                style={{ flexShrink: 0, color: "var(--mantine-color-dimmed)" }}
              />
              <span
                style={{
                  maxWidth: 140,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {thread.branch}
              </span>
            </span>
            {workIn ? (
              <>
                <Box
                  component="span"
                  visibleFrom="sm"
                  aria-hidden
                  style={{
                    width: 1,
                    height: 12,
                    background: "var(--mantine-color-default-border)",
                  }}
                />
                <Tooltip
                  label={
                    thread.worktreePath
                      ? `Work In: ${thread.worktreePath}`
                      : "Work In: managed isolated worktree"
                  }
                  inline
                >
                  <Box
                    component="span"
                    visibleFrom="sm"
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 4,
                    }}
                  >
                    <FolderGit2
                      size={12}
                      aria-hidden
                      style={{ flexShrink: 0, color: "var(--mantine-color-dimmed)" }}
                    />
                    <span
                      style={{
                        maxWidth: 140,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {workIn}
                    </span>
                  </Box>
                </Tooltip>
              </>
            ) : null}
          </>
        ) : (
          <Text size="xs" c="dimmed">
            {projectCount} {projectCount === 1 ? "project" : "projects"} ·{" "}
            {threadCount} {threadCount === 1 ? "thread" : "threads"}
          </Text>
        )}
      </div>

      <div
        style={{
          marginLeft: "auto",
          display: "flex",
          flexShrink: 0,
          alignItems: "center",
          gap: 12,
        }}
      >
        {offline ? (
          <span
            style={{
              display: "flex",
              alignItems: "center",
              gap: 4,
              color: "var(--mantine-color-yellow-filled)",
            }}
          >
            <AlertCircle size={12} aria-hidden />
            <Text size="xs">Reconnecting…</Text>
          </span>
        ) : null}
        {thread ? (
          <Button
            type="button"
            variant="subtle"
            size="sm"
            onClick={onOpenChanges}
            title="Open changes"
            leftSection={<FileDiff size={14} aria-hidden />}
            style={{
              height: "auto",
              gap: 6,
              padding: "2px 4px",
              fontWeight: 400,
              color: "var(--mantine-color-dimmed)",
            }}
          >
            <span
              style={{
                display: "inline-flex",
                gap: 4,
                fontVariantNumeric: "tabular-nums",
              }}
            >
              <Text size="xs" c="green">
                +{thread.changes.additions}
              </Text>
              <Text size="xs" c="red">
                −{thread.changes.deletions}
              </Text>
            </span>
            {thread.changes.seen > 0 && thread.changes.total > 0 ? (
              <Text size="xs" c="dimmed">
                {thread.changes.seen}/{thread.changes.total} seen
              </Text>
            ) : null}
          </Button>
        ) : null}
        <CountIndicator
          count={runningCount}
          label="running"
          active={runningCount > 0}
          color="var(--mantine-color-blue-filled)"
        />
        <CountIndicator
          count={waitingCount}
          label="waiting"
          active={waitingCount > 0}
          color="var(--mantine-color-yellow-filled)"
        />
      </div>
    </footer>
  );
}

/** One visual language for every status-bar counter: a 6px dot + count. */
function CountIndicator({
  count,
  label,
  active,
  color,
}: {
  count: number;
  label: string;
  active: boolean;
  color: string;
}) {
  return (
    <span
      title={`${count} ${label}`}
      style={{ display: "flex", alignItems: "center", gap: 4 }}
    >
      <span
        aria-hidden
        style={{
          width: 6,
          height: 6,
          borderRadius: "50%",
          flexShrink: 0,
          background: active
            ? color
            : "var(--mantine-color-default-border)",
        }}
      />
      <Text size="xs" c={active ? undefined : "dimmed"}>
        {count} {label}
      </Text>
    </span>
  );
}
