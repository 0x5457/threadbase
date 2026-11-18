import {
  ActionIcon,
  Button,
  Card,
  Checkbox,
  Code,
  Group,
  Stack,
  Text,
  TextInput,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { IconTrash } from "@tabler/icons-react";
import { useState } from "react";
import { createMcpToken, listMcpTokens, revokeMcpToken } from "../../api";
import type { McpScope } from "../../api/types/McpScope";

const SCOPES: Array<{ value: McpScope; label: string; hint: string }> = [
  { value: "read", label: "Read", hint: "List and read threads." },
  { value: "write", label: "Write", hint: "Start a turn on a thread." },
  {
    value: "approve",
    label: "Approve",
    hint: "Answer approvals and questions. Administrators only.",
  },
];

/**
 * Tokens for outside agents.
 *
 * A token acts as the person who created it, so their roles and permission
 * modes still apply. The token is shown once and can be revoked here.
 */
export function McpTokens({ endpoint }: { endpoint: string }) {
  const queryClient = useQueryClient();
  const [label, setLabel] = useState("");
  const [scopes, setScopes] = useState<McpScope[]>(["read"]);
  const [created, setCreated] = useState<string | null>(null);

  const tokens = useQuery({
    queryKey: ["mcp-tokens"],
    queryFn: listMcpTokens,
  });
  const create = useMutation({
    mutationFn: () => createMcpToken(label.trim(), scopes),
    onSuccess: (result) => {
      setCreated(result.token);
      setLabel("");
      void queryClient.invalidateQueries({ queryKey: ["mcp-tokens"] });
    },
    onError: (cause) =>
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not create token",
        color: "red",
      }),
  });
  const revoke = useMutation({
    mutationFn: (id: string) => revokeMcpToken(id),
    onSuccess: (response) => {
      queryClient.setQueryData(["mcp-tokens"], response);
      notifications.show({ message: "Token revoked", color: "teal" });
    },
    onError: (cause) =>
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not revoke token",
        color: "red",
      }),
  });

  return (
    <Card withBorder p="md" className="settings-card">
      <Stack gap="sm">
        <Text size="sm" fw={500}>
          Agent access (MCP)
        </Text>
        <Text size="xs" c="dimmed">
          A token lets an outside agent use Threadbase as you. Your roles and
          the thread&apos;s permission mode still apply, and every turn it starts
          is marked in the transcript.
        </Text>
        <Group gap={6}>
          <Text size="xs" c="dimmed">
            Endpoint
          </Text>
          <Code>{endpoint}</Code>
        </Group>

        <Group align="flex-end" gap="sm" wrap="wrap">
          <TextInput
            size="xs"
            label="New token"
            description="A name you will recognise later, such as the tool or machine."
            placeholder="claude-desktop"
            value={label}
            onChange={(event) => setLabel(event.currentTarget.value)}
            w={220}
          />
          <Button
            size="compact-sm"
            loading={create.isPending}
            disabled={label.trim().length === 0 || scopes.length === 0}
            onClick={() => create.mutate()}
          >
            Create token
          </Button>
        </Group>
        <Group gap="md">
          {SCOPES.map((scope) => (
            <Checkbox
              key={scope.value}
              size="xs"
              label={scope.label}
              title={scope.hint}
              checked={scopes.includes(scope.value)}
              onChange={(event) =>
                setScopes((current) =>
                  event.currentTarget.checked
                    ? [...current, scope.value]
                    : current.filter((value) => value !== scope.value),
                )
              }
            />
          ))}
        </Group>

        {created ? (
          <Stack gap={4}>
            <Text size="xs" fw={500}>
              Copy this now — it is not shown again.
            </Text>
            <Group gap={6} wrap="nowrap">
              <Code style={{ overflowWrap: "anywhere" }}>{created}</Code>
              <Button
                size="compact-xs"
                variant="default"
                onClick={() => {
                  void navigator.clipboard?.writeText(created);
                  notifications.show({
                    message: "Token copied",
                    color: "teal",
                  });
                }}
              >
                Copy
              </Button>
            </Group>
          </Stack>
        ) : null}

        {(tokens.data?.tokens.length ?? 0) === 0 ? (
          <Text size="xs" c="dimmed">
            No tokens yet.
          </Text>
        ) : (
          <Stack gap={4}>
            {tokens.data?.tokens.map((token) => (
              <Group key={token.id} justify="space-between" wrap="nowrap">
                <Stack gap={0}>
                  <Text size="sm">{token.label}</Text>
                  <Text size="xs" c="dimmed">
                    {token.scopes.join(", ")} ·{" "}
                    {token.lastUsedAt
                      ? `last used ${new Date(token.lastUsedAt).toLocaleString()}`
                      : "never used"}
                  </Text>
                </Stack>
                <ActionIcon
                  aria-label={`Revoke ${token.label}`}
                  disabled={revoke.isPending}
                  onClick={() => revoke.mutate(token.id)}
                >
                  <IconTrash size={14} stroke={1.5} />
                </ActionIcon>
              </Group>
            ))}
          </Stack>
        )}
      </Stack>
    </Card>
  );
}
