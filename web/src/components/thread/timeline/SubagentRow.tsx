import { ChevronRight, GitBranch, XCircle } from "lucide-react";
import { Box, Button, Flex, Stack, Text } from "@mantine/core";
import type { SubagentDto } from "@/api/types/SubagentDto";
import { SubagentStatusBadge } from "@/components/thread/SubagentStatusBadge";

export interface SubagentRowProps {
  subagent: SubagentDto;
  onOpen?: (subagentId: string) => void;
}

/** Dispatch card for a subagent session, collapsed to one summary line. */
export function SubagentRow({ subagent, onOpen }: SubagentRowProps) {
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
              listStyle: "none",
              cursor: "pointer",
            }}
          >
            <GitBranch
              size={14}
              aria-hidden
              style={{ color: "var(--mantine-color-dimmed)", flexShrink: 0 }}
            />
            <Text size="sm" fw={500} truncate="end" flex={1} miw={0}>
              {subagent.title}
            </Text>
            <SubagentStatusBadge status={subagent.status} />
            <ChevronRight
              size={12}
              aria-hidden
              style={{
                color: "var(--mantine-color-dimmed)",
                flexShrink: 0,
                marginLeft: "auto",
              }}
            />
          </summary>
          <Box
            p="sm"
            style={{ borderTop: "1px solid var(--mantine-color-default-border)" }}
          >
            <Stack gap={4}>
              {subagent.description ? (
                <Text size="sm" style={{ whiteSpace: "pre-wrap" }}>
                  {subagent.description}
                </Text>
              ) : null}
              {subagent.status === "error" ? (
                <Flex align="center" gap="xs">
                  <XCircle
                    size={12}
                    aria-hidden
                    style={{ color: "var(--mantine-color-red-6)" }}
                  />
                  <Text size="sm" c="red.6">
                    Stopped with an error
                  </Text>
                </Flex>
              ) : null}
              {subagent.cwd ? (
                <Text
                  fz={12}
                  ff="monospace"
                  c="dimmed"
                  style={{ wordBreak: "break-all" }}
                >
                  {subagent.cwd}
                </Text>
              ) : (
                <Text size="sm" c="dimmed">
                  No independent session
                </Text>
              )}
              {subagent.usage ? (
                <Text size="sm" c="dimmed">
                  {subagent.usage.totalTokens.toLocaleString()} tokens
                </Text>
              ) : null}
              {onOpen ? (
                <Button
                  variant="subtle"
                  color="dark"
                  size="xs"
                  px={0}
                  onClick={() => onOpen(subagent.id)}
                >
                  Open subagent
                </Button>
              ) : null}
            </Stack>
          </Box>
        </details>
      </Box>
    </Flex>
  );
}
