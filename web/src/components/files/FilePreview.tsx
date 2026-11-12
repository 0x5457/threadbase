import {
  ActionIcon,
  Group,
  Loader,
  Text,
  useComputedColorScheme,
} from "@mantine/core";
import { File as PierreFile } from "@pierre/diffs/react";
import { IconArrowLeft } from "@tabler/icons-react";
import { useQuery } from "@tanstack/react-query";
import { useMemo, type CSSProperties } from "react";
import { readFile } from "../../api";
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

  if (file.isPending) {
    return (
      <Group justify="center" py="xl">
        <Loader size="sm" />
      </Group>
    );
  }
  if (file.isError) {
    return (
      <Text size="sm" c="red" p="sm">
        {file.error instanceof Error
          ? file.error.message
          : "Cannot read this file"}
      </Text>
    );
  }
  if (!fileData) return null;

  return (
    <div className="file-block">
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
