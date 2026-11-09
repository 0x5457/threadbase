import { useSyncExternalStore } from "react";

export type Theme = "dark" | "light";

const STORAGE_KEY = "threadbase.theme";
const listeners = new Set<() => void>();

function readStoredTheme(): Theme {
  try {
    const value = window.localStorage.getItem(STORAGE_KEY);
    return value === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}

let currentTheme: Theme = typeof window === "undefined" ? "dark" : readStoredTheme();

function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
}

export function initTheme(): void {
  currentTheme = readStoredTheme();
  applyTheme(currentTheme);
  window.addEventListener("storage", (event) => {
    if (event.key !== STORAGE_KEY) {
      return;
    }
    currentTheme = event.newValue === "light" ? "light" : "dark";
    applyTheme(currentTheme);
    for (const listener of listeners) {
      listener();
    }
  });
}

export function getTheme(): Theme {
  return currentTheme;
}

export function setTheme(theme: Theme): void {
  currentTheme = theme;
  applyTheme(theme);
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // A blocked storage backend should not stop the theme from applying.
  }
  for (const listener of listeners) {
    listener();
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useTheme(): Theme {
  return useSyncExternalStore(subscribe, getTheme, () => "dark");
}

export function toggleTheme(): void {
  setTheme(currentTheme === "dark" ? "light" : "dark");
}
