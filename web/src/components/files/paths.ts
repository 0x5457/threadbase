import { basename, dirname } from "pathe";
import prettyBytes from "pretty-bytes";
import type { FileEntry } from "../../api";

export interface Crumb {
  label: string;
  path: string;
}

export function baseName(path: string) {
  return basename(path);
}

export function directoryOf(path: string) {
  const directory = dirname(path);
  return directory === "." ? "" : directory;
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
  return prettyBytes(bytes);
}

export function sortEntries(entries: FileEntry[]) {
  return entries.toSorted((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}
