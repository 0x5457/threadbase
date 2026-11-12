import {
  ActionIcon,
  Avatar,
  Badge,
  Button,
  CopyButton,
  Divider,
  Group,
  Loader,
  Modal,
  Select,
  Stack,
  Text,
  TextInput,
  Tooltip,
} from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import {
  IconCopy,
  IconLink,
  IconMail,
  IconTrash,
  IconUserPlus,
} from "@tabler/icons-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import {
  addMember,
  createInvite,
  getThreadBundle,
  removeMember,
  revokeInvite,
  setShareLevel,
  updateMember,
  type ShareMember,
  type ShareState,
} from "../api";

const roles = [
  { value: "editor", label: "Editor" },
  { value: "viewer", label: "Viewer" },
  { value: "owner", label: "Owner" },
];

function initials(name: string) {
  return name
    .split(/\s+/)
    .map((part) => part[0])
    .slice(0, 2)
    .join("")
    .toUpperCase();
}

function inviteUrl(token: string | null) {
  if (!token) return null;
  return `${window.location.origin}/invite/${token}`;
}

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
  const queryClient = useQueryClient();
  const [level, setLevel] = useState<ShareState["shareLevel"]>("invite_only");
  const [username, setUsername] = useState("");
  const [memberRole, setMemberRole] =
    useState<ShareMember["role"]>("editor");
  const [email, setEmail] = useState("");
  const [inviteRole, setInviteRole] = useState<ShareMember["role"]>("editor");
  const [inviteLink, setInviteLink] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const bundle = useQuery({
    queryKey: ["thread", threadId],
    queryFn: () => getThreadBundle(threadId as string),
    enabled: Boolean(threadId),
    staleTime: 2_000,
  });
  const share = bundle.data?.bundle.share;

  useEffect(() => {
    if (share) setLevel(share.shareLevel);
  }, [share]);

  useEffect(() => {
    if (!opened) return;
    setUsername("");
    setEmail("");
    setInviteLink(null);
    setBusy(null);
    setMemberRole("editor");
    setInviteRole("editor");
  }, [opened, threadId]);

  const refresh = () => {
    if (!threadId) return;
    void queryClient.invalidateQueries({ queryKey: ["thread", threadId] });
  };

  const run = async (key: string, action: () => Promise<unknown>) => {
    setBusy(key);
    try {
      await action();
      refresh();
    } catch (cause) {
      notifications.show({
        message: cause instanceof Error ? cause.message : "Request failed",
        color: "red",
      });
    } finally {
      setBusy(null);
    }
  };

  const add = () => {
    if (!threadId || !username.trim()) return;
    void run("add", async () => {
      await addMember(threadId, username.trim(), memberRole);
      setUsername("");
      notifications.show({ message: "Member added", color: "teal" });
    });
  };

  const invite = () => {
    if (!threadId) return;
    void run("invite", async () => {
      const result = await createInvite(threadId, {
        email: email.trim() || undefined,
        role: inviteRole,
      });
      setEmail("");
      setInviteLink(inviteUrl(result.token));
      notifications.show({ message: "Invite created", color: "teal" });
    });
  };

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="Share thread"
      size="md"
      centered
      fullScreen={phone}
    >
      {bundle.isPending || !share ? (
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
            value={level}
            onChange={(value) => {
              if (!value) return;
              const next = value as ShareState["shareLevel"];
              setLevel(next);
              void run("level", () => setShareLevel(share.threadId, next));
            }}
            allowDeselect={false}
          />

          <Divider label="Members" labelPosition="left" />

          <Stack gap={4}>
            {share.members.map((member) => (
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
                <Select
                  size="xs"
                  w={100}
                  data={roles}
                  value={member.role}
                  allowDeselect={false}
                  onChange={(value) => {
                    if (!threadId || !value) return;
                    void run(`role:${member.user.id}`, async () => {
                      await updateMember(
                        threadId,
                        member.user.id,
                        value as ShareMember["role"],
                      );
                    });
                  }}
                />
                <Tooltip label="Remove member">
                  <ActionIcon
                    aria-label="Remove member"
                    loading={busy === `remove:${member.user.id}`}
                    onClick={() => {
                      if (!threadId) return;
                      void run(`remove:${member.user.id}`, () =>
                        removeMember(threadId, member.user.id),
                      );
                    }}
                  >
                    <IconTrash size={13} stroke={1.5} />
                  </ActionIcon>
                </Tooltip>
              </Group>
            ))}
          </Stack>

          <Group gap="xs" align="flex-end" wrap="nowrap">
            <TextInput
              flex={1}
              label="Add member"
              placeholder="github login"
              value={username}
              onChange={(event) => setUsername(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") add();
              }}
              leftSection={<IconUserPlus size={13} stroke={1.5} />}
            />
            <Select
              size="sm"
              w={100}
              data={roles}
              value={memberRole}
              onChange={(value) =>
                setMemberRole((value as ShareMember["role"]) ?? "editor")
              }
              allowDeselect={false}
            />
            <Button
              variant="default"
              loading={busy === "add"}
              disabled={!username.trim()}
              onClick={add}
            >
              Add
            </Button>
          </Group>

          <Divider label="Invite by email" labelPosition="left" />

          <Group gap="xs" align="flex-end" wrap="nowrap">
            <TextInput
              flex={1}
              placeholder="name@example.com"
              value={email}
              onChange={(event) => setEmail(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") invite();
              }}
              leftSection={<IconMail size={13} stroke={1.5} />}
            />
            <Select
              size="sm"
              w={100}
              data={roles}
              value={inviteRole}
              onChange={(value) =>
                setInviteRole((value as ShareMember["role"]) ?? "editor")
              }
              allowDeselect={false}
            />
            <Button
              variant="default"
              loading={busy === "invite"}
              onClick={invite}
            >
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

          {share.pendingInvites.length ? (
            <Stack gap={4}>
              <Text size="xs" c="dimmed">
                Pending invites
              </Text>
              {share.pendingInvites.map((pending) => (
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
                    onClick={() => {
                      if (!threadId) return;
                      void run(`revoke:${pending.id}`, () =>
                        revokeInvite(threadId, pending.id),
                      );
                    }}
                  >
                    <IconTrash size={13} stroke={1.5} />
                  </ActionIcon>
                </Group>
              ))}
            </Stack>
          ) : null}
        </Stack>
      )}
    </Modal>
  );
}
