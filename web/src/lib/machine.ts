import catalogue from "../api/state-machines.json";
import type { MachineSpec } from "../api/types/MachineSpec";

/**
 * The lifecycle catalogue, generated from `threadbase-core` by
 * `crates/core/tests/state_machines.rs` and served by `GET /api/machines`.
 *
 * UI code asks `can("queue", item.status, "delete")` instead of comparing
 * status strings. When a transition changes on the server, the control
 * changes with it on the next build; there is no second rule set to forget.
 */
const MACHINES = catalogue as MachineSpec[];
const BY_ID = new Map(MACHINES.map((machine) => [machine.id, machine]));

type MachineId =
  | "thread"
  | "setup"
  | "approval"
  | "elicitation"
  | "subagent"
  | "queue"
  | "comment"
  | "watch"
  | "turn"
  | "auth_session"
  | "push"
  | "connection"
  | "draft"
  | "terminal";

/** True when `state` is a declared state of `id`. */
export function isState(id: MachineId, state: string): boolean {
  return BY_ID.get(id)?.states.some((candidate) => candidate.id === state) ?? false;
}

/** True when `action` is a legal command in `state`. */
export function can(
  id: MachineId,
  state: string | null | undefined,
  action: string,
): boolean {
  if (!state) return false;
  const entry = BY_ID.get(id)?.states.find((candidate) => candidate.id === state);
  return entry?.commands.some((command) => command.id === action) ?? false;
}

/** Human label for a state, falling back to the wire value. */
export function stateLabel(id: MachineId, state: string): string {
  const entry = BY_ID.get(id)?.states.find((candidate) => candidate.id === state);
  return entry?.label ?? state.replaceAll("_", " ");
}

/** Human label for an action, falling back to the wire value. */
export function actionLabel(id: MachineId, action: string): string {
  const entry = BY_ID.get(id)?.actions.find((candidate) => candidate.id === action);
  return entry?.label ?? action.replaceAll("_", " ");
}

/** True when `allowed_actions` (from `ThreadDto` / `ThreadState`) permits a command. */
export function threadCan(
  allowedActions: readonly string[] | undefined,
  action: string,
): boolean {
  return allowedActions?.includes(action) ?? false;
}
