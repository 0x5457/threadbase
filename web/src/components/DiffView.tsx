import { useComputedColorScheme } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { PatchDiff } from "@pierre/diffs/react";
import { useMemo } from "react";

/**
 * Thin adapter around Pierre's diff renderer. Pierre owns syntax
 * highlighting, line numbers, and the diff layout; this component only
 * supplies the surface tokens so the diff sits inside the thread.
 */
export function DiffView({ patch }: { patch: string }) {
  const scheme = useComputedColorScheme("light");
  const narrow = useMediaQuery("(max-width: 74.99em)");

  const options = useMemo(
    () => ({
      diffStyle: "unified" as const,
      disableFileHeader: true,
      themeType: scheme === "dark" ? ("dark" as const) : ("light" as const),
      overflow: narrow ? ("wrap" as const) : ("scroll" as const),
      diffIndicators: "bars" as const,
      lineDiffType: "word-alt" as const,
      hunkSeparators: "simple" as const,
    }),
    [scheme, narrow],
  );

  return (
    <div className="diff-wrap">
      <PatchDiff patch={patch} options={options} disableWorkerPool />
    </div>
  );
}
