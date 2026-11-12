import { Button, Group, TextInput } from "@mantine/core";
import { IconUserPlus } from "@tabler/icons-react";
import type { ShareMember } from "../../api";
import { RoleSelect } from "./RoleSelect";

export function AddMemberForm({
  username,
  role,
  busy,
  onUsernameChange,
  onRoleChange,
  onAdd,
}: {
  username: string;
  role: ShareMember["role"];
  busy: boolean;
  onUsernameChange: (username: string) => void;
  onRoleChange: (role: ShareMember["role"]) => void;
  onAdd: () => void;
}) {
  return (
    <Group gap="xs" align="flex-end" wrap="nowrap">
      <TextInput
        flex={1}
        label="Add member"
        placeholder="github login"
        value={username}
        onChange={(event) => onUsernameChange(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") onAdd();
        }}
        leftSection={<IconUserPlus size={13} stroke={1.5} />}
      />
      <RoleSelect
        size="sm"
        value={role}
        onChange={(next) => onRoleChange(next ?? "editor")}
      />
      <Button
        variant="default"
        loading={busy}
        disabled={!username.trim()}
        onClick={onAdd}
      >
        Add
      </Button>
    </Group>
  );
}
