import { NavLink, Text } from "@mantine/core";
import { IconChevronRight, IconFile, IconFolder } from "@tabler/icons-react";
import type { FileEntry } from "../../api";
import { formatSize } from "./paths";

export function EntryRow({
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
