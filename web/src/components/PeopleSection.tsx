import {
  Avatar,
  Badge,
  Button,
  Card,
  Group,
  Modal,
  Stack,
  Text,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { LoadError } from "./loading/LoadError";
import { SettingsRowsSkeleton } from "./loading/Skeleton";
import { useDelayedFlag } from "../lib/use-delayed-flag";
import { useState } from "react";
import { listUsers, updateUser, type User, type UserAccess } from "../api";
import { initials } from "./thread/format";

interface UserAction {
  id: string;
  label: string;
  access: UserAccess;
  isAdmin: boolean;
  success: string;
  tone?: "default" | "primary" | "danger";
}

function actionsFor(user: User): UserAction[] {
  if (user.access === "pending") {
    return [
      {
        id: "approve-read-only",
        label: "Approve read-only",
        access: "read_only",
        isAdmin: false,
        success: `${user.displayName} can now view the workspace`,
      },
      {
        id: "approve",
        label: "Approve",
        access: "member",
        isAdmin: user.isAdmin,
        success: `${user.displayName} can now edit the workspace`,
        tone: "primary",
      },
      {
        id: "deny",
        label: "Deny",
        access: "disabled",
        isAdmin: false,
        success: `Access denied for ${user.displayName}`,
        tone: "danger",
      },
    ];
  }

  if (user.isAdmin) {
    return [
      {
        id: "remove-admin",
        label: "Remove admin",
        access: user.access,
        isAdmin: false,
        success: `${user.displayName} is no longer an admin`,
      },
    ];
  }

  if (user.access === "read_only") {
    return [
      {
        id: "allow-editing",
        label: "Allow editing",
        access: "member",
        isAdmin: false,
        success: `${user.displayName} can now edit the workspace`,
        tone: "primary",
      },
    ];
  }

  if (user.access === "member") {
    return [
      {
        id: "make-read-only",
        label: "Make read-only",
        access: "read_only",
        isAdmin: false,
        success: `${user.displayName} can now only view the workspace`,
      },
    ];
  }

  if (user.access === "disabled") {
    return [
      {
        id: "restore",
        label: "Restore as read-only",
        access: "read_only",
        isAdmin: false,
        success: `${user.displayName} can view the workspace again`,
        tone: "primary",
      },
    ];
  }

  return [];
}

function accessBadges(user: User, currentUserId: string) {
  return (
    <>
      {user.id === currentUserId ? (
        <Badge color="gray" variant="light">
          You
        </Badge>
      ) : null}
      {user.isAdmin ? (
        <Badge color="brand" variant="light">
          Admin
        </Badge>
      ) : null}
      {user.access === "pending" ? (
        <Badge color="yellow" variant="light">
          Pending
        </Badge>
      ) : null}
      {user.access === "read_only" ? (
        <Badge color="gray" variant="light">
          Read-only
        </Badge>
      ) : null}
      {user.access === "disabled" ? (
        <Badge color="red" variant="light">
          Disabled
        </Badge>
      ) : null}
    </>
  );
}

export function PeopleSection({ currentUserId }: { currentUserId: string }) {
  const queryClient = useQueryClient();
  const users = useQuery({
    queryKey: ["users"],
    queryFn: listUsers,
    staleTime: 10_000,
  });
  const [pending, setPending] = useState<string | null>(null);
  const showLoading = useDelayedFlag(users.isPending, 300, 500);
  const showPending = useDelayedFlag(pending !== null, 300, 500);
  const [actionError, setActionError] = useState<{
    message: string;
    onRetry: () => void;
  } | null>(null);
  const [revokeFor, setRevokeFor] = useState<User | null>(null);

  const apply = async (user: User, action: UserAction) => {
    setPending(`${user.id}:${action.id}`);
    setActionError(null);
    try {
      const { users: next } = await updateUser(user.id, {
        isAdmin: action.isAdmin,
        access: action.access,
      });
      queryClient.setQueryData(["users"], { users: next });
      notifications.show({ message: action.success, color: "teal" });
      return true;
    } catch (cause) {
      setActionError({
        message:
          cause instanceof Error ? cause.message : "Could not update access",
        onRetry: () => void apply(user, action),
      });
      return false;
    } finally {
      setPending(null);
    }
  };

  const revokeAccess = async () => {
    const user = revokeFor;
    if (!user) return;
    const ok = await apply(user, {
      id: "revoke",
      label: "Revoke access",
      access: "disabled",
      isAdmin: false,
      success: `Access revoked for ${user.displayName}`,
      tone: "danger",
    });
    if (ok) setRevokeFor(null);
  };

  const pendingCount =
    users.data?.users.filter((user) => user.access === "pending").length ?? 0;

  return (
    <Card withBorder p="md" className="settings-card">
      <Stack gap="sm">
        <Group justify="space-between" align="center">
          <Text size="sm" fw={500}>
            People
          </Text>
          <Text size="xs" c="dimmed">
            {pendingCount} pending
          </Text>
        </Group>
        <Text size="xs" c="dimmed">
          Approve new accounts and manage who can view or edit the workspace.
        </Text>

        {users.isPending && showLoading ? (
          <SettingsRowsSkeleton label="Loading people" />
        ) : null}

        {actionError ? (
          <LoadError
            title="Could not update access"
            message={actionError.message}
            onRetry={actionError.onRetry}
            compact
          />
        ) : null}
        {users.isError ? (
          <LoadError
            title="Could not load people"
            message={
              users.error instanceof Error ? users.error.message : undefined
            }
            onRetry={() => void users.refetch()}
            compact
          />
        ) : null}

        {(users.data?.users ?? []).map((user) => {
          const isSelf = user.id === currentUserId;
          const actions = isSelf ? [] : actionsFor(user);
          const canRevoke =
            !isSelf && user.access !== "pending" && user.access !== "disabled";

          return (
            <Group
              key={user.id}
              justify="space-between"
              align="center"
              wrap="nowrap"
              gap="sm"
            >
              <Group gap="xs" wrap="nowrap" style={{ minWidth: 0 }}>
                <Avatar
                  size={26}
                  radius="xl"
                  color="brand"
                  src={user.avatarUrl ?? undefined}
                >
                  {initials(user.displayName || user.username)}
                </Avatar>
                <Stack gap={1} style={{ minWidth: 0 }}>
                  <Group gap={5} wrap="nowrap">
                    <Text size="sm" fw={500} truncate>
                      {user.displayName}
                    </Text>
                    {accessBadges(user, currentUserId)}
                  </Group>
                  <Text size="xs" c="dimmed" truncate>
                    @{user.username}
                  </Text>
                </Stack>
              </Group>

              {actions.length > 0 || canRevoke ? (
                <Group gap={5} wrap="wrap" justify="flex-end">
                  {actions.map((action) => (
                    <Button
                      key={action.id}
                      variant={
                        action.tone === "primary"
                          ? "filled"
                          : action.tone === "danger"
                            ? "subtle"
                            : "default"
                      }
                      color={
                        action.tone === "primary"
                          ? "dark"
                          : action.tone === "danger"
                            ? "red"
                            : undefined
                      }
                      loading={
                        showPending && pending === `${user.id}:${action.id}`
                      }
                      disabled={
                        pending !== null &&
                        pending !== `${user.id}:${action.id}`
                      }
                      onClick={() => void apply(user, action)}
                    >
                      {action.label}
                    </Button>
                  ))}
                  {canRevoke ? (
                    <Button
                      variant="subtle"
                      color="red"
                      disabled={pending !== null}
                      onClick={() => setRevokeFor(user)}
                    >
                      Revoke access
                    </Button>
                  ) : null}
                </Group>
              ) : null}
            </Group>
          );
        })}
      </Stack>

      <Modal
        opened={revokeFor !== null}
        onClose={() => setRevokeFor(null)}
        title="Revoke access"
        centered
        closeOnClickOutside={pending === null}
        closeOnEscape={pending === null}
        withCloseButton={pending === null}
      >
        <Text size="sm" c="dimmed" mb="md">
          <strong>{revokeFor?.displayName}</strong> will no longer be able to
          open this workspace.
        </Text>
        {actionError ? (
          <LoadError
            title="Could not revoke access"
            message={actionError.message}
            onRetry={actionError.onRetry}
            compact
          />
        ) : null}
        <Group justify="flex-end" gap="xs">
          <Button
            variant="default"
            onClick={() => setRevokeFor(null)}
            disabled={pending !== null}
          >
            Cancel
          </Button>
          <Button
            color="red"
            loading={showPending && pending === `${revokeFor?.id}:revoke`}
            disabled={pending !== null}
            onClick={() => void revokeAccess()}
          >
            Revoke access
          </Button>
        </Group>
      </Modal>
    </Card>
  );
}
