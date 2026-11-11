/** Tree navigation helpers: ordering, activation and roving focus. */
import type { KeyboardEvent } from "react";
import type { ThreadDto } from "@/api/types/ThreadDto";

export function sortThreads(threads: ThreadDto[]): ThreadDto[] {
  return [...threads].sort((a, b) => {
    const aTime = Number(a.updatedAt ?? a.createdAt ?? 0);
    const bTime = Number(b.updatedAt ?? b.createdAt ?? 0);
    if (aTime !== bTime) {
      return bTime - aTime;
    }
    return a.title.localeCompare(b.title);
  });
}

export function activateRow(
  event: KeyboardEvent<HTMLDivElement>,
  action: () => void,
): void {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    action();
  }
}

function visibleTreeRows(container: HTMLElement | null): HTMLElement[] {
  if (!container) {
    return [];
  }
  return Array.from(
    container.querySelectorAll<HTMLElement>("[data-tree-row]"),
  ).filter((element) => element.offsetParent !== null);
}

export function moveTreeFocus(
  event: KeyboardEvent<HTMLElement>,
  container: HTMLElement | null,
): boolean {
  const rows = visibleTreeRows(container);
  const target = (event.target as HTMLElement).closest<HTMLElement>(
    "[data-tree-row]",
  );
  if (!target || rows.length === 0) {
    return false;
  }
  const index = rows.indexOf(target);
  if (index < 0) {
    return false;
  }

  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const offset = event.key === "ArrowDown" ? 1 : -1;
    const next = Math.min(rows.length - 1, Math.max(0, index + offset));
    rows[next]?.focus();
    return true;
  }
  if (event.key === "Home" || event.key === "End") {
    event.preventDefault();
    rows[event.key === "Home" ? 0 : rows.length - 1]?.focus();
    return true;
  }
  if (event.key === "ArrowRight") {
    const expanded = target.dataset.expanded;
    if (expanded === "false") {
      const chevron = target.querySelector<HTMLElement>("[data-tree-chevron]");
      if (chevron) {
        event.preventDefault();
        chevron.click();
        return true;
      }
    }
  }
  if (event.key === "ArrowLeft") {
    if (target.dataset.expanded === "true") {
      const chevron = target.querySelector<HTMLElement>("[data-tree-chevron]");
      if (chevron) {
        event.preventDefault();
        chevron.click();
        return true;
      }
    } else {
      const depth = Number(target.dataset.depth ?? 0);
      for (let cursor = index - 1; cursor >= 0; cursor -= 1) {
        if (Number(rows[cursor]?.dataset.depth ?? 0) < depth) {
          event.preventDefault();
          rows[cursor]?.focus();
          return true;
        }
      }
    }
  }
  return false;
}
