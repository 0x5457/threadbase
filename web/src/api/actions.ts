import { jsonRequest, request } from "./client";
import type { AgentConfigResponse } from "./types/AgentConfigResponse";
import type { ApprovalDecision } from "./types/ApprovalDecision";
import type { ApprovalDto } from "./types/ApprovalDto";
import type { ApprovalResolvedResponse } from "./types/ApprovalResolvedResponse";
import type { CommentAnchor } from "./types/CommentAnchor";
import type { CommentSide } from "./types/CommentSide";
import type { CommentDto } from "./types/CommentDto";
import type { CommentResponse } from "./types/CommentResponse";
import type { CommentStatus } from "./types/CommentStatus";
import type { ElicitationAction } from "./types/ElicitationAction";
import type { ElicitationDto } from "./types/ElicitationDto";
import type { ElicitationResolvedResponse } from "./types/ElicitationResolvedResponse";
import type { QueueItemDto } from "./types/QueueItemDto";
import type { SubagentDto } from "./types/SubagentDto";
import type { ToolCallData } from "./types/ToolCallData";

export type Approval = ApprovalDto;
export type Elicitation = ElicitationDto;
export type Comment = CommentDto;
export type QueueItem = QueueItemDto;
export type Subagent = SubagentDto;
export type { CommentAnchor };
export type { CommentSide };
export type { ToolCallData };

export function getAgentConfig(threadId: string) {
  return request<AgentConfigResponse>(`/api/threads/${threadId}/agent-config`);
}

/** Starts the bound adapter if needed so it can advertise its options. */
export function ensureAgentConfig(threadId: string) {
  return request<AgentConfigResponse>(
    `/api/threads/${threadId}/agent-config?ensure=true`,
  );
}

export function decideApproval(
  approvalId: string,
  decision: ApprovalDecision,
  note?: string,
) {
  return jsonRequest<ApprovalResolvedResponse>(
    `/api/approvals/${approvalId}/decide`,
    "POST",
    { decision, note: note ?? null },
  );
}

export function respondElicitation(
  elicitationId: string,
  action: ElicitationAction,
  values?: unknown,
) {
  return jsonRequest<ElicitationResolvedResponse>(
    `/api/elicitations/${elicitationId}/respond`,
    "POST",
    { action, values: values ?? null },
  );
}

export function createComment(
  threadId: string,
  body: string,
  anchor: CommentAnchor,
  send = false,
) {
  return jsonRequest<CommentResponse>(
    `/api/threads/${threadId}/comments`,
    "POST",
    { body, anchor, parentId: null, send },
  );
}

export function updateComment(
  commentId: string,
  patch: { status?: CommentStatus; body?: string },
) {
  return jsonRequest<CommentResponse>(`/api/comments/${commentId}`, "PATCH", patch);
}

export function deleteComment(commentId: string) {
  return jsonRequest<{ ok: boolean }>(`/api/comments/${commentId}`, "DELETE");
}

export function sendComment(commentId: string) {
  return jsonRequest<CommentResponse>(`/api/comments/${commentId}/send`, "POST");
}
