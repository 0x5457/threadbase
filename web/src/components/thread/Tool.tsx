import {
  IconArrowRight,
  IconBrain,
  IconCheck,
  IconChevronDown,
  IconFileText,
  IconLoader2,
  IconPencil,
  IconSearch,
  IconTerminal2,
  IconTool,
  IconTrash,
  IconWorld,
  IconX,
} from "@tabler/icons-react";
import type { TablerIcon } from "@tabler/icons-react";
import type { ToolCallData } from "../../api";
import type { ToolKind } from "../../api/types/ToolKind";
import type { ViewItem } from "../../lib/thread-events";
import { budgetLabel, countLines, exceedsDiffBudget } from "../../lib/diff-size";
import { ImageLightbox } from "./ImageLightbox";
import { formatDuration } from "./format";
import { Suspense, lazy, memo, useMemo, useState } from "react";

/**
 * Pierre's diff renderer drags in the tree-sitter grammar and highlight stack
 * (about 200 KB gzipped, several hundred milliseconds of parse work on a
 * phone). It used to be a static import here, so *opening a thread* paid for
 * it even though most tool rows are shell commands. Loading it on the first
 * row that actually shows a diff keeps the transcript's first paint cheap.
 */
const DiffView = lazy(async () => {
  const module = await import("../DiffView");
  return { default: module.DiffView };
});

function DiffFallback({ patch }: { patch: string }) {
  return <pre className="tool-card-fallback">{patch}</pre>;
}

/** Kind glyphs: the row's verb is text, the glyph is the shortcut. */
const KIND_ICONS: Record<ToolKind, TablerIcon> = {
  read: IconFileText,
  edit: IconPencil,
  delete: IconTrash,
  move: IconArrowRight,
  search: IconSearch,
  execute: IconTerminal2,
  think: IconBrain,
  fetch: IconWorld,
  other: IconTool,
};

/** Short verb per kind. Shell commands carry no verb: the command is the verb. */
const VERBS: Partial<Record<ToolKind, string>> = {
  read: "Read",
  edit: "Edited",
  delete: "Deleted",
  move: "Moved",
  think: "Thought",
};

/** File tools read as filenames; commands and queries stay monospace. */
const FILE_KINDS = new Set<ToolKind>(["read", "edit", "delete", "move"]);

const CARD_LABELS: Record<ToolKind, string> = {
  read: "Read",
  edit: "Diff",
  delete: "Delete",
  move: "Move",
  search: "Search",
  execute: "Shell",
  think: "Thought",
  fetch: "Fetch",
  other: "Tool",
};

function basename(path: string) {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** A single line that says what the tool did, without opening it. */
function describeCall(call: ToolCallData): {
  verb: string | null;
  target: string;
  full: string;
} {
  const path = call.locations[0]?.path ?? null;
  const title = call.title.replace(/\s+/g, " ").trim();
  const verb = VERBS[call.kind] ?? null;
  // A location on a shell/search call is a file it touched, not the thing the
  // row is about; only file tools take their target from `locations`.
  if (FILE_KINDS.has(call.kind) && path) {
    return { verb, target: basename(path), full: path };
  }
  const firstLine = call.title.split("\n").find((line) => line.trim() !== "");
  return {
    verb,
    target: firstLine?.trim() || title || CARD_LABELS[call.kind],
    full: call.title,
  };
}

type OutputBlock =
  | { type: "text"; text: string }
  | { type: "image"; mimeType: string; data?: string; url?: string };

type EditHunk = { removed: string[]; added: string[] };

/** Line slices that actually changed, not the whole old/new blocks. Adapters
 *  often send an edit as `{oldText,newText}` with unchanged context; showing
 *  all of it as -/+ lines would overstate the change. */
function editHunks(input: string | null): EditHunk[] | null {
  if (!input) return null;
  let parsed: { edits?: Array<{ oldText?: string; newText?: string }> };
  try {
    parsed = JSON.parse(input) as typeof parsed;
  } catch {
    return null;
  }
  if (!Array.isArray(parsed.edits)) return null;
  const lines = (text: string | undefined) =>
    text === undefined || text === "" ? [] : text.split("\n");
  const hunks: EditHunk[] = [];
  for (const edit of parsed.edits) {
    const oldLines = lines(edit.oldText);
    const newLines = lines(edit.newText);
    let prefix = 0;
    while (
      prefix < oldLines.length &&
      prefix < newLines.length &&
      oldLines[prefix] === newLines[prefix]
    ) {
      prefix += 1;
    }
    let suffix = 0;
    while (
      suffix < oldLines.length - prefix &&
      suffix < newLines.length - prefix &&
      oldLines[oldLines.length - 1 - suffix] ===
        newLines[newLines.length - 1 - suffix]
    ) {
      suffix += 1;
    }
    const removed = oldLines.slice(prefix, oldLines.length - suffix);
    const added = newLines.slice(prefix, newLines.length - suffix);
    if (removed.length > 0 || added.length > 0) hunks.push({ removed, added });
  }
  return hunks;
}

/** Per-side cap: a generated-file edit can carry thousands of lines. */
const HUNK_LINES = 400;

function HunkLines({ lines, tone }: { lines: string[]; tone: "add" | "del" }) {
  const shown = lines.slice(0, HUNK_LINES);
  return (
    <>
      {shown.map((line, index) => (
        // A hunk is a fixed snapshot for this render and never reorders.
        // react-doctor-disable-next-line react-doctor/no-array-index-as-key
        <div key={`${tone}:${index}:${line}`} className={`tool-edit-line ${tone}`}>
          {tone === "add" ? "+" : "−"} {line}
        </div>
      ))}
      {lines.length > shown.length ? (
        <div className="tool-edit-line more">
          … {lines.length - shown.length} more lines
        </div>
      ) : null}
    </>
  );
}

/** Unwrap ACP tool content: text blocks and images, instead of raw JSON/base64. */
function parseOutput(raw: string | null | undefined): OutputBlock[] | null {
  if (!raw) return null;
  const trimmed = raw.trim();
  if (!trimmed.startsWith("{")) return null;
  let parsed: { content?: unknown };
  try {
    parsed = JSON.parse(trimmed) as typeof parsed;
  } catch {
    return null;
  }
  if (!Array.isArray(parsed.content)) return null;
  const blocks: OutputBlock[] = [];
  for (const item of parsed.content) {
    if (typeof item !== "object" || item === null) continue;
    const block = item as Record<string, unknown>;
    if (block.type === "text" && typeof block.text === "string") {
      blocks.push({ type: "text", text: block.text });
    } else if (
      block.type === "image" &&
      typeof block.mimeType === "string" &&
      (typeof block.url === "string" || typeof block.data === "string")
    ) {
      blocks.push({
        type: "image",
        mimeType: block.mimeType,
        data: typeof block.data === "string" ? block.data : undefined,
        url: typeof block.url === "string" ? block.url : undefined,
      });
    }
  }
  return blocks.length > 0 ? blocks : null;
}

function running(status: ToolCallData["status"]) {
  return status === "pending" || status === "in_progress";
}

function StatusMark({ status }: { status: ToolCallData["status"] }) {
  if (running(status)) {
    return <IconLoader2 size={12} stroke={1.75} className="tool-spinner" />;
  }
  if (status === "failed") {
    return <span className="tool-failed">failed</span>;
  }
  return <IconCheck size={12} stroke={2} className="tool-done" />;
}

function ToolCard({
  call,
  patch,
  patchTooLarge,
  outputText,
  outputParsed,
  outputLineCount,
  edits,
  add,
  del,
}: {
  call: ToolCallData;
  patch: string | null;
  patchTooLarge: boolean;
  outputText: string;
  outputParsed: boolean;
  outputLineCount: number;
  edits: EditHunk[] | null;
  add: number;
  del: number;
}) {
  const duration = Math.max(0, call.updatedAt - call.createdAt);
  const rawOutput = call.output ?? "";
  const meta = [
    add || del ? `+${add} −${del}` : null,
    outputLineCount > 0
      ? outputLineCount > 4_000
        ? "4,000+ lines"
        : `${outputLineCount} ${outputLineCount === 1 ? "line" : "lines"}`
      : null,
    duration > 0 ? formatDuration(duration) : null,
  ].filter((part): part is string => Boolean(part));
  const label = patch || edits?.length ? "Diff" : CARD_LABELS[call.kind];
  const command = call.kind === "execute" ? call.title : null;
  const body = patch ? (
    patchTooLarge ? (
      <p className="tool-card-empty">
        This diff is too large to show inline ({budgetLabel()}). Open the file in
        your editor to review it.
      </p>
    ) : (
      <div className="tool-card-diff">
        <Suspense fallback={<DiffFallback patch={patch} />}>
          <DiffView patch={patch} />
        </Suspense>
      </div>
    )
  ) : edits && edits.length > 0 ? (
    <div className="tool-edits">
      {edits.map((hunk, index) => (
        // Hunks are a fixed snapshot for this render and never reorder.
        // react-doctor-disable-next-line react-doctor/no-array-index-as-key
        <div key={index} className="tool-edit">
          <HunkLines lines={hunk.removed} tone="del" />
          <HunkLines lines={hunk.added} tone="add" />
        </div>
      ))}
    </div>
  ) : outputText ? (
    <pre className="tool-card-output">{outputText}</pre>
  ) : rawOutput.trim() !== "" && !outputParsed ? (
    <pre className="tool-card-output">{rawOutput}</pre>
  ) : call.input ? (
    <pre className="tool-card-output">{call.input}</pre>
  ) : running(call.status) ? (
    <p className="tool-card-empty">Waiting for output…</p>
  ) : (
    <p className="tool-card-empty">No output.</p>
  );

  return (
    <div className="tool-card">
      <div className="tool-card-head">
        <span className="tool-card-kind">{label}</span>
        {meta.length ? (
          <span className="tool-card-meta">{meta.join(" · ")}</span>
        ) : null}
      </div>
      {command ? (
        <pre className="tool-card-command">
          <span aria-hidden>$</span> {command}
        </pre>
      ) : null}
      {body}
      {running(call.status) ? (
        <div className="tool-card-foot">
          <IconLoader2 size={12} stroke={1.75} className="tool-spinner" /> Running
        </div>
      ) : call.status === "failed" ? (
        <div className="tool-card-foot is-failed">
          <IconX size={12} stroke={2} /> Failed
        </div>
      ) : call.kind === "execute" ? (
        <div className="tool-card-foot">
          <IconCheck size={12} stroke={2} /> Success
        </div>
      ) : null}
    </div>
  );
}

export const Tool = memo(function Tool({
  item,
  expanded,
  onToggleExpanded,
  nested = false,
}: {
  item: Extract<ViewItem, { kind: "tool" }>;
  /** The details card is open; kept in React so unmounting keeps it open. */
  expanded: boolean;
  onToggleExpanded: () => void;
  /** Rendered inside a run: the group supplies the rail and the indent. */
  nested?: boolean;
}) {
  const [lightbox, setLightbox] = useState<string | null>(null);
  const call: ToolCallData = item.call;
  const patch = call.diff?.patch ?? null;
  // A tool can write a whole generated file. Handing that patch to the diff
  // renderer would mount the entire file as DOM.
  const patchTooLarge = patch !== null && exceedsDiffBudget(patch);
  const edits = useMemo(
    () => (!patch && call.kind === "edit" ? editHunks(call.input) : null),
    [patch, call.kind, call.input],
  );
  const editStats = edits?.reduce(
    (total, hunk) => ({
      add: total.add + hunk.added.length,
      del: total.del + hunk.removed.length,
    }),
    { add: 0, del: 0 },
  );
  const output = useMemo(() => parseOutput(call.output), [call.output]);
  const outputText =
    output
      ?.filter(
        (block): block is Extract<OutputBlock, { type: "text" }> =>
          block.type === "text",
      )
      .map((block) => block.text)
      .join("\n") ?? "";
  const outputImages =
    output?.filter(
      (block): block is Extract<OutputBlock, { type: "image" }> =>
        block.type === "image",
    ) ?? [];
  // ACP content is counted as text; shell stdout is counted raw.
  const countedOutput =
    outputText || (output === null ? (call.output ?? "") : "");
  const outputLineCount =
    countedOutput.trim() === "" ? 0 : countLines(countedOutput, 4_000);
  // Image results are the content: they stay visible under the row, while the
  // card carries any text or input the tool returned.
  const hasDetails = Boolean(
    patch ||
      edits?.length ||
      outputText ||
      call.input ||
      (!output && (call.output ?? "").trim() !== ""),
  );
  const { verb, target, full } = describeCall(call);
  const Icon = KIND_ICONS[call.kind] ?? IconTool;
  const add = call.diff?.additions ?? editStats?.add ?? 0;
  const del = call.diff?.deletions ?? editStats?.del ?? 0;
  const isFile = FILE_KINDS.has(call.kind);

  const rowContent = (
    <>
      <span className="tool-icon" aria-hidden>
        <Icon size={15} stroke={1.5} />
      </span>
      <span className="tool-text">
        {verb ? <span className="tool-verb">{verb}</span> : null}
        <span
          className={isFile ? "tool-target is-file" : "tool-target"}
          title={full}
        >
          {target}
        </span>
        {add || del ? (
          <span className="tool-diff-stats">
            {add ? <span className="add">+{add}</span> : null}
            {del ? <span className="del">−{del}</span> : null}
          </span>
        ) : null}
      </span>
      <span className="tool-end">
        <StatusMark status={call.status} />
        {hasDetails ? (
          <IconChevronDown
            size={12}
            stroke={1.5}
            className="tool-chevron"
            aria-hidden
          />
        ) : null}
      </span>
    </>
  );

  const row = hasDetails ? (
    <button
      type="button"
      className={`tool-row${call.status === "failed" ? " is-failed" : ""}`}
      aria-expanded={expanded}
      onClick={onToggleExpanded}
    >
      {rowContent}
    </button>
  ) : (
    <div className={`tool-row${call.status === "failed" ? " is-failed" : ""}`}>
      {rowContent}
    </div>
  );

  const content = (
    <>
      {row}
      {outputImages.length > 0 ? (
        <div className="tool-images">
          {outputImages.map((image, index) => {
            // Replays carry a URL so a screenshot-heavy turn does not ship its
            // bytes again; a live event may still carry the data inline.
            const src =
              image.url ?? `data:${image.mimeType};base64,${image.data ?? ""}`;
            const size = image.data
              ? ` · ${Math.round((image.data.length * 3) / 4 / 1024)} KB`
              : "";
            return (
              <button
                type="button"
                // Tool output images are a fixed, non-reorderable snapshot.
                // react-doctor-disable-next-line react-doctor/no-array-index-as-key
                key={`${image.mimeType}:${index}`}
                className="tool-image"
                aria-label={`Preview image output (${image.mimeType})`}
                onClick={() => setLightbox(src)}
                title={`${image.mimeType}${size}`}
              >
                <img src={src} alt="" loading="lazy" />
              </button>
            );
          })}
        </div>
      ) : null}
      {expanded && hasDetails ? (
        <ToolCard
          call={call}
          patch={patch}
          patchTooLarge={patchTooLarge}
          outputText={outputText}
          outputParsed={output !== null}
          outputLineCount={outputLineCount}
          edits={edits}
          add={add}
          del={del}
        />
      ) : null}
      {lightbox ? (
        <ImageLightbox src={lightbox} onClose={() => setLightbox(null)} />
      ) : null}
    </>
  );

  if (nested) {
    return <div className="tool-call">{content}</div>;
  }

  return (
    <div className="block tool-block">
      <div className="block-gutter" />
      <div className="block-body">{content}</div>
    </div>
  );
});
