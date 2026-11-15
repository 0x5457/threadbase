import { binaryRequest, jsonRequest, request } from "./client";
import type { AttachmentDto } from "./types/AttachmentDto";
import type { ChangesDto } from "./types/ChangesDto";
import type { CreateThreadRequest } from "./types/CreateThreadRequest";
import type { DeliveryMode } from "./types/DeliveryMode";
import type { EventEnvelope } from "./types/EventEnvelope";
import type { FileChangeDto } from "./types/FileChangeDto";
import type { FileDiffDto } from "./types/FileDiffDto";
import type { ForkThreadRequest } from "./types/ForkThreadRequest";
import type { MergeBackResponse } from "./types/MergeBackResponse";
import type { OkResponse } from "./types/OkResponse";
import type { OlderEventsDto } from "./types/OlderEventsDto";
import type { SendMessageResponse } from "./types/SendMessageResponse";
import type { ThreadBundleDto } from "./types/ThreadBundleDto";
import type { ThreadDto } from "./types/ThreadDto";
import type { ThreadResponse } from "./types/ThreadResponse";
import type { ThreadsResponse } from "./types/ThreadsResponse";
import type { ThreadWatchRequest } from "./types/ThreadWatchRequest";
import type { ThreadWatchResponse } from "./types/ThreadWatchResponse";
import type { UpdateThreadRequest } from "./types/UpdateThreadRequest";
import type { WatchState } from "./types/WatchState";

export type ThreadSummary = ThreadDto;
export type ThreadBundle = ThreadBundleDto;
export type ThreadChanges = ChangesDto;
export type { MergeBackResponse } from "./types/MergeBackResponse";
export type FileChange = FileChangeDto;
export type FileDiff = FileDiffDto;
export type ThreadEvent = EventEnvelope;
export type ThreadAttachment = AttachmentDto;

export type UpdateThreadPatch = Partial<
  Pick<
    UpdateThreadRequest,
    "title" | "pinned" | "archived" | "model" | "effort"
  >
>;

export interface CreateThreadInput {
  /** Optional; the server names the thread from the first message when empty. */
  title?: string;
  agent: string;
  model?: string | null;
  effort?: string | null;
  baseBranch: string;
  initialPrompt?: string | null;
  /** Give the thread its own worktree; false runs in the project directory. */
  worktree?: boolean;
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

export function getOlderEvents(threadId: string, beforeSeq: number) {
  return request<OlderEventsDto>(
    `/api/threads/${threadId}/events/older?before=${beforeSeq}`,
  );
}

export function getThread(threadId: string) {
  return request<ThreadResponse>(`/api/threads/${threadId}`);
}

/** The transcript poll passes `changes: false` to skip the server-side git diff. */
export function getThreadBundle(
  threadId: string,
  options: { changes?: boolean } = {},
) {
  const suffix = options.changes === false ? "?changes=false" : "";
  return request<{ bundle: ThreadBundle }>(
    `/api/threads/${threadId}/bundle${suffix}`,
  );
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
    title: input.title?.trim() || null,
    agent: input.agent,
    model: input.model ?? null,
    effort: input.effort ?? null,
    baseBranch: input.baseBranch,
    initialPrompt: input.initialPrompt ?? null,
    worktree: input.worktree ?? null,
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

export function retryThreadSetup(threadId: string) {
  return jsonRequest<OkResponse>(`/api/threads/${threadId}/setup`, "POST");
}

export function getThreadWatch(threadId: string) {
  return request<ThreadWatchResponse>(`/api/threads/${threadId}/watch`);
}

export function setThreadWatch(threadId: string, state: WatchState) {
  const body: ThreadWatchRequest = { state };
  return jsonRequest<ThreadWatchResponse>(
    `/api/threads/${threadId}/watch`,
    "POST",
    body,
  );
}

/** Stop the running turn. Legal according to the thread machine's commands. */
export function cancelThread(threadId: string) {
  return jsonRequest<OkResponse>(`/api/threads/${threadId}/cancel`, "POST");
}

/** Restart the adapter in a fresh ACP session. */
export function restartThread(threadId: string) {
  return jsonRequest<OkResponse>(`/api/threads/${threadId}/restart`, "POST");
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
  attachmentIds?: string[],
) {
  return jsonRequest<SendMessageResponse>(
    `/api/threads/${threadId}/messages`,
    "POST",
    {
      text,
      delivery: delivery ?? null,
      fromDraft: false,
      attachmentIds: attachmentIds?.length ? attachmentIds : null,
    },
  );
}

/** Upload one pasted image; the prompt references it by id. */
export function uploadAttachment(threadId: string, file: Blob) {
  return binaryRequest<ThreadAttachment>(
    `/api/threads/${threadId}/attachments`,
    file,
  );
}

/** Send the accumulated CRDT draft blocks, plus any attached images. */
export function sendDraft(
  threadId: string,
  delivery?: DeliveryMode,
  attachmentIds?: string[],
) {
  return jsonRequest<SendMessageResponse>(
    `/api/threads/${threadId}/messages`,
    "POST",
    {
      text: "",
      delivery: delivery ?? null,
      fromDraft: true,
      attachmentIds: attachmentIds?.length ? attachmentIds : null,
    },
  );
}

export function dropQueueItem(threadId: string, itemId: string) {
  return jsonRequest<OkResponse>(
    `/api/threads/${threadId}/queue/${itemId}`,
    "DELETE",
  );
}

export function restoreQueueItem(threadId: string, itemId: string) {
  return jsonRequest<OkResponse>(
    `/api/threads/${threadId}/queue/${itemId}/restore`,
    "POST",
  );
}

/** Rewrite a message that is still waiting; sent rows are immutable. */
export function updateQueueItem(threadId: string, itemId: string, text: string) {
  return jsonRequest<OkResponse>(
    `/api/threads/${threadId}/queue/${itemId}`,
    "PATCH",
    { text },
  );
}

/** Deliver a queued message into the running turn instead of waiting. */
export function steerQueueItem(threadId: string, itemId: string) {
  return jsonRequest<OkResponse>(
    `/api/threads/${threadId}/queue/${itemId}/steer`,
    "POST",
  );
}

export function reorderQueue(threadId: string, itemIds: string[]) {
  return jsonRequest<OkResponse>(
    `/api/threads/${threadId}/queue/order`,
    "POST",
    { itemIds },
  );
}

export function cancelSubagent(threadId: string, subagentId: string) {
  return jsonRequest<OkResponse>(
    `/api/threads/${threadId}/subagents/${subagentId}/cancel`,
    "POST",
  );
}

/** One subagent's own transcript, when the adapter leaves it on disk. */
export function getSubagentLog(threadId: string, subagentId: string) {
  return request<import("./types/SubagentLogResponse").SubagentLogResponse>(
    `/api/threads/${threadId}/subagents/${encodeURIComponent(subagentId)}/log`,
  );
}
