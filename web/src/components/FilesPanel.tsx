import {
  ActionIcon,
  Anchor,
  Button,
  Group,
  Loader,
  NavLink,
  ScrollArea,
  Text,
  useComputedColorScheme,
} from "@mantine/core";
import { File as PierreFile } from "@pierre/diffs/react";
import {
  IconArrowLeft,
  IconArrowsDiff,
  IconChevronRight,
  IconFile,
  IconFolder,
  IconX,
} from "@tabler/icons-react";
import { useQuery } from "@tanstack/react-query";
import { useMemo, useState, type CSSProperties } from "react";
import { listFiles, readFile, type FileEntry } from "../api";

const pathTextStyle: CSSProperties = {
  flex: 1,
  minWidth: 0,
  overflow: "hidden",
  fontFamily: "var(--mono)",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
};

const fileNameStyle: CSSProperties = {
  flex: "0 1 auto",
  minWidth: 0,
  overflow: "hidden",
  fontFamily: "var(--mono)",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
};

interface Crumb {
  label: string;
  path: string;
}

function baseName(path: string) {
  return path.slice(path.lastIndexOf("/") + 1);
}

function directoryOf(path: string) {
  const slash = path.lastIndexOf("/");
  return slash === -1 ? "" : path.slice(0, slash);
}

function pathCrumbs(path: string): Crumb[] {
  const crumbs: Crumb[] = [{ label: "worktree", path: "" }];
  let current = "";
  for (const segment of path.split("/")) {
    if (segment === "") continue;
    current = current === "" ? segment : `${current}/${segment}`;
    crumbs.push({ label: segment, path: current });
  }
  return crumbs;
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : String(Math.round(value))} ${units[unit]}`;
}

function sortEntries(entries: FileEntry[]) {
  return entries.toSorted((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
}

function Breadcrumbs({
  crumbs,
  onNavigate,
}: {
  crumbs: Crumb[];
  onNavigate: (path: string) => void;
}) {
  if (crumbs.length < 2) return null;
  return (
    <Group
      gap={2}
      wrap="wrap"
      role="navigation"
      aria-label="Folder path"
      style={{
        padding: "2px 10px 3px",
        borderBottom: "1px solid var(--line-soft)",
      }}
    >
      {crumbs.map((crumb, index) => {
        const current = index === crumbs.length - 1;
        return (
          <Group key={crumb.path || "worktree"} gap={2} wrap="nowrap">
            {index > 0 ? (
              <Text size="xs" c="dimmed">
                /
              </Text>
            ) : null}
            {current ? (
              <Text
                size="xs"
                fw={500}
                style={{
                  maxWidth: 220,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {crumb.label}
              </Text>
            ) : (
              <Anchor
                component="button"
                type="button"
                size="xs"
                c="dimmed"
                onClick={() => onNavigate(crumb.path)}
                style={{ padding: "3px 1px" }}
              >
                {crumb.label}
              </Anchor>
            )}
          </Group>
        );
      })}
    </Group>
  );
}

function EntryRow({
  entry,
  onOpen,
}: {
  entry: FileEntry;
  onOpen: () => void;
}) {
  return (
    <NavLink
      component="button"
      className="side-nav change-row"
      aria-label={
        entry.isDir ? `Open folder ${entry.path}` : `Open file ${entry.path}`
      }
      onClick={onOpen}
      leftSection={
        entry.isDir ? (
          <IconFolder size={13} stroke={1.5} />
        ) : (
          <IconFile size={13} stroke={1.5} />
        )
      }
      label={
        <Text size="sm" className="change-path">
          {entry.name}
        </Text>
      }
      rightSection={
        entry.isDir ? (
          <IconChevronRight size={13} stroke={1.5} />
        ) : (
          <Text size="xs" c="dimmed">
            {formatSize(entry.size)}
          </Text>
        )
      }
    />
  );
}

export function FilesPanel({
  threadId,
  onClose,
  onShowChanges,
}: {
  threadId: string;
  onClose?: () => void;
  onShowChanges?: () => void;
}) {
  // Keying the browser by thread clears the browsed path when the panel
  // switches to a different worktree.
  return (
    <FilesBrowser
      key={threadId}
      threadId={threadId}
      onClose={onClose}
      onShowChanges={onShowChanges}
    />
  );
}

function FilesBrowser({
  threadId,
  onClose,
  onShowChanges,
}: {
  threadId: string;
  onClose?: () => void;
  onShowChanges?: () => void;
}) {
  const scheme = useComputedColorScheme("light");
  const [path, setPath] = useState("");
  const [openedFile, setOpenedFile] = useState<string | null>(null);

  const listing = useQuery({
    queryKey: ["files", threadId, path],
    queryFn: () => listFiles(threadId, path),
  });

  const file = useQuery({
    queryKey: ["files", threadId, openedFile, "content"],
    queryFn: () => readFile(threadId, openedFile as string),
    enabled: openedFile !== null,
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

  const openDirectory = (next: string) => {
    setOpenedFile(null);
    setPath(next);
  };

  const crumbs = openedFile
    ? [
        ...pathCrumbs(directoryOf(openedFile)),
        { label: baseName(openedFile), path: openedFile },
      ]
    : pathCrumbs(path);

  const entries = listing.data ? sortEntries(listing.data.entries) : [];
  const fileName = openedFile ? baseName(openedFile) : "";
  const fileDir = openedFile ? directoryOf(openedFile) : "";
  const fileData = file.data?.file;
  const binary = fileData ? fileData.content.includes("\u0000") : false;

  return (
    <div className="changes">
      <div className="changes-head">
        <Text size="sm" fw={500}>
          Files
        </Text>
        <Text size="sm" c="dimmed" style={pathTextStyle}>
          /{openedFile ?? path}
        </Text>
        {onShowChanges ? (
          <Button
            variant="subtle"
            size="compact-xs"
            color="gray"
            leftSection={<IconArrowsDiff size={12} stroke={1.5} />}
            onClick={onShowChanges}
          >
            Changes
          </Button>
        ) : null}
        {onClose ? (
          <ActionIcon aria-label="Close files"
            data-autofocus size={22} onClick={onClose}>
            <IconX size={14} stroke={1.5} />
          </ActionIcon>
        ) : null}
      </div>
      <Breadcrumbs crumbs={crumbs} onNavigate={openDirectory} />
      <ScrollArea className="changes-scroll" type="hover">
        {openedFile ? (
          file.isPending ? (
            <Group justify="center" py="xl">
              <Loader size="sm" />
            </Group>
          ) : file.isError ? (
            <Text size="sm" c="red" p="sm">
              {file.error instanceof Error
                ? file.error.message
                : "Cannot read this file"}
            </Text>
          ) : fileData ? (
            <div className="file-block">
              <div className="file-head">
                <ActionIcon
                  aria-label="Back to folder"
                  size={22}
                  onClick={() => setOpenedFile(null)}
                >
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
                          lang: fileData.language ?? undefined,
                        }}
                        options={fileOptions}
                        disableWorkerPool
                      />
                    </div>
                  )}
                </>
              )}
            </div>
          ) : null
        ) : listing.isPending ? (
          <Group justify="center" py="xl">
            <Loader size="sm" />
          </Group>
        ) : listing.isError ? (
          <Text size="sm" c="red" p="sm">
            {listing.error instanceof Error
              ? listing.error.message
              : "Cannot list this folder"}
          </Text>
        ) : entries.length === 0 ? (
          <Text size="sm" c="dimmed" ta="center" py="xl">
            This folder is empty
          </Text>
        ) : (
          <div className="changes-files">
            {entries.map((entry) => (
              <EntryRow
                key={entry.path}
                entry={entry}
                onOpen={() => {
                  if (entry.isDir) openDirectory(entry.path);
                  else setOpenedFile(entry.path);
                }}
              />
            ))}
          </div>
        )}
      </ScrollArea>
    </div>
  );
}
