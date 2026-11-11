import { ListChecks } from "lucide-react";
import { Box, Flex, Paper, Text } from "@mantine/core";
import type { PlanEntry } from "@/api/types/PlanEntry";

function statusColor(status: PlanEntry["status"]): string {
  switch (status) {
    case "completed":
      return "var(--mantine-color-green-6)";
    case "in_progress":
      return "var(--mantine-color-yellow-6)";
    default:
      return "var(--mantine-color-default-border)";
  }
}

/** The agent's plan: one dotted list, completed steps struck through. */
export function PlanCard({ entries }: { entries: PlanEntry[] }) {
  return (
    <Flex gap="md">
      <Box w={28} />
      <Paper withBorder radius="md" flex={1} p="sm">
        <Flex align="center" gap="xs" mb="xs">
          <ListChecks
            size={14}
            aria-hidden
            style={{
              color: "var(--mantine-color-dimmed)",
              flexShrink: 0,
            }}
          />
          <Text size="sm" fw={500}>
            Plan
          </Text>
        </Flex>
        <Box component="ul" style={{ listStyle: "none", margin: 0, padding: 0 }}>
          {entries.map((entry) => (
            <Flex
              component="li"
              key={entry.content}
              align="flex-start"
              gap="xs"
              fz="md"
              c="dimmed"
            >
              <span
                aria-hidden
                style={{
                  marginTop: 4,
                  width: 6,
                  height: 6,
                  borderRadius: "50%",
                  flexShrink: 0,
                  background: statusColor(entry.status),
                }}
              />
              <Text
                component="span"
                size="md"
                c={entry.status === "completed" ? "dimmed" : undefined}
                style={
                  entry.status === "completed"
                    ? { textDecoration: "line-through" }
                    : undefined
                }
              >
                {entry.content}
              </Text>
            </Flex>
          ))}
        </Box>
      </Paper>
    </Flex>
  );
}
