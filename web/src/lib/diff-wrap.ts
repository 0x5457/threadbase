import { useSyncExternalStore } from "react";

/**
 * Shared diff wrapping preference. `auto` follows the reading width (wrap on
 * tablets and phones); an explicit choice overrides it everywhere and survives
 * reloads. A tiny external store keeps every mounted diff in sync.
 */
export type DiffWrapPreference = "auto" | "wrap" | "scroll";

const KEY = "threadbase.diff-wrap";
const listeners = new Set<() => void>();

function read(): DiffWrapPreference {
  try {
    const value = window.localStorage.getItem(KEY);
    return value === "wrap" || value === "scroll" ? value : "auto";
  } catch {
    return "auto";
  }
}

let preference: DiffWrapPreference = read();

function emit() {
  for (const listener of listeners) listener();
}

export function setDiffWrap(next: DiffWrapPreference) {
  preference = next;
  try {
    if (next === "auto") window.localStorage.removeItem(KEY);
    else window.localStorage.setItem(KEY, next);
  } catch {
    // Private mode: the choice simply does not persist.
  }
  emit();
}

export function useDiffWrapPreference() {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => preference,
    () => "auto" as const,
  );
}

export function useEffectiveDiffWrap(narrow: boolean) {
  const current = useDiffWrapPreference();
  return current === "auto" ? narrow : current === "wrap";
}

/** Flip the currently effective state and remember the explicit choice. */
export function toggleDiffWrap(narrow: boolean) {
  const effective = preference === "auto" ? narrow : preference === "wrap";
  setDiffWrap(effective ? "scroll" : "wrap");
}
