import { useMantineColorScheme } from "@mantine/core";
import { useEffect } from "react";

export type Theme = "dark" | "light";

/** Single source of truth for the persisted color scheme key. */
export const THEME_STORAGE_KEY = "threadbase.theme";

let toggleImpl: (() => void) | null = null;
let setImpl: ((theme: Theme) => void) | null = null;

/**
 * Bridges the `toggleTheme` / `setTheme` helpers to Mantine's color scheme
 * state. Renders nothing.
 */
export function ThemeBridge() {
  const { toggleColorScheme, setColorScheme } = useMantineColorScheme();

  useEffect(() => {
    toggleImpl = () => toggleColorScheme();
    setImpl = (theme: Theme) => setColorScheme(theme);
    return () => {
      toggleImpl = null;
      setImpl = null;
    };
  }, [toggleColorScheme, setColorScheme]);

  return null;
}

export function useTheme(): Theme {
  return useMantineColorScheme().colorScheme as Theme;
}

export function toggleTheme(): void {
  toggleImpl?.();
}

export function setTheme(theme: Theme): void {
  setImpl?.(theme);
}
