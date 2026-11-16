import {
  ActionIcon,
  Button,
  Group,
  Loader,
  ScrollArea,
  Text,
} from "@mantine/core";
import { IconArrowsDiff, IconX } from "@tabler/icons-react";
import { useQuery } from "@tanstack/react-query";
import { useMemo, useState, type CSSProperties } from "react";
import { Virtuoso } from "react-virtuoso";
import { listFiles } from "../../api";
import { Breadcrumbs } from "./Breadcrumbs";
import { EntryRow } from "./EntryRow";
import { FilePreview } from "./FilePreview";
import { baseName, directoryOf, pathCrumbs, sortEntries } from "./paths";

const pathTextStyle: CSSProperties = {
  flex: 1,
  minWidth: 0,
  overflow: "hidden",
  fontFamily: "var(--mono)",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
};

export function FilesBrowser({
  threadId,
  onClose,
  onShowChanges,
}: {
  threadId: string;
  onClose?: () => void;
  onShowChanges?: () => void;
}) {
  const [path, setPath] = useState("");
  const [openedFile, setOpenedFile] = useState<string | null>(null);

  const listing = useQuery({
    queryKey: ["files", threadId, path],
    queryFn: () => listFiles(threadId, path),
  });

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

  const entries = useMemo(
    () => (listing.data ? sortEntries(listing.data.entries) : []),
    [listing.data],
  );

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
          <ActionIcon
            aria-label="Close files"
            data-autofocus
            size={22}
            onClick={onClose}
          >
            <IconX size={14} stroke={1.5} />
          </ActionIcon>
        ) : null}
      </div>
      <Breadcrumbs crumbs={crumbs} onNavigate={openDirectory} />
      {openedFile ? (
        <ScrollArea className="changes-scroll" type="hover">
          <FilePreview
            threadId={threadId}
            path={openedFile}
            onBack={() => setOpenedFile(null)}
          />
        </ScrollArea>
      ) : listing.isPending ? (
        <Group className="changes-scroll" justify="center" py="xl">
          <Loader size="sm" />
        </Group>
      ) : listing.isError ? (
        <Text size="sm" c="red" p="sm" className="changes-scroll">
          {listing.error instanceof Error
            ? listing.error.message
            : "Cannot list this folder"}
        </Text>
      ) : entries.length === 0 ? (
        <Text size="sm" c="dimmed" ta="center" py="xl" className="changes-scroll">
          This folder is empty
        </Text>
      ) : (
        // A monorepo folder can hold thousands of entries; mounting every row
        // froze the panel before the user could scroll. Virtuoso keeps only
        // the visible window in the DOM.
        <div className="changes-scroll changes-files">
          <Virtuoso
            style={{ height: "100%" }}
            data={entries}
            increaseViewportBy={300}
            computeItemKey={(_, entry) => entry.path}
            components={{
              Header: () =>
                listing.data?.truncated ? (
                  <Text size="xs" c="dimmed" px="sm" py={6}>
                    This folder has more than 5,000 entries; showing the first
                    5,000.
                  </Text>
                ) : null,
            }}
            itemContent={(_, entry) => (
              <EntryRow
                entry={entry}
                onOpen={() => {
                  if (entry.isDir) openDirectory(entry.path);
                  else setOpenedFile(entry.path);
                }}
              />
            )}
          />
        </div>
      )}
    </div>
  );
}
