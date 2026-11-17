import { Text, useComputedColorScheme } from "@mantine/core";
import { PatchDiff } from "@pierre/diffs/react";
import type {
  AnnotationSide,
  DiffLineAnnotation,
} from "@pierre/diffs/react";
import { memo, useMemo, type ReactNode } from "react";
import type { Comment } from "../api";
import { budgetLabel, exceedsDiffBudget } from "../lib/diff-size";
import { useDiffWrap } from "../lib/diff-wrap";

/** Metadata carried by every line annotation in the changes panel. */
export interface DiffCommentMeta {
  /** `${hunkId}:${side}:${line}`; also identifies the draft composer. */
  key: string;
  comments: Comment[];
  draft: boolean;
}

/**
 * Thin adapter around Pierre's diff renderer. Pierre owns syntax
 * highlighting, line numbers, and the diff layout; this component only
 * supplies the surface tokens so the diff sits inside the thread, plus the
 * line click that starts a review comment.
 */
export const DiffView = memo(function DiffView({
  patch,
  annotations,
  renderAnnotation,
  onLineNumberClick,
  commentable = false,
}: {
  patch: string;
  annotations?: DiffLineAnnotation<DiffCommentMeta>[];
  renderAnnotation?: (annotation: DiffLineAnnotation<DiffCommentMeta>) => ReactNode;
  onLineNumberClick?: (side: AnnotationSide, lineNumber: number) => void;
  /** Line commenting is only offered on the base-branch diff. */
  commentable?: boolean;
}) {
  const scheme = useComputedColorScheme("light");
  const { ref, wrap } = useDiffWrap<HTMLDivElement>();

  const options = useMemo(
    () => ({
      diffStyle: "unified" as const,
      disableFileHeader: true,
      themeType: scheme === "dark" ? ("dark" as const) : ("light" as const),
      overflow: wrap ? ("wrap" as const) : ("scroll" as const),
      diffIndicators: "bars" as const,
      lineDiffType: "word-alt" as const,
      hunkSeparators: "simple" as const,
      lineHoverHighlight: commentable ? ("number" as const) : ("disabled" as const),
      onLineNumberClick: commentable
        ? (props: { annotationSide: AnnotationSide; lineNumber: number }) => {
            onLineNumberClick?.(props.annotationSide, props.lineNumber);
          }
        : undefined,
    }),
    [scheme, wrap, commentable, onLineNumberClick],
  );

  // Refuse oversized patches here as well as in the panel's query gate: an
  // inline tool diff arrives inside the event payload and never passes the
  // file-list check. Rendering it would freeze the timeline for good.
  if (exceedsDiffBudget(patch)) {
    return (
      <div className="diff-wrap diff-too-large">
        <Text size="sm" c="dimmed">
          Diff is too large to show in the app ({budgetLabel()}).
        </Text>
        <Text size="xs" c="dimmed">
          Open the file in your editor to review it.
        </Text>
      </div>
    );
  }

  return (
    <div className="diff-wrap" ref={ref}>
      <PatchDiff
        patch={patch}
        options={options}
        lineAnnotations={annotations}
        renderAnnotation={renderAnnotation}
        disableWorkerPool
      />
    </div>
  );
});
