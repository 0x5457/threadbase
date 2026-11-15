/**
 * Schedules a non-essential warm-up while the browser is idle.
 *
 * The chunk prefetches in the shell used to be fixed timers (1.2 s and 2.5 s
 * after paint). On a throttled phone those timers fired while the user was
 * already opening a thread, so the prefetch competed with the very render it
 * was meant to warm. Idle scheduling keeps the same work off the interaction
 * path, and a metered or save-data connection skips it entirely.
 */
export function whenIdle(task: () => void, timeout = 3_000): () => void {
  type IdleCapable = Window & {
    requestIdleCallback?: (
      callback: () => void,
      options?: { timeout: number },
    ) => number;
    cancelIdleCallback?: (handle: number) => void;
  };
  const connection = (
    navigator as Navigator & { connection?: { saveData?: boolean } }
  ).connection;
  if (connection?.saveData === true) return () => {};

  const idle = window as IdleCapable;
  if (typeof idle.requestIdleCallback === "function") {
    const handle = idle.requestIdleCallback(task, { timeout });
    return () => idle.cancelIdleCallback?.(handle);
  }
  // Safari still has no requestIdleCallback; a short timer keeps the same
  // "after paint, before the user needs it" behaviour.
  const timer = window.setTimeout(task, 1_500);
  return () => window.clearTimeout(timer);
}
