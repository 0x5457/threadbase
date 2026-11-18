import { ApiError, jsonRequest, request } from "./client";
import type { AttachmentDto } from "./types/AttachmentDto";
import type { ChangesDto } from "./types/ChangesDto";
import type { CreateThreadRequest } from "./types/CreateThreadRequest";
import type { DeliveryMode } from "./types/DeliveryMode";
import type { EventEnvelope } from "./types/EventEnvelope";
import type { FileChangeDto } from "./types/FileChangeDto";
import type { FileDiffDto } from "./types/FileDiffDto";
import type { ForkThreadRequest } from "./types/ForkThreadRequest";
import type { MergeBackResponse } from "./types/MergeBackResponse";
import type { PlacementRequest } from "./types/PlacementRequest";
import type { RewindRequest } from "./types/RewindRequest";
import type { OkResponse } from "./types/OkResponse";
import type { OlderEventsDto } from "./types/OlderEventsDto";
import type { SendMessageResponse } from "./types/SendMessageResponse";
import type { ThreadBundleDto } from "./types/ThreadBundleDto";
import type { ThreadDto } from "./types/ThreadDto";
import type { ThreadHistoryResponse } from "./types/ThreadHistoryResponse";
import type { ThreadGoalResponse } from "./types/ThreadGoalResponse";
import type { CommitSummaryDto } from "./types/CommitSummaryDto";
import type { ThreadResponse } from "./types/ThreadResponse";
import type { ThreadsResponse } from "./types/ThreadsResponse";
import type { ThreadWatchRequest } from "./types/ThreadWatchRequest";
import type { ThreadWatchResponse } from "./types/ThreadWatchResponse";
import type { UpdateThreadRequest } from "./types/UpdateThreadRequest";
import type { WatchState } from "./types/WatchState";

export type ThreadSummary = ThreadDto;
type ThreadBundle = ThreadBundleDto;
export type ThreadChanges = ChangesDto;
export type CommitSummary = CommitSummaryDto;
export type { MergeBackResponse } from "./types/MergeBackResponse";
export type FileChange = FileChangeDto;
export type FileDiff = FileDiffDto;
export type ThreadEvent = EventEnvelope;
type ThreadAttachment = AttachmentDto;

type UpdateThreadPatch = Partial<
  Pick<
    UpdateThreadRequest,
    "title" | "pinned" | "archived" | "model" | "effort" | "autoSettle" | "permissionMode"
  >
>;

interface CreateThreadInput {
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

interface Baseline {
  kind?: ChangesDto["baseline"]["kind"];
  commit?: string | null;
}

export function listAllThreads() {
  return request<ThreadsResponse>("/api/threads");
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

/** Checkpoint commits newer than the thread's base, newest first. */
export function getThreadHistory(threadId: string) {
  return request<ThreadHistoryResponse>(`/api/threads/${threadId}/commits`);
}

/**
 * Goal progress the agent keeps in this thread's workspace (`null` for
 * adapters without goals). Cheap: one small file read on the server.
 */
export function getThreadGoal(threadId: string) {
  return request<ThreadGoalResponse>(`/api/threads/${threadId}/goal`);
}

/** Everything one turn changed: from its checkpoint to the next one. */
export function getTurnChanges(threadId: string, turnId: string) {
  return request<ChangesDto>(`/api/threads/${threadId}/turns/${turnId}/changes`);
}

export function getTurnFileDiff(
  threadId: string,
  turnId: string,
  path: string,
) {
  const params = new URLSearchParams({ path });
  return request<FileDiffDto>(
    `/api/threads/${threadId}/turns/${turnId}/file?${params.toString()}`,
  );
}

/** One commit's own diff: parent to commit. */
export function getCommitChanges(threadId: string, sha: string) {
  return request<ChangesDto>(`/api/threads/${threadId}/commits/${sha}`);
}

export function getCommitFileDiff(threadId: string, sha: string, path: string) {
  const params = new URLSearchParams({ path });
  return request<FileDiffDto>(
    `/api/threads/${threadId}/commits/${sha}/file?${params.toString()}`,
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

/**
 * Move a thread between shelves. `action` is a command id from the published
 * placement machine, so the machine stays the only list of legal moves.
 */
export function setThreadPlacement(
  threadId: string,
  action: string,
  until?: number | null,
) {
  const body: PlacementRequest = { action, until: until ?? null };
  return jsonRequest<ThreadResponse>(
    `/api/threads/${threadId}/placement`,
    "POST",
    body,
  );
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

/**
 * Replace a user prompt and discard everything after it. The server restarts
 * the agent in a fresh ACP session, so the edited turn runs against the
 * remaining transcript with no memory of the removed branch.
 */
export function editThreadMessage(
  threadId: string,
  messageId: string,
  text: string,
) {
  return jsonRequest<SendMessageResponse>(
    `/api/threads/${threadId}/messages/${messageId}/edit`,
    "POST",
    { text },
  );
}

/**
 * Discard one prompt's turn and every later one. The server resets the worktree
 * to the checkpoint taken before that turn and clears the agent's context.
 */
export function rewindThread(threadId: string, messageId: string) {
  const body: RewindRequest = { messageId };
  return jsonRequest<OkResponse>(
    `/api/threads/${threadId}/rewind`,
    "POST",
    body,
  );
}

/** Restart the adapter in a fresh ACP session. */
export function restartThread(threadId: string) {
  return jsonRequest<OkResponse>(`/api/threads/${threadId}/restart`, "POST");
}

export function forkThread(
  threadId: string,
  options?: { messageId?: string; title?: string },
) {
  const body: ForkThreadRequest = {
    title: options?.title ?? null,
    messageId: options?.messageId ?? null,
  };
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

interface UploadAttachmentOptions {
  onProgress?: (loaded: number, total: number) => void;
  signal?: AbortSignal;
}

/**
 * Upload one pasted/picked file; the prompt references it by id.
 *
 * XHR, not fetch, because an upload can be 25 MB and the composer needs the
 * byte count and a real abort. The response shape is unchanged.
 */
export function uploadAttachment(
  threadId: string,
  file: Blob,
  options: UploadAttachmentOptions = {},
): Promise<ThreadAttachment> {
  const name =
    file instanceof File && file.name ? file.name : "attachment";
  return new Promise<ThreadAttachment>((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    const abort = () => xhr.abort();
    const cleanup = () => {
      options.signal?.removeEventListener("abort", abort);
    };
    xhr.open("POST", `/api/threads/${threadId}/attachments`);
    xhr.withCredentials = true;
    xhr.setRequestHeader("x-requested-with", "threadbase");
    xhr.setRequestHeader(
      "content-type",
      file.type || "application/octet-stream",
    );
    xhr.setRequestHeader("x-attachment-name", encodeURIComponent(name));
    xhr.upload.onprogress = (event) => {
      if (event.lengthComputable) {
        options.onProgress?.(event.loaded, event.total);
      }
    };
    xhr.onload = () => {
      cleanup();
      let body: unknown = null;
      try {
        body = JSON.parse(xhr.responseText);
      } catch {
        body = null;
      }
      if (xhr.status >= 200 && xhr.status < 300) {
        resolve(body as ThreadAttachment);
        return;
      }
      const error = (body as {
        error?: { code?: string; message?: string };
      } | null)?.error;
      reject(
        new ApiError(
          error?.message ?? `Upload failed (${xhr.status})`,
          xhr.status,
          error?.code ?? null,
        ),
      );
    };
    xhr.onerror = () => {
      cleanup();
      reject(new Error("Upload failed"));
    };
    xhr.onabort = () => {
      cleanup();
      reject(new DOMException("Upload cancelled", "AbortError"));
    };
    if (options.signal?.aborted) {
      cleanup();
      reject(new DOMException("Upload cancelled", "AbortError"));
      return;
    }
    options.signal?.addEventListener("abort", abort, { once: true });
    xhr.send(file);
  });
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

/** One subagent's own transcript, when the adapter leaves it on disk. */
export function getSubagentLog(threadId: string, subagentId: string) {
  return request<import("./types/SubagentLogResponse").SubagentLogResponse>(
    `/api/threads/${threadId}/subagents/${encodeURIComponent(subagentId)}/log`,
  );
}
