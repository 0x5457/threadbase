import { useComputedColorScheme } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { PatchDiff } from "@pierre/diffs/react";
import { memo, useMemo } from "react";
import { useEffectiveDiffWrap } from "../lib/diff-wrap";

/**
 * Thin adapter around Pierre's diff renderer. Pierre owns syntax
 * highlighting, line numbers, and the diff layout; this component only
 * supplies the surface tokens so the diff sits inside the thread.
 */
export const DiffView = memo(function DiffView({
  patch,
}: {
  patch: string;
}) {
  const scheme = useComputedColorScheme("light");
  const narrow = useMediaQuery("(max-width: 74.99em)");
  const wrap = useEffectiveDiffWrap(narrow);

  const options = useMemo(
    () => ({
      diffStyle: "unified" as const,
      disableFileHeader: true,
      themeType: scheme === "dark" ? ("dark" as const) : ("light" as const),
      overflow: wrap ? ("wrap" as const) : ("scroll" as const),
      diffIndicators: "bars" as const,
      lineDiffType: "word-alt" as const,
      hunkSeparators: "simple" as const,
    }),
    [scheme, wrap],
  );

  return (
    <div className="diff-wrap">
      <PatchDiff patch={patch} options={options} disableWorkerPool />
    </div>
  );
});
