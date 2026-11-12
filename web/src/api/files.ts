import { request } from "./client";
import type { FileContentResponseDto } from "./types/FileContentResponseDto";
import type { FileEntryDto } from "./types/FileEntryDto";
import type { FileSearchResponse } from "./types/FileSearchResponse";
import type { FilesResponseDto } from "./types/FilesResponseDto";

export type FileEntry = FileEntryDto;
export type FileContent = FileContentResponseDto["file"];

export function listFiles(threadId: string, path = "") {
  const query = path ? `?path=${encodeURIComponent(path)}` : "";
  return request<FilesResponseDto>(`/api/threads/${threadId}/files${query}`);
}

export function readFile(threadId: string, path: string) {
  return request<FileContentResponseDto>(
    `/api/threads/${threadId}/file?path=${encodeURIComponent(path)}`,
  );
}

export function searchFiles(threadId: string, query: string, limit = 50) {
  const params = new URLSearchParams({ q: query, limit: String(limit) });
  return request<FileSearchResponse>(
    `/api/threads/${threadId}/files/search?${params.toString()}`,
  );
}
