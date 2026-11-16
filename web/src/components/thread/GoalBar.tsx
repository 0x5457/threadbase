import { ActionIcon, Group, Text, Tooltip, UnstyledButton } from "@mantine/core";
import {
  IconChevronDown,
  IconPlayerPause,
  IconPlayerPlay,
  IconListCheck,
} from "@tabler/icons-react";
import { memo, useId, useState } from "react";
import type { GoalDto } from "../../api/types/GoalDto";
import type { GoalTaskDto } from "../../api/types/GoalTaskDto";
import { formatDuration, formatTokens } from "./format";

/**
 * One goal, as the adapter reports it.
 *
 * The bar stays out of the transcript: goals outlive turns, and progress is
 * ambient state rather than a message. Pause and resume are the adapter's own
 * commands (`/goal-pause`, `/goal-resume`), so the server stays free of goal
 * semantics.
 */

function tone(status: string) {
  switch (status) {
    case "active":
      return "running";
    case "complete":
      return "completed";
    case "blocked":
      return "failed";
    case "budget_limited":
      return "waiting";
    default:
      return "idle";
  }
}

function label(status: string) {
  return status.replaceAll("_", " ");
}

/**
 * What the bar says while a goal is not `complete`.
 *
 * The adapter's own status is `active` even after every task is done: completion
 * is gated by a separate audit, and a rejected audit sends the agent back to
 * work. Without that context an all-checks-green goal reads as "stuck", so say
 * which stage it is in.
 */
function stage(goal: GoalDto, done: number): string | null {
  if (goal.status === "complete") return null;
  const verdict = goal.audit?.verdict;
  const allDone = goal.tasks.length > 0 && done >= goal.tasks.length;
  if (!allDone) {
    // Auto-continue work runs outside a turn, so the thread reads `idle` while
    // the agent is still going. Say it here rather than leave the bar mute.
    return goal.status === "active" ? "working" : null;
  }
  switch (verdict) {
    case "disapproved":
      return "audit rejected · reworking";
    case "error":
      return "audit failed · retrying";
    case "approved":
      return "audit passed · completing";
    default:
      return "tasks done · awaiting audit";
  }
}

function taskMark(task: GoalTaskDto) {
  if (task.status === "complete") return "✓";
  if (task.status === "skipped") return "–";
  return "○";
}

/** Goal time is wall-clock work, so hours read better than "155m". */
function timeSpent(seconds: number) {
  if (seconds < 3600) return formatDuration(seconds * 1000);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.round((seconds % 3600) / 60);
  return `${hours}h ${minutes}m`;
}

function TaskRow({ task }: { task: GoalTaskDto }) {
  const open = task.subtasks.filter(
    (subtask) => subtask.status !== "complete" && subtask.status !== "skipped",
  ).length;
  return (
    <li className="goal-task" data-status={task.status}>
      <span className="goal-task-mark" aria-hidden="true">
        {taskMark(task)}
      </span>
      <div className="goal-task-body">
        <Text size="xs" className="goal-task-title">
          {task.title || task.id}
        </Text>
        {task.evidence ? (
          <Text size="xs" c="dimmed" className="goal-task-detail">
            {task.evidence}
          </Text>
        ) : task.verificationContract ? (
          <Text size="xs" c="dimmed" className="goal-task-detail">
            {task.verificationContract}
          </Text>
        ) : null}
        {open > 0 ? (
          <Text size="xs" c="dimmed" className="goal-task-detail">
            {open} subtask{open === 1 ? "" : "s"} left
          </Text>
        ) : null}
      </div>
    </li>
  );
}

export const GoalBar = memo(function GoalBar({
  goal,
  canWrite,
  busy,
  onCommand,
}: {
  goal: GoalDto;
  canWrite: boolean;
  /** A goal command is in flight; buttons disable instead of double-sending. */
  busy: boolean;
  onCommand: (command: string, interrupt?: boolean) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const panelId = useId();
  const done = goal.tasks.filter(
    (task) => task.status === "complete" || task.status === "skipped",
  ).length;
  const skipped = goal.tasks.filter((task) => task.status === "skipped").length;
  const paused = goal.status === "paused";
  const finished = goal.status === "complete";
  const auditStage = stage(goal, done);
  const auditAt = goal.audit?.at ? new Date(goal.audit.at).toLocaleString() : null;

  return (
    <div className="goal-bar">
      <div className="goal-bar-head">
        <UnstyledButton
          className="goal-bar-toggle"
          onClick={() => setExpanded((value) => !value)}
          aria-expanded={expanded}
          aria-controls={panelId}
          title={expanded ? "Collapse goal" : "Expand goal tasks"}
        >
          <span className={`dot ${tone(goal.status)}`} />
          <Text size="xs" fw={600} className="goal-bar-label">
            {goal.sisyphus ? "Sisyphus goal" : "Goal"}
          </Text>
          <Text size="xs" c="dimmed" className="goal-bar-objective">
            {goal.objective}
          </Text>
        </UnstyledButton>
        <Group gap={8} className="goal-bar-meta" wrap="nowrap">
          <Text size="xs" c="dimmed" title={auditAt ?? undefined}>
            {auditStage ?? label(goal.status)}
          </Text>
          {goal.tasks.length > 0 ? (
            <Tooltip
              label={
                skipped > 0
                  ? `${done} of ${goal.tasks.length} tasks done (${skipped} skipped)`
                  : `${done} of ${goal.tasks.length} tasks done`
              }
            >
              <Group gap={3} wrap="nowrap" className="goal-bar-count">
                <IconListCheck size={12} stroke={1.5} />
                <Text size="xs" c="dimmed">
                  {done}/{goal.tasks.length}
                </Text>
              </Group>
            </Tooltip>
          ) : null}
          {goal.tokensUsed > 0 ? (
            <Text size="xs" c="dimmed" title={`${goal.tokensUsed} tokens`}>
              {formatTokens(goal.tokensUsed)} tok
            </Text>
          ) : null}
          {goal.activeSeconds > 0 ? (
            <Text size="xs" c="dimmed">
              {timeSpent(goal.activeSeconds)}
            </Text>
          ) : null}
          <IconChevronDown
            size={12}
            stroke={1.5}
            className={`goal-bar-chevron${expanded ? " open" : ""}`}
            aria-hidden="true"
          />
        </Group>
        {canWrite ? (
          <Group gap={2} wrap="nowrap" className="goal-bar-actions">
            {!finished ? (
              <Tooltip label={paused ? "Resume goal" : "Pause goal"}>
                <ActionIcon
                  size={22}
                  variant="subtle"
                  color="gray"
                  disabled={busy}
                  aria-label={paused ? "Resume goal" : "Pause goal"}
                  onClick={() =>
                    onCommand(paused ? "/goal-resume" : "/goal-pause", !paused)
                  }
                >
                  {paused ? (
                    <IconPlayerPlay size={13} stroke={1.5} />
                  ) : (
                    <IconPlayerPause size={13} stroke={1.5} />
                  )}
                </ActionIcon>
              </Tooltip>
            ) : null}
          </Group>
        ) : null}
      </div>
      {expanded ? (
        <div className="goal-tasks" id={panelId}>
          {goal.tasks.length === 0 ? (
            <Text size="xs" c="dimmed" className="goal-task-detail">
              No task plan recorded yet.
            </Text>
          ) : (
            <ol className="goal-task-list">
              {goal.tasks.map((task) => (
                <TaskRow key={task.id} task={task} />
              ))}
            </ol>
          )}
          {goal.autoContinue ? (
            <Text size="xs" c="dimmed" className="goal-task-detail">
              Continues automatically while active.{" "}
              {goal.updatedAt ? `Updated ${new Date(goal.updatedAt).toLocaleString()}.` : ""}
            </Text>
          ) : null}
          {goal.audit ? (
            <Text size="xs" c="dimmed" className="goal-task-detail">
              {`Completion audit: ${goal.audit.verdict}`}
              {auditAt ? ` · ${auditAt}` : ""}
              {goal.audit.summary ? ` — ${goal.audit.summary}` : ""}
            </Text>
          ) : null}
        </div>
      ) : null}
    </div>
  );
});
