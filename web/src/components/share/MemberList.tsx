import { ActionIcon, Avatar, Group, Stack, Text, Tooltip } from "@mantine/core";
import { IconTrash } from "@tabler/icons-react";
import type { ShareMember } from "../../api";
import { RoleSelect } from "./RoleSelect";

function initials(name: string) {
  return name
    .split(/\s+/)
    .map((part) => part[0])
    .slice(0, 2)
    .join("")
    .toUpperCase();
}

export function MemberList({
  members,
  busy,
  onRoleChange,
  onRemove,
}: {
  members: ShareMember[];
  busy: string | null;
  onRoleChange: (member: ShareMember, role: ShareMember["role"]) => void;
  onRemove: (member: ShareMember) => void;
}) {
  return (
    <Stack gap={4}>
      {members.map((member) => (
        <Group key={member.user.id} gap={8} wrap="nowrap">
          <Avatar
            size={22}
            radius="xl"
            color="brand"
            src={member.user.avatarUrl ?? undefined}
          >
            {initials(member.user.displayName)}
          </Avatar>
          <Stack gap={0} style={{ flex: 1, minWidth: 0 }}>
            <Text size="sm" className="share-name">
              {member.user.displayName}
            </Text>
            <Text size="xs" c="dimmed">
              @{member.user.username}
            </Text>
          </Stack>
          <RoleSelect
            size="xs"
            value={member.role}
            onChange={(role) => {
              if (role) onRoleChange(member, role);
            }}
          />
          <Tooltip label="Remove member">
            <ActionIcon
              aria-label="Remove member"
              loading={busy === `remove:${member.user.id}`}
              onClick={() => onRemove(member)}
            >
              <IconTrash size={13} stroke={1.5} />
            </ActionIcon>
          </Tooltip>
        </Group>
      ))}
    </Stack>
  );
}
