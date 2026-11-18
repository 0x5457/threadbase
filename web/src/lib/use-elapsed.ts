import { useEffect, useState } from "react";

/** Whole seconds since `since`, ticking while `active` is true. */
export function useElapsed(since: number, active = true): number {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [active, since]);

  return Math.max(0, Math.floor((now - since) / 1000));
}
