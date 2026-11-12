import { Badge, Card, Group, Stack, Text } from "@mantine/core";
import { IconAlertTriangle } from "@tabler/icons-react";
import type { Approval } from "../../api";
import { SectionHead } from "./SectionHead";

export function ApprovalSummary({ approvals }: { approvals: Approval[] }) {
  return (
    <div>
      <SectionHead
        icon={<IconAlertTriangle size={13} stroke={1.5} />}
        label="Pending approvals"
        count={approvals.length}
      />
      {approvals.length === 0 ? (
        <Text size="sm" c="dimmed" px={10} py="sm">
          Nothing waiting on you
        </Text>
      ) : (
        <Stack gap={4} p={6}>
          {approvals.map((approval) => (
            <Card key={approval.id} withBorder p={6} className="approval-card">
              <Group gap={6} wrap="nowrap">
                <Text size="sm" flex={1} lineClamp={1}>
                  {approval.title}
                </Text>
                <Badge size="xs" variant="light" color="orange">
                  {approval.kind}
                </Badge>
              </Group>
            </Card>
          ))}
        </Stack>
      )}
    </div>
  );
}
