import { Group, Text } from "@mantine/core";
import { IconChevronDown } from "@tabler/icons-react";
import type { ToolCallData } from "../../api";
import type { ToolKind } from "../../api/types/ToolKind";
import type { ViewItem } from "../../lib/thread-events";
import { DiffView } from "../DiffView";
import { StatusDot } from "./StatusDot";
import { describeTitle } from "./format";
import { memo } from "react";

/** Short verb per tool kind: the head names the action, the preview the content. */
const VERBS: Record<ToolKind, string> = {
  read: "Read",
  edit: "Edit",
  delete: "Delete",
  move: "Move",
  search: "Search",
  execute: "Shell",
  think: "Think",
  fetch: "Fetch",
  other: "Tool",
};

type PreviewLine = { text: string; tone?: "add" | "del" };

const PREVIEW_LINES = 3;
const EDIT_LINES = 2;
const DIFF_PREVIEW_LINES = 4;
const PREVIEW_CHARS = 160;

function clip(text: string) {
  return text.length > PREVIEW_CHARS ? `${text.slice(0, PREVIEW_CHARS)}…` : text;
}

/** Mini diff from the edit tool's `{path, edits:[{oldText,newText}]}` input. */
function editPreview(input: string | null): { lines: PreviewLine[]; hidden: number } | null {
  if (!input) return null;
  let parsed: { edits?: Array<{ oldText?: string; newText?: string }> };
  try {
    parsed = JSON.parse(input) as typeof parsed;
  } catch {
    return null;
  }
  const first = parsed.edits?.[0];
  if (!first) return null;
  const removed = (first.oldText ?? "").split("\n").filter((line) => line.trim() !== "");
  const added = (first.newText ?? "").split("\n").filter((line) => line.trim() !== "");
  const lines: PreviewLine[] = [
    ...removed
      .slice(0, EDIT_LINES)
      .map((line) => ({ text: clip(`- ${line.trimEnd()}`), tone: "del" as const })),
    ...added
      .slice(0, EDIT_LINES)
      .map((line) => ({ text: clip(`+ ${line.trimEnd()}`), tone: "add" as const })),
  ];
  if (lines.length === 0) return null;
  const hidden =
    Math.max(0, removed.length - EDIT_LINES) +
    Math.max(0, added.length - EDIT_LINES) +
    Math.max(0, (parsed.edits?.length ?? 1) - 1);
  return { lines, hidden };
}

/** The opening lines of whatever the tool acts on, so a collapsed run still
 *  says what the agent is doing instead of only that it did something. */
function toolPreview(call: ToolCallData): { lines: PreviewLine[]; hidden: number } | null {
  if (call.kind === "edit") {
    const diff = editPreview(call.input);
    if (diff) return diff;
  }
  if (call.kind === "execute") {
    const lines = call.title
      .split("\n")
      .map((line) => line.trimEnd())
      .filter((line) => line.trim() !== "");
    if (lines.length === 0) return null;
    return {
      lines: lines.slice(0, PREVIEW_LINES).map((text) => ({ text: clip(text) })),
      hidden: Math.max(0, lines.length - PREVIEW_LINES),
    };
  }
  // Read/other keep the head path; only surface input when it adds context.
  if (call.kind === "read" || !call.input) return null;
  const lines = call.input
    .split("\n")
    .map((line) => line.trimEnd())
    .filter((line) => line.trim() !== "");
  if (lines.length === 0) return null;
  return {
    lines: lines.slice(0, PREVIEW_LINES).map((text) => ({ text: clip(text) })),
    hidden: Math.max(0, lines.length - PREVIEW_LINES),
  };
}

/** First lines of a real patch: hunks folded into a few +/- lines, the full
 *  diff stays behind the toggle. */
function diffTextPreview(patch: string): { lines: PreviewLine[]; hidden: number } | null {
  const rows = patch
    .split("\n")
    .filter(
      (line) =>
        line.trim() !== "" &&
        !line.startsWith("@@") &&
        !line.startsWith("--- ") &&
        !line.startsWith("+++ ") &&
        !line.startsWith("diff ") &&
        !line.startsWith("index ") &&
        !line.startsWith("\\"),
    );
  if (rows.length === 0) return null;
  // Changed lines say more than context: show those first, context only when
  // the patch has no +/- rows at all.
  const changed = rows.filter(
    (line) => line.startsWith("+") || line.startsWith("-"),
  );
  const shown = changed.length > 0 ? changed : rows;
  return {
    lines: shown.slice(0, DIFF_PREVIEW_LINES).map((line) => ({
      text: clip(line),
      tone: line.startsWith("+")
        ? ("add" as const)
        : line.startsWith("-")
          ? ("del" as const)
          : undefined,
    })),
    hidden: Math.max(0, shown.length - DIFF_PREVIEW_LINES),
  };
}

export const Tool = memo(function Tool({
  item,
  expanded,
  onToggleExpanded,
}: {
  item: Extract<ViewItem, { kind: "tool" }>;
  /** The details body is open; kept in React so unmounting keeps it open. */
  expanded: boolean;
  onToggleExpanded: () => void;
}) {
  const call: ToolCallData = item.call;
  const path = call.locations[0]?.path;
  const preview = call.diff?.patch
    ? diffTextPreview(call.diff.patch)
    : toolPreview(call);
  return (
    <div className="block">
      <div className="block-gutter" />
      <div className="block-body">
        <Group gap={6} className="tool-head" wrap="nowrap">
          <Text size="sm" title={call.title}>
            <span className="verb">{VERBS[call.kind] ?? describeTitle(call.title)}</span>
            {path ? (
              <>
                {" "}
                <span className="path">{path}</span>
              </>
            ) : null}
          </Text>
          <StatusDot status={call.status} />
        </Group>
        {preview ? (
          <div className="tool-lines">
            {preview.lines.map((line, index) => (
              <div
                key={index}
                className={line.tone ? `tool-line ${line.tone}` : "tool-line"}
              >
                {line.text}
              </div>
            ))}
            {preview.hidden > 0 ? (
              <div className="tool-line more">… {preview.hidden} more lines</div>
            ) : null}
          </div>
        ) : null}
        {call.diff?.patch || call.input || call.output ? (
          <details
            className="tool-details"
            open={expanded}
            onToggle={(event) => {
              if (event.currentTarget.open !== expanded) onToggleExpanded();
            }}
          >
            <summary>
              {call.diff?.patch ? "Full diff" : "Details"}{" "}
              <IconChevronDown size={11} stroke={1.5} className="chevron" />
            </summary>
            {expanded ? (
              <>
                {call.diff?.patch ? <DiffView patch={call.diff.patch} /> : null}
                {!call.diff?.patch && call.input ? (
                  <pre className="cmd">{call.input}</pre>
                ) : null}
                {!call.diff?.patch && call.output ? (
                  <pre className="cmd">{call.output}</pre>
                ) : null}
              </>
            ) : null}
          </details>
        ) : null}
      </div>
    </div>
  );
});
