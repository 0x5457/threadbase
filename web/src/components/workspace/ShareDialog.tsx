import { Check, Copy, Link2, UserPlus } from "lucide-react";
import {
  ActionIcon,
  Badge,
  Box,
  Button,
  Group,
  Modal,
  Paper,
  Select,
  Stack,
  Text,
  TextInput,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import {
  addMember,
  createInvite,
  removeMember,
  revokeInvite,
  setShareLevel,
} from "@/api/endpoints";
import { queryKeys, useMe } from "@/api/queries";
import type { Role } from "@/api/types/Role";
import type { ThreadBundleDto } from "@/api/types/ThreadBundleDto";

interface ShareDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  bundle: ThreadBundleDto;
}

const roleOptions: Array<{ value: Role; label: string }> = [
  { value: "editor", label: "Editor" },
  { value: "viewer", label: "Viewer" },
  { value: "owner", label: "Owner" },
];

const accessOptions = [
  ["invite_only", "Only invited"],
  ["workspace", "Whole workspace"],
] as const;

export function ShareDialog({ open, onOpenChange, bundle }: ShareDialogProps) {
  const queryClient = useQueryClient();
  const me = useMe();
  const [username, setUsername] = useState("");
  const [role, setRole] = useState<Role>("editor");
  const [inviteLink, setInviteLink] = useState<string | null>(null);
  const thread = bundle.thread;
  const owner =
    bundle.members.find((member) => member.user.id === me.data?.user.id)
      ?.role === "owner" || me.data?.user.isAdmin === true;
  const refresh = () => {
    void queryClient.invalidateQueries({
      queryKey: queryKeys.threadBundle(thread.id),
    });
    void queryClient.invalidateQueries({ queryKey: queryKeys.threads, exact: true });
  };
  const level = useMutation({
    mutationFn: (value: "invite_only" | "workspace") =>
      setShareLevel(thread.id, value),
    onSuccess: refresh,
    onError: (error) =>
      notifications.show({
        message: String(error),
        color: "red",
      }),
  });
  const member = useMutation({
    mutationFn: () => addMember(thread.id, { username: username.trim(), role }),
    onSuccess: () => {
      setUsername("");
      refresh();
      notifications.show({
        message: "Member added",
        color: "green",
      });
    },
    onError: (error) =>
      notifications.show({
        message: String(error),
        color: "red",
      }),
  });
  const invite = useMutation({
    mutationFn: () =>
      createInvite(thread.id, {
        username: username.trim() || null,
        email: null,
        role,
      }),
    onSuccess: (response) => {
      setUsername("");
      refresh();
      if (response.token) {
        const link = `${window.location.origin}/invite/${response.token}`;
        setInviteLink(link);
        void navigator.clipboard.writeText(link).catch(() => undefined);
        notifications.show({
          message: "Invite link copied",
          color: "green",
        });
      } else {
        notifications.show({
          message: "Member added",
          color: "green",
        });
      }
    },
    onError: (error) =>
      notifications.show({
        message: String(error),
        color: "red",
      }),
  });
  const revoke = useMutation({
    mutationFn: (inviteId: string) => revokeInvite(thread.id, inviteId),
    onSuccess: refresh,
    onError: (error) =>
      notifications.show({
        message: String(error),
        color: "red",
      }),
  });
  const remove = useMutation({
    mutationFn: (userId: string) => removeMember(thread.id, userId),
    onSuccess: refresh,
    onError: (error) =>
      notifications.show({
        message: String(error),
        color: "red",
      }),
  });

  const copyLink = async () => {
    await navigator.clipboard.writeText(window.location.href);
    notifications.show({
      message: "Thread link copied",
      color: "green",
    });
  };

  return (
    <Modal
      opened={open}
      onClose={() => onOpenChange(false)}
      title="Share thread"
      size="sm"
    >
      <Stack gap="sm">
        <Stack gap="xs">
          <Text size="xs" c="dimmed">
            Access
          </Text>
          <Box
            style={{
              display: "grid",
              gridTemplateColumns: "1fr 1fr",
              gap: 6,
            }}
          >
            {accessOptions.map(([value, label]) => (
              <Button
                key={value}
                type="button"
                variant="subtle"
                color={thread.shareLevel === value ? "blue" : "gray"}
                disabled={!owner}
                aria-pressed={thread.shareLevel === value}
                onClick={() => level.mutate(value)}
                justify="flex-start"
                size="xs"
                leftSection={
                  thread.shareLevel === value ? (
                    <Check size={12} aria-hidden />
                  ) : undefined
                }
              >
                {label}
              </Button>
            ))}
          </Box>
        </Stack>

        <Stack gap="xs">
          <Text size="xs" c="dimmed">
            Members{" "}
            <Text component="span" size="xs" c="dimmed">
              ({bundle.members.length})
            </Text>
          </Text>
          <Paper withBorder radius="md">
            <Stack gap={0}>
              {bundle.members.map((entry) => (
                <Group
                  key={entry.user.id}
                  gap="xs"
                  px="sm"
                  py="xs"
                  wrap="nowrap"
                  style={{
                    borderBottom: "1px solid var(--mantine-color-default-border)",
                  }}
                >
                  <span
                    aria-hidden
                    style={{
                      width: 8,
                      height: 8,
                      flexShrink: 0,
                      borderRadius: "50%",
                      background: entry.user.color,
                    }}
                  />
                  <Text size="sm" truncate="end" style={{ flex: 1 }}>
                    {entry.user.displayName}
                    <Text component="span" size="sm" c="dimmed" ml={4}>
                      @{entry.user.username}
                    </Text>
                  </Text>
                  <Badge variant="light" color="gray" size="xs">
                    {entry.role}
                  </Badge>
                  {owner &&
                  entry.user.id !== me.data?.user.id &&
                  entry.role !== "owner" ? (
                    <Button
                      type="button"
                      variant="subtle"
                      color="red"
                      size="xs"
                      px="xs"
                      onClick={() => remove.mutate(entry.user.id)}
                    >
                      remove
                    </Button>
                  ) : null}
                </Group>
              ))}
            </Stack>
          </Paper>
        </Stack>

        {bundle.share.pendingInvites.length > 0 ? (
          <Stack gap="xs">
            <Text size="xs" c="dimmed">
              Pending links
            </Text>
            <Paper withBorder radius="md">
              <Stack gap={0}>
                {bundle.share.pendingInvites.map((invite) => (
                  <Group
                    key={invite.id}
                    gap="xs"
                    px="sm"
                    py="xs"
                    wrap="nowrap"
                    style={{
                      borderBottom:
                        "1px solid var(--mantine-color-default-border)",
                    }}
                  >
                    <Text size="sm" c="dimmed" truncate="end" style={{ flex: 1 }}>
                      {invite.email ?? "invite link"} · {invite.role}
                    </Text>
                    {owner ? (
                      <Button
                        type="button"
                        variant="subtle"
                        color="red"
                        size="xs"
                        px="xs"
                        onClick={() => revoke.mutate(invite.id)}
                      >
                        revoke
                      </Button>
                    ) : null}
                  </Group>
                ))}
              </Stack>
            </Paper>
          </Stack>
        ) : null}

        {owner ? (
          <Stack gap="xs">
            <Text size="xs" c="dimmed">
              Invite
            </Text>
            <Group gap="xs" wrap="nowrap">
              <TextInput
                value={username}
                onChange={(event) => setUsername(event.target.value)}
                placeholder="username or email"
                size="xs"
              />
              <Select
                value={role}
                onChange={(value) => setRole((value ?? "editor") as Role)}
                data={roleOptions}
                size="xs"
                w={96}
              />
              <Button
                variant="filled"
                size="xs"
                loading={member.isPending || invite.isPending}
                disabled={!username.trim()}
                onClick={() => member.mutate()}
                leftSection={<UserPlus size={14} aria-hidden />}
              >
                Add
              </Button>
            </Group>
            <Button
              type="button"
              variant="subtle"
              color="blue"
              size="xs"
              justify="flex-start"
              px={0}
              onClick={() => invite.mutate()}
            >
              Create a 14-day invite link instead
            </Button>
            {inviteLink ? (
              <Paper withBorder radius="md" px="sm" py="xs">
                <Group gap="xs">
                  <Text
                    size="sm"
                    ff="var(--mantine-font-family-monospace)"
                    truncate="end"
                    style={{ flex: 1 }}
                  >
                    {inviteLink}
                  </Text>
                  <ActionIcon
                    variant="subtle"
                    color="gray"
                    size="sm"
                    aria-label="Copy invite link"
                    onClick={() =>
                      void navigator.clipboard.writeText(inviteLink)
                    }
                  >
                    <Copy size={14} />
                  </ActionIcon>
                </Group>
              </Paper>
            ) : null}
          </Stack>
        ) : (
          <Text size="sm" c="dimmed">
            Only the thread owner can change access or invite people.
          </Text>
        )}
      </Stack>

      <Group justify="flex-end" gap="xs" mt="sm">
        <Button variant="default" size="sm" onClick={() => onOpenChange(false)}>
          Done
        </Button>
        <Button
          variant="default"
          size="sm"
          leftSection={<Link2 size={14} aria-hidden />}
          onClick={() => void copyLink()}
        >
          Copy link
        </Button>
      </Group>
    </Modal>
  );
}
