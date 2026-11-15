import { request } from "./client";
import type { SearchHitDto } from "./types/SearchHitDto";
import type { SearchResponse } from "./types/SearchResponse";

export type SearchHit = SearchHitDto;

export function searchWorkspace(
  query: string,
  options: { projectId?: string | null; limit?: number } = {},
) {
  const params = new URLSearchParams({ q: query });
  if (options.projectId) params.set("project_id", options.projectId);
  if (options.limit) params.set("limit", String(options.limit));
  return request<SearchResponse>(`/api/search?${params.toString()}`);
}
