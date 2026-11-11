/** Tool kind → icon and status → colour, shared by every tool surface. */
import {
  Brain,
  FileSearch,
  FolderInput,
  Globe,
  PencilLine,
  Search,
  Terminal,
  Trash2,
  Wrench,
} from "lucide-react";
import type { ToolCallData } from "@/api/types/ToolCallData";
import type { ToolKind } from "@/api/types/ToolKind";

export const TOOL_ICONS: Record<ToolKind, typeof Wrench> = {
  read: FileSearch,
  edit: PencilLine,
  delete: Trash2,
  move: FolderInput,
  search: Search,
  execute: Terminal,
  think: Brain,
  fetch: Globe,
  other: Wrench,
};

export function toolStatusColor(
  status: ToolCallData["status"],
): "blue" | "green" | "red" {
  switch (status) {
    case "pending":
    case "in_progress":
      return "blue";
    case "completed":
      return "green";
    case "failed":
      return "red";
  }
}
