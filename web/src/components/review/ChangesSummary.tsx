import { Button, Group, Text } from "@mantine/core";
import { IconArrowsDiff, IconFileDiff } from "@tabler/icons-react";
import type { ThreadChanges } from "../../api";
import { SectionHead } from "./SectionHead";

export function ChangesSummary({
  changes,
  onOpenChanges,
}: {
  changes: ThreadChanges;
  onOpenChanges?: () => void;
}) {
  return (
    <div className="changes">
      <SectionHead
        icon={<IconFileDiff size={13} stroke={1.5} />}
        label="Changes"
        trailing={
          <Group gap={8} wrap="nowrap">
            <Text size="sm" className="stat-add">
              +{changes.stats.additions}
            </Text>
            <Text size="sm" className="stat-del">
              −{changes.stats.deletions}
            </Text>
            {onOpenChanges ? (
              <Button
                size="compact-xs"
                variant="default"
                leftSection={<IconArrowsDiff size={12} stroke={1.5} />}
                onClick={onOpenChanges}
              >
                Open changes
              </Button>
            ) : null}
          </Group>
        }
      />
      <Text size="xs" c="dimmed" px={10} pt={6}>
        Since {changes.baseLabel} · {changes.files.length} file
        {changes.files.length === 1 ? "" : "s"}
      </Text>
      {changes.files.length === 0 ? (
        <Text size="sm" c="dimmed" px={10} py="sm">
          No file changes yet
        </Text>
      ) : (
        changes.files.map((file) => (
          <div key={file.path} className="file-head">
            <Text size="sm" className="dir" ff="monospace">
              {file.path}
            </Text>
            <Text size="xs" className="stat-add">
              +{file.additions}
            </Text>
            <Text size="xs" className="stat-del">
              −{file.deletions}
            </Text>
            <Text size="xs" c="dimmed">
              {file.status}
            </Text>
          </div>
        ))
      )}
    </div>
  );
}
