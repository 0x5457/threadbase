import { useMediaQuery as useMantineMediaQuery } from "@mantine/hooks";
import { useEffect, useState, useSyncExternalStore } from "react";

/**
 * Mantine's media-query hook, but with the initial value read synchronously so
 * the shell does not flash the wrong layout on first paint.
 */
export function useMediaQuery(query: string): boolean {
  return useMantineMediaQuery(query, false, {
    getInitialValueInEffect: false,
  });
}

function subscribeToOnlineStatus(callback: () => void): () => void {
  window.addEventListener("online", callback);
  window.addEventListener("offline", callback);
  return () => {
    window.removeEventListener("online", callback);
    window.removeEventListener("offline", callback);
  };
}

export function useOnlineStatus(): boolean {
  return useSyncExternalStore(
    subscribeToOnlineStatus,
    () => navigator.onLine,
    () => true,
  );
}

export function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const timeout = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(timeout);
  }, [value, delayMs]);

  return debounced;
}

