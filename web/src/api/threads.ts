import { jsonRequest, request } from "./client";
import type { ChangesDto } from "./types/ChangesDto";
import type { CreateThreadRequest } from "./types/CreateThreadRequest";
import type { DeliveryMode } from "./types/DeliveryMode";
import type { EventEnvelope } from "./types/EventEnvelope";
import type { FileChangeDto } from "./types/FileChangeDto";
import type { FileDiffDto } from "./types/FileDiffDto";
import type { ForkThreadRequest } from "./types/ForkThreadRequest";
import type { MergeBackResponse } from "./types/MergeBackResponse";
import type { OkResponse } from "./types/OkResponse";
import type { SendMessageResponse } from "./types/SendMessageResponse";
import type { ThreadBundleDto } from "./types/ThreadBundleDto";
import type { ThreadDto } from "./types/ThreadDto";
import type { ThreadResponse } from "./types/ThreadResponse";
import type { ThreadsResponse } from "./types/ThreadsResponse";
import type { UpdateThreadRequest } from "./types/UpdateThreadRequest";

export type ThreadSummary = ThreadDto;
export type ThreadBundle = ThreadBundleDto;
export type ThreadChanges = ChangesDto;
export type { MergeBackResponse } from "./types/MergeBackResponse";
export type FileChange = FileChangeDto;
export type FileDiff = FileDiffDto;
export type ThreadEvent = EventEnvelope;

export type UpdateThreadPatch = Partial<
  Pick<
    UpdateThreadRequest,
    "title" | "pinned" | "archived" | "shareLevel" | "model" | "effort"
  >
>;

export interface CreateThreadInput {
  title: string;
  agent: string;
  model?: string | null;
  effort?: string | null;
  baseBranch: string;
  initialPrompt?: string | null;
}

export interface Baseline {
  kind?: ChangesDto["baseline"]["kind"];
  commit?: string | null;
}

export function listAllThreads() {
  return request<ThreadsResponse>("/api/threads");
}

export function listThreads(projectId: string) {
  return request<ThreadsResponse>(`/api/projects/${projectId}/threads`);
}

export function getThread(threadId: string) {
  return request<ThreadResponse>(`/api/threads/${threadId}`);
}

export function getThreadBundle(threadId: string) {
  return request<{ bundle: ThreadBundle }>(`/api/threads/${threadId}/bundle`);
}

export function getFileDiff(threadId: string, path: string, baseline?: Baseline) {
  const params = new URLSearchParams({ path });
  if (baseline?.kind) params.set("kind", baseline.kind);
  if (baseline?.commit) params.set("commit", baseline.commit);
  return request<FileDiffDto>(
    `/api/threads/${threadId}/changes/file?${params.toString()}`,
  );
}

export function createThread(projectId: string, input: CreateThreadInput) {
  const body: CreateThreadRequest = {
    title: input.title,
    agent: input.agent,
    model: input.model ?? null,
    effort: input.effort ?? null,
    baseBranch: input.baseBranch,
    initialPrompt: input.initialPrompt ?? null,
  };
  return jsonRequest<ThreadResponse>(
    `/api/projects/${projectId}/threads`,
    "POST",
    body,
  );
}

export function updateThread(threadId: string, patch: UpdateThreadPatch) {
  return jsonRequest<ThreadResponse>(`/api/threads/${threadId}`, "PATCH", patch);
}

export function forkThread(threadId: string, title?: string) {
  const body: ForkThreadRequest = { title: title ?? null };
  return jsonRequest<ThreadResponse>(`/api/threads/${threadId}/fork`, "POST", body);
}

export function mergeThread(threadId: string, commit = true) {
  return jsonRequest<MergeBackResponse>(
    `/api/threads/${threadId}/merge-back`,
    "POST",
    { commit, commitMessage: null },
  );
}

export function revertThread(threadId: string) {
  return jsonRequest<OkResponse>(`/api/threads/${threadId}/revert`, "POST");
}

export function sendMessage(
  threadId: string,
  text: string,
  delivery?: DeliveryMode,
) {
  return jsonRequest<SendMessageResponse>(
    `/api/threads/${threadId}/messages`,
    "POST",
    { text, delivery: delivery ?? null, fromDraft: false },
  );
}

/** Send the accumulated CRDT draft blocks. */
export function sendDraft(threadId: string, delivery?: DeliveryMode) {
  return jsonRequest<SendMessageResponse>(
    `/api/threads/${threadId}/messages`,
    "POST",
    { text: "", delivery: delivery ?? null, fromDraft: true },
  );
}

export function dropQueueItem(threadId: string, itemId: string) {
  return jsonRequest<OkResponse>(
    `/api/threads/${threadId}/queue/${itemId}`,
    "DELETE",
  );
}

export function cancelSubagent(threadId: string, subagentId: string) {
  return jsonRequest<OkResponse>(
    `/api/threads/${threadId}/subagents/${subagentId}/cancel`,
    "POST",
  );
}
