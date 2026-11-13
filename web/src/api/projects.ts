import { jsonRequest, request } from "./client";
import type { DirectoryListingDto } from "./types/DirectoryListingDto";
import type { ProjectDto } from "./types/ProjectDto";
import type { ProjectResponse } from "./types/ProjectResponse";
import type { ProjectsResponse } from "./types/ProjectsResponse";

export type Project = ProjectDto;
export type DirectoryListing = DirectoryListingDto;

export function listProjects() {
  return request<ProjectsResponse>("/api/projects");
}

export function createProject(
  path: string,
  options: { name?: string | null; create?: boolean } = {},
) {
  return jsonRequest<ProjectResponse>("/api/projects", "POST", {
    path,
    name: options.name ?? null,
    create: options.create ?? false,
  });
}

export function updateProject(
  projectId: string,
  patch: { name?: string; path?: string; remoteUrl?: string | null },
) {
  return jsonRequest<ProjectResponse>(`/api/projects/${projectId}`, "PATCH", patch);
}

export function browseDirectories(path?: string) {
  const query = path ? `?path=${encodeURIComponent(path)}` : "";
  return request<DirectoryListingDto>(`/api/fs/directories${query}`);
}

export function listSkills(projectId: string) {
  return request<import("./types/SkillsResponse").SkillsResponse>(
    `/api/projects/${projectId}/skills`,
  );
}

/**
 * Removes the project together with every thread, worktree and branch
 * Threadbase created for it. The repository directory is left untouched.
 */
export function deleteProject(projectId: string) {
  return request<{ ok: boolean }>(
    `/api/projects/${projectId}?confirm=true`,
    { method: "DELETE" },
  );
}
