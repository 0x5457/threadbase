import { useQuery } from "@tanstack/react-query";
import { getState } from "../api";
import type { PermissionMode } from "../api/types/PermissionMode";

/** The ladder, weakest first. Order matters: the ceiling is a rank. */
export const PERMISSION_MODES: PermissionMode[] = [
  "supervised",
  "auto_edit",
  "full_access",
];

const LABELS: Record<PermissionMode, string> = {
  supervised: "Supervised",
  auto_edit: "Auto-accept edits",
  full_access: "Full access",
};

const HINTS: Record<PermissionMode, string> = {
  supervised: "Commands and edits both ask.",
  auto_edit: "Edits are accepted; commands and network still ask.",
  full_access: "Nothing asks. Every tool runs unattended.",
};

export function permissionModeLabel(mode: PermissionMode): string {
  return LABELS[mode] ?? mode;
}

export function permissionModeHint(mode: PermissionMode): string {
  return HINTS[mode] ?? "";
}

export function permissionModeRank(mode: PermissionMode): number {
  const index = PERMISSION_MODES.indexOf(mode);
  return index === -1 ? 0 : index;
}

/** True when the instance ceiling forbids this mode. */
export function permissionModeManaged(
  mode: PermissionMode,
  cap: PermissionMode,
): boolean {
  return permissionModeRank(mode) > permissionModeRank(cap);
}

/**
 * The instance's permission defaults. Shared cache entry: the picker, the
 * project settings and the settings page all read the same query, so the
 * ceiling is one request no matter how many surfaces ask.
 */
export function usePermissionPolicy() {
  const state = useQuery({
    queryKey: ["state"],
    queryFn: getState,
    staleTime: 60_000,
  });
  const policy = state.data?.permissions;
  return {
    defaultMode: policy?.default ?? "supervised",
    cap: policy?.cap ?? "full_access",
  };
}
