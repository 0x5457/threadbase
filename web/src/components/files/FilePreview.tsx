import {
  ActionIcon,
  Text,
  useComputedColorScheme,
} from "@mantine/core";
import { File as PierreFile } from "@pierre/diffs/react";
import { IconArrowLeft } from "@tabler/icons-react";
import { useQuery } from "@tanstack/react-query";
import { useMemo, type CSSProperties } from "react";
import { readFile } from "../../api";
import { useDelayedFlag } from "../../lib/use-delayed-flag";
import { LoadError } from "../loading/LoadError";
import { ListSkeleton } from "../loading/Skeleton";
import { baseName, directoryOf, formatSize } from "./paths";

const fileNameStyle: CSSProperties = {
  flex: "0 1 auto",
  minWidth: 0,
  overflow: "hidden",
  fontFamily: "var(--mono)",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
};

export function FilePreview({
  threadId,
  path,
  onBack,
}: {
  threadId: string;
  path: string;
  onBack: () => void;
}) {
  const scheme = useComputedColorScheme("light");
  const file = useQuery({
    queryKey: ["files", threadId, path, "content"],
    queryFn: () => readFile(threadId, path),
    retry: false,
  });
  const showSkeleton = useDelayedFlag(file.isPending, 300, 500);

  const fileOptions = useMemo(
    () => ({
      themeType: scheme === "dark" ? ("dark" as const) : ("light" as const),
      disableFileHeader: true,
      overflow: "wrap" as const,
    }),
    [scheme],
  );

  const fileName = baseName(path);
  const fileDir = directoryOf(path);
  const fileData = file.data?.file;
  const binary = fileData ? fileData.content.includes("\u0000") : false;

  const head = (
    <div className="file-head">
      <ActionIcon aria-label="Back to folder" size={22} onClick={onBack}>
        <IconArrowLeft size={14} stroke={1.5} />
      </ActionIcon>
      <Text size="sm" style={fileNameStyle}>
        {fileName}
      </Text>
      <Text size="sm" c="dimmed" className="dir">
        {fileDir ? `${fileDir}/` : ""}
      </Text>
    </div>
  );

  if (file.isPending) {
    return (
      <div className="file-block">
        {head}
        {showSkeleton ? (
          <ListSkeleton
            label="Loading file"
            rows={12}
            lines={1}
            className="file-preview-skeleton"
            rowClassName="skeleton-code-row"
          />
        ) : null}
      </div>
    );
  }
  if (file.isError) {
    return (
      <div className="file-block">
        {head}
        <LoadError
          title="Cannot read this file"
          message={file.error instanceof Error ? file.error.message : undefined}
          onRetry={() => void file.refetch()}
        />
      </div>
    );
  }
  if (!fileData) return null;

  return (
    <div className="file-block">
      {head}
      {binary ? (
        <Text size="sm" c="dimmed" ta="center" py="xl">
          Binary file — no text preview
        </Text>
      ) : (
        <>
          {fileData.truncated ? (
            <Text size="xs" c="dimmed" px="sm" py={4}>
              Truncated: showing the first{" "}
              {formatSize(fileData.content.length)}
            </Text>
          ) : null}
          {fileData.content.length === 0 ? (
            <Text size="sm" c="dimmed" ta="center" py="xl">
              This file is empty
            </Text>
          ) : (
            <div className="diff-wrap">
              <PierreFile
                file={{
                  name: fileName,
                  contents: fileData.content,
                }}
                options={fileOptions}
                disableWorkerPool
              />
            </div>
          )}
        </>
      )}
    </div>
  );
}
