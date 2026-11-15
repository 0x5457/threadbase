/**
 * How much diff the app is willing to render.
 *
 * A changed file can be arbitrarily large: an agent writing a generated
 * bundle, a lockfile, a log, a minified asset. The diff renderer turns every
 * line into DOM, so a 500k-line patch becomes millions of nodes and a
 * gigabyte of heap — the tab stops responding, and because the Changes panel
 * is mounted for every worktree thread the same work repeats on every page
 * switch until the browser is unusable.
 *
 * These budgets are the seam between "this is a reviewable change" and "open
 * it in your editor": past them the panel refuses to fetch or render the
 * patch and shows the counts instead. They live here so the panel, the
 * transcript's tool rows and the renderer all agree.
 */

/** Rendered patch lines (added + removed + context). ~6 DOM nodes per line. */
export const MAX_DIFF_LINES = 4_000;

/** Patch bytes; protects against a few enormous lines with no newlines. */
export const MAX_DIFF_CHARS = 512 * 1024;

/**
 * Counts lines without splitting the string, stopping just past `limit`.
 * `split("\n")` on a 36 MB patch allocates a half-million-element array
 * before the first render even starts.
 */
export function countLines(text: string, limit = MAX_DIFF_LINES): number {
  let lines = text === "" ? 0 : 1;
  for (let index = 0; index < text.length; index += 1) {
    if (text.charCodeAt(index) !== 10) continue;
    lines += 1;
    if (lines > limit) return lines;
  }
  return lines;
}

/** True when a patch is too large to hand to the diff renderer. */
export function exceedsDiffBudget(patch: string): boolean {
  if (patch.length > MAX_DIFF_CHARS) return true;
  return countLines(patch) > MAX_DIFF_LINES;
}

/**
 * True when the changed-line counts from a file list already exceed the
 * budget, so the panel can skip fetching the patch at all.
 */
export function exceedsLineBudget(additions: number, deletions: number): boolean {
  return additions + deletions > MAX_DIFF_LINES;
}

/** "4,000+ lines" style label for the notice; never overstates precision. */
export function budgetLabel(): string {
  return `${MAX_DIFF_LINES.toLocaleString()}+ lines`;
}
