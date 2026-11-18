import { useEffect, useRef, useState } from "react";

/**
 * Keeps an action icon turning until the next whole turn. A spinner that stops
 * halfway through a rotation reads as a jump; this one always lands at rest.
 */
export function useSpinningIcon(active: boolean, period = 640): boolean {
  const [spinning, setSpinning] = useState(active);
  const startedAt = useRef(active ? Date.now() : 0);

  useEffect(() => {
    if (active) {
      if (!spinning) {
        startedAt.current = Date.now();
        setSpinning(true);
      }
      return;
    }
    if (!spinning) return;
    const elapsed = Date.now() - startedAt.current;
    const remainder = elapsed % period;
    const remaining = remainder === 0 ? 0 : period - remainder;
    const timer = window.setTimeout(() => {
      setSpinning(false);
      startedAt.current = 0;
    }, remaining);
    return () => window.clearTimeout(timer);
  }, [active, period, spinning]);

  return spinning;
}
