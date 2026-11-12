import {
  Button,
  CopyButton,
  Group,
  Text,
  TextInput,
} from "@mantine/core";
import { IconCopy, IconMail } from "@tabler/icons-react";
import type { ShareMember } from "../../api";
import { RoleSelect } from "./RoleSelect";

export function InviteForm({
  email,
  role,
  busy,
  inviteLink,
  onEmailChange,
  onRoleChange,
  onInvite,
}: {
  email: string;
  role: ShareMember["role"];
  busy: boolean;
  inviteLink: string | null;
  onEmailChange: (email: string) => void;
  onRoleChange: (role: ShareMember["role"]) => void;
  onInvite: () => void;
}) {
  return (
    <>
      <Group gap="xs" align="flex-end" wrap="nowrap">
        <TextInput
          flex={1}
          placeholder="name@example.com"
          value={email}
          onChange={(event) => onEmailChange(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") onInvite();
          }}
          leftSection={<IconMail size={13} stroke={1.5} />}
        />
        <RoleSelect
          size="sm"
          value={role}
          onChange={(next) => onRoleChange(next ?? "editor")}
        />
        <Button variant="default" loading={busy} onClick={onInvite}>
          Invite
        </Button>
      </Group>
      {inviteLink ? (
        <Group gap={6} wrap="nowrap">
          <Text size="sm" c="dimmed" className="share-link">
            {inviteLink}
          </Text>
          <CopyButton value={inviteLink}>
            {({ copied, copy }) => (
              <Button
                variant="default"
                size="compact-xs"
                leftSection={<IconCopy size={12} />}
                onClick={copy}
              >
                {copied ? "Copied" : "Copy"}
              </Button>
            )}
          </CopyButton>
        </Group>
      ) : null}
    </>
  );
}
