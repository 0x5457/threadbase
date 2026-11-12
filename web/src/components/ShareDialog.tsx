import {
  Divider,
  Group,
  Loader,
  Modal,
  Select,
  Stack,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import type { ShareState } from "../api";
import { AddMemberForm } from "./share/AddMemberForm";
import { InviteForm } from "./share/InviteForm";
import { MemberList } from "./share/MemberList";
import { PendingInvites } from "./share/PendingInvites";
import { useShareDialog } from "./share/useShareDialog";

export function ShareDialog({
  threadId,
  opened,
  onClose,
}: {
  threadId: string | null;
  opened: boolean;
  onClose: () => void;
}) {
  const phone = useMediaQuery("(max-width: 48em)");
  const share = useShareDialog(threadId, opened);

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Share thread"
      size="md"
      centered
      fullScreen={phone}
    >
      {share.bundle.isPending || !share.share ? (
        <Group justify="center" py="xl">
          <Loader size="sm" />
        </Group>
      ) : (
        <Stack gap="sm">
          <Select
            label="Who can open this thread"
            data={[
              { value: "invite_only", label: "Only invited people" },
              { value: "workspace", label: "Everyone in the workspace" },
            ]}
            value={share.level}
            onChange={(value) => {
              if (!value) return;
              share.changeLevel(value as ShareState["shareLevel"]);
            }}
            allowDeselect={false}
          />

          <Divider label="Members" labelPosition="left" />

          <MemberList
            members={share.share.members}
            busy={share.busy}
            onRoleChange={share.updateRole}
            onRemove={share.remove}
          />

          <AddMemberForm
            username={share.username}
            role={share.memberRole}
            busy={share.busy === "add"}
            onUsernameChange={share.setUsername}
            onRoleChange={share.setMemberRole}
            onAdd={share.add}
          />

          <Divider label="Invite by email" labelPosition="left" />

          <InviteForm
            email={share.email}
            role={share.inviteRole}
            busy={share.busy === "invite"}
            inviteLink={share.inviteLink}
            onEmailChange={share.setEmail}
            onRoleChange={share.setInviteRole}
            onInvite={share.invite}
          />

          <PendingInvites
            invites={share.share.pendingInvites}
            busy={share.busy}
            onRevoke={share.revoke}
          />
        </Stack>
      )}
    </Modal>
  );
}
