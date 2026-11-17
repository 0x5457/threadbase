import { useElementSize, useMediaQuery } from "@mantine/hooks";
import { useSyncExternalStore } from "react";

/**
 * Shared diff wrapping preference. `auto` follows the reading width (a diff
 * narrower than this box wraps; anything wider can scroll sideways); an
 * explicit choice overrides it everywhere and survives reloads. A tiny
 * external store keeps every mounted diff in sync.
 */
type DiffWrapPreference = "auto" | "wrap" | "scroll";

/**
 * The width below which a unified diff wraps instead of scrolling.
 *
 * This is a property of the box the diff is painted into, not of the window:
 * the changes aside is resizable and defaults to 420px, so a 1440px screen
 * still has a panel too narrow for aligned code. `auto` used to read the
 * viewport query alone, which left that panel scrolling horizontally.
 */
const AUTO_WRAP_BELOW = 560;

/** Viewport-width half of the old rule: tablets and phones always wrap. */
const NARROW_VIEWPORT = "(max-width: 74.99em)";

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

function setDiffWrap(next: DiffWrapPreference) {
  preference = next;
  try {
    if (next === "auto") window.localStorage.removeItem(KEY);
    else window.localStorage.setItem(KEY, next);
  } catch {
    // Private mode: the choice simply does not persist.
  }
  emit();
}

function useDiffWrapPreference() {
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

/**
 * Measure the box a diff is about to be painted into and report whether its
 * content should wrap. `auto` means "wrap when this box is narrow" — the
 * element can be the changes aside, a drawer, or an inline tool diff in the
 * transcript, so the window width cannot answer it. Mount the returned `ref`
 * on that box.
 */
export function useDiffWrap<T extends HTMLElement = HTMLDivElement>() {
  const narrowViewport = useMediaQuery(NARROW_VIEWPORT);
  const { ref, width } = useElementSize<T>();
  // `width` is 0 until the first observation; the viewport rule still gives a
  // sane answer for that frame and the real width arrives before paint.
  const narrow = narrowViewport || (width > 0 && width < AUTO_WRAP_BELOW);
  return { ref, narrow, wrap: useEffectiveDiffWrap(narrow) };
}

/** Flip the currently effective state and remember the explicit choice. */
export function toggleDiffWrap(narrow: boolean) {
  const effective = preference === "auto" ? narrow : preference === "wrap";
  setDiffWrap(effective ? "scroll" : "wrap");
}
