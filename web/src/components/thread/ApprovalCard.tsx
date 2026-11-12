import { Box, Button, Card, Group, Text } from "@mantine/core";
import type { Approval } from "../../api";

export function ApprovalCard({
  approval,
  busy,
  onDecide,
}: {
  approval: Approval;
  busy: boolean;
  onDecide: (kind: Approval["options"][number]["kind"]) => void;
}) {
  const options =
    approval.options.length > 0
      ? approval.options
      : [
          { id: "deny", kind: "deny" as const, label: "Reject" },
          {
            id: "allow_session",
            kind: "allow_session" as const,
            label: "Always allow",
          },
          { id: "allow_once", kind: "allow_once" as const, label: "Allow once" },
        ];

  return (
    <div className="block">
      <div className="block-gutter" />
      <div className="block-body">
        <Card withBorder p={0} className="approval-card">
          <Group h={27} px={9} gap={6} className="approval-head" wrap="nowrap">
            <span className="dot waiting" />
            <Text size="sm" c="dimmed">
              Permission requested
            </Text>
            <Box style={{ flex: 1 }} />
            <Text size="sm" c="dimmed">
              {approval.agent}
            </Text>
          </Group>
          <Box p={9}>
            <Text size="md" fw={500} mb={4}>
              {approval.title}
            </Text>
            {approval.paths.length ? (
              <Text size="sm" c="dimmed" className="approval-path">
                {approval.paths.join(", ")}
              </Text>
            ) : null}
            {approval.command ? (
              <pre className="cmd">{approval.command}</pre>
            ) : approval.input ? (
              <pre className="cmd">{approval.input}</pre>
            ) : null}
            <Group justify="flex-end" gap={6} mt={8}>
              {options.map((option) => (
                <Button
                  key={option.id}
                  loading={busy}
                  variant={
                    option.kind === "deny"
                      ? "subtle"
                      : option.kind === "allow_once"
                        ? "filled"
                        : "default"
                  }
                  color={option.kind === "allow_once" ? "dark" : "gray"}
                  onClick={() => onDecide(option.kind)}
                >
                  {option.label}
                </Button>
              ))}
            </Group>
          </Box>
        </Card>
      </div>
    </div>
  );
}
