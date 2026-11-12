import type { FileEntry } from "../../api";

export interface Crumb {
  label: string;
  path: string;
}

export function baseName(path: string) {
  return path.slice(path.lastIndexOf("/") + 1);
}

export function directoryOf(path: string) {
  const slash = path.lastIndexOf("/");
  return slash === -1 ? "" : path.slice(0, slash);
}

export function pathCrumbs(path: string): Crumb[] {
  const crumbs: Crumb[] = [{ label: "worktree", path: "" }];
  let current = "";
  for (const segment of path.split("/")) {
    if (segment === "") continue;
    current = current === "" ? segment : `${current}/${segment}`;
    crumbs.push({ label: segment, path: current });
  }
  return crumbs;
}

export function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : String(Math.round(value))} ${units[unit]}`;
}

export function sortEntries(entries: FileEntry[]) {
  return entries.toSorted((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}
