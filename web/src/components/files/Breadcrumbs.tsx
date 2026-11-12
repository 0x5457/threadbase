import { Anchor, Group, Text } from "@mantine/core";
import type { Crumb } from "./paths";

export function Breadcrumbs({
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
