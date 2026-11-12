import { ActionIcon, Badge, Group, Stack, Text } from "@mantine/core";
import { IconLink, IconTrash } from "@tabler/icons-react";
import type { ShareInvite } from "../../api";

export function PendingInvites({
  invites,
  busy,
  onRevoke,
}: {
  invites: ShareInvite[];
  busy: string | null;
  onRevoke: (invite: ShareInvite) => void;
}) {
  if (!invites.length) return null;

  return (
    <Stack gap={4}>
      <Text size="xs" c="dimmed">
        Pending invites
      </Text>
      {invites.map((pending) => (
        <Group key={pending.id} gap={8} wrap="nowrap">
          <IconLink size={13} stroke={1.5} />
          <Text size="sm" flex={1}>
            {pending.email ?? "link invite"}
          </Text>
          <Badge size="xs" variant="light" color="gray">
            {pending.role}
          </Badge>
          <ActionIcon
            aria-label="Revoke invite"
            loading={busy === `revoke:${pending.id}`}
            onClick={() => onRevoke(pending)}
          >
            <IconTrash size={13} stroke={1.5} />
          </ActionIcon>
        </Group>
      ))}
    </Stack>
  );
}
