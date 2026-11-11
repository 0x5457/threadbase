import { ShieldCheck, Trash2 } from "lucide-react";
import {
  ActionIcon,
  Badge,
  Group,
  Paper,
  Skeleton,
  Stack,
  Text,
} from "@mantine/core";
import { notifications } from "@mantine/notifications";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { deleteApprovalRule, fetchApprovalRules } from "@/api/endpoints";
import { EmptyState, ErrorState } from "@/components/app";

export function ApprovalRulesSection() {
  const queryClient = useQueryClient();
  const rules = useQuery({
    queryKey: ["approval-rules"],
    queryFn: fetchApprovalRules,
  });
  const remove = useMutation({
    mutationFn: (id: string) => deleteApprovalRule(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["approval-rules"] });
      notifications.show({
        message: "Allow rule revoked",
        color: "green",
      });
    },
    onError: (error) =>
      notifications.show({
        message: String(error),
        color: "red",
      }),
  });
  if (rules.isLoading) {
    return <Skeleton h={80} />;
  }
  if (rules.isError) {
    return (
      <ErrorState
        error={rules.error}
        title="Cannot load allow rules"
        onRetry={() => void rules.refetch()}
        compact
      />
    );
  }
  const items = rules.data?.rules ?? [];
  if (items.length === 0) {
    return (
      <EmptyState
        icon={ShieldCheck}
        title="No standing allow rules"
        description="Rules created with “Allow for thread” appear here and can be revoked."
        compact
      />
    );
  }
  return (
    <Paper withBorder radius="md">
      <Stack gap={0}>
        {items.map((rule) => (
          <Group
            key={rule.id}
            gap="sm"
            px="sm"
            py="xs"
            wrap="nowrap"
            style={{
              borderBottom: "1px solid var(--mantine-color-default-border)",
            }}
          >
            <Stack gap={0} style={{ flex: 1, minWidth: 0 }}>
              <Text size="sm" truncate="end">
                {rule.agent} · {rule.tool}
                {rule.commandPrefix ? (
                  <Text
                    component="span"
                    size="sm"
                    c="dimmed"
                    ff="var(--mantine-font-family-monospace)"
                    ml={4}
                  >
                    {rule.commandPrefix}
                  </Text>
                ) : null}
              </Text>
              <Text size="xs" c="dimmed">
                {rule.scope} · created by {rule.createdBy.displayName}
              </Text>
            </Stack>
            <Badge variant="outline" color="gray" size="xs">
              allow
            </Badge>
            <ActionIcon
              variant="subtle"
              color="gray"
              size="sm"
              className="tb-hit-40"
              aria-label="Revoke allow rule"
              loading={remove.isPending}
              onClick={() => remove.mutate(rule.id)}
            >
              <Trash2 size={14} aria-hidden />
            </ActionIcon>
          </Group>
        ))}
      </Stack>
    </Paper>
  );
}
