import { ArrowLeftRight, ChevronRight } from "lucide-react";
import { Box, Flex, Text } from "@mantine/core";
import type { AgentMessageItem } from "./types";

/**
 * Agent-to-agent message: a collapsed row that expands to the payload.
 * Uses the same 28px gutter as every other timeline row.
 */
export function AgentMessageRow({ item }: { item: AgentMessageItem }) {
  const label =
    item.direction === "sent"
      ? `Sent Agent Message to ${item.to}`
      : `Received Agent Message from ${item.from}`;

  return (
    <Flex gap="md">
      <Box w={28} />
      <Box
        flex={1}
        miw={0}
        style={{
          border: "1px solid var(--mantine-color-default-border)",
          borderRadius: "var(--mantine-radius-md)",
          background: "var(--mantine-color-default)",
          overflow: "hidden",
        }}
      >
        <details>
          <summary
            style={{
              display: "flex",
              alignItems: "center",
              gap: "0.5rem",
              padding: "var(--mantine-spacing-xs) var(--mantine-spacing-sm)",
              fontSize: "var(--mantine-font-size-md)",
              listStyle: "none",
              cursor: "pointer",
              color: "var(--mantine-color-dimmed)",
            }}
          >
            <ArrowLeftRight
              size={14}
              aria-hidden
              style={{ color: "var(--mantine-color-dimmed)", flexShrink: 0 }}
            />
            <Text size="sm" fw={500} c="dimmed" truncate="end" flex={1} miw={0}>
              {label}
            </Text>
            <ChevronRight
              size={12}
              data-disclosure-chevron
              aria-hidden
              style={{
                color: "var(--mantine-color-dimmed)",
                flexShrink: 0,
                marginLeft: "auto",
              }}
            />
          </summary>
          <Text
            size="sm"
            p="sm"
            style={{
              whiteSpace: "pre-wrap",
              lineHeight: 1.6,
              borderTop: "1px solid var(--mantine-color-default-border)",
            }}
          >
            {item.text}
          </Text>
        </details>
      </Box>
    </Flex>
  );
}
