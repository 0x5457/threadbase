import { createContext, useContext } from "react";

/** Options without a provider prefix ("high", "medium") land here. */
export const OTHER_GROUP = "Other";
/** Pinned above the provider groups, holding the last few picks. */
export const RECENT_GROUP = "Recent";
/** How many picks are remembered per (agent, picker label). */
export const MAX_RECENT = 5;

/**
 * Recent picks are stored as a JSON array of option values, most recent first,
 * under `threadbase.models.recent.<agent>.<label>` so an agent's model history
 * never shows up in another agent's list or in the effort picker.
 */
export function recentStorageKey(agent: string, label: string) {
  return `threadbase.models.recent.${agent}.${label}`;
}

/** Provider an option belongs to: the text before its first "/". */
export function providerOf(value: string): string {
  const slash = value.indexOf("/");
  return slash > 0 ? value.slice(0, slash) : OTHER_GROUP;
}

/**
 * Buckets items by provider, each bucket keeping the order its items arrived
 * in, so groups follow the first item that belongs to them.
 */
export function groupByProvider<T extends { value: string }>(
  items: T[],
): { label: string; items: T[] }[] {
  const buckets = new Map<string, T[]>();
  for (const item of items) {
    const label = providerOf(item.value);
    const bucket = buckets.get(label);
    if (bucket) bucket.push(item);
    else buckets.set(label, [item]);
  }
  return Array.from(buckets, ([label, bucket]) => ({ label, items: bucket }));
}

/** Idle-list provider order: alphabetical, with "Other" last. */
export function sortByProvider<T extends { value: string }>(items: T[]): T[] {
  const last = (item: T) => (providerOf(item.value) === OTHER_GROUP ? 1 : 0);
  return [...items].sort(
    (left, right) =>
      last(left) - last(right) ||
      providerOf(left.value).localeCompare(providerOf(right.value)),
  );
}

/**
 * Recent values ready to render: the current selection pinned first so it
 * always stays reachable, then every stored pick, newest first.
 */
export function recentValues(
  recent: string[],
  selected: string | null,
): string[] {
  if (!selected) return recent;
  return [selected, ...recent.filter((value) => value !== selected)];
}

export function readRecent(agent: string, label: string): string[] {
  try {
    const raw = window.localStorage.getItem(recentStorageKey(agent, label));
    const parsed: unknown = raw === null ? null : JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((value): value is string => typeof value === "string")
      .slice(0, MAX_RECENT);
  } catch {
    // Storage can be missing (private mode) or hold something else; a picker
    // without history is still usable, so a bad read degrades to empty.
    return [];
  }
}

/** Records a pick and returns the stored list, most recent first. */
export function rememberRecent(
  agent: string,
  label: string,
  value: string,
): string[] {
  const next = [
    value,
    ...readRecent(agent, label).filter((entry) => entry !== value),
  ].slice(0, MAX_RECENT);
  try {
    window.localStorage.setItem(
      recentStorageKey(agent, label),
      JSON.stringify(next),
    );
  } catch {
    // The pick still applies; only its history is lost.
  }
  return next;
}

/**
 * Agent whose config the pickers below it edit. The pickers are instantiated
 * deeper in the toolbar, so the composer provides the agent here instead of
 * threading it through every layer; recents key on it.
 */
export const PickerAgentContext = createContext("agent");

export function usePickerAgent(): string {
  return useContext(PickerAgentContext);
}

/** Last explicitly picked model/effort for an agent, used for new threads and
 *  for threads that never recorded a choice. */
export function pickStorageKey(agent: string, kind: "model" | "effort") {
  return `threadbase.pick.${agent}.${kind}`;
}

export function readPick(agent: string, kind: "model" | "effort") {
  try {
    return window.localStorage.getItem(pickStorageKey(agent, kind)) || null;
  } catch {
    return null;
  }
}

export function rememberPick(
  agent: string,
  kind: "model" | "effort",
  value: string | null,
) {
  if (!value) return;
  try {
    window.localStorage.setItem(pickStorageKey(agent, kind), value);
  } catch {
    // Private mode: the pick simply does not persist.
  }
}
