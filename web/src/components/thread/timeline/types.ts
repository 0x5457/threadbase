/** Projection types for the thread timeline. */
import type { ApprovalDto } from "@/api/types/ApprovalDto";
import type { ElicitationAction } from "@/api/types/ElicitationAction";
import type { ElicitationDto } from "@/api/types/ElicitationDto";
import type { MessageDto } from "@/api/types/MessageDto";
import type { PlanEntry } from "@/api/types/PlanEntry";
import type { ToolCallData } from "@/api/types/ToolCallData";

export interface MessageItem {
  kind: "message";
  key: string;
  messageId: string;
  seq?: number;
  role: "user" | "agent";
  text: string;
  author: MessageDto["author"];
  authors: MessageDto["authors"];
  createdAt?: number | bigint;
}

export interface ToolItem {
  kind: "tool";
  key: string;
  call: ToolCallData;
}

export interface ToolGroupItem {
  kind: "tool_group";
  key: string;
  calls: ToolCallData[];
}

export interface PlanItem {
  kind: "plan";
  key: string;
  entries: PlanEntry[];
}

export interface ElicitationItem {
  kind: "elicitation";
  key: string;
  elicitation: ElicitationDto;
  resolvedAction: ElicitationAction | null;
}

export interface ApprovalItem {
  kind: "approval";
  key: string;
  approval: ApprovalDto;
  resolved: boolean;
  decision: string | null;
}

export interface SubagentItem {
  kind: "subagent";
  key: string;
  subagent: import("@/api/types/SubagentDto").SubagentDto;
}

export interface AgentMessageItem {
  kind: "agent_message";
  key: string;
  direction: "sent" | "received";
  from: string;
  to: string;
  text: string;
}

export interface NoticeItem {
  kind: "notice";
  key: string;
  tone: "muted" | "danger" | "warning" | "info";
  label: string;
  text: string;
}

export type TimelineItem =
  | MessageItem
  | ToolItem
  | ToolGroupItem
  | PlanItem
  | ElicitationItem
  | ApprovalItem
  | SubagentItem
  | AgentMessageItem
  | NoticeItem;
