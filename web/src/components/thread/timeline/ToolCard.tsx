import {
  ChevronRight,
  MessageSquare,
} from "lucide-react";
import { ActionIcon, Badge, Box, Flex, Paper, Text } from "@mantine/core";
import type { CommentAnchor } from "@/api/types/CommentAnchor";
import type { ToolCallData } from "@/api/types/ToolCallData";
import { TOOL_ICONS, toolStatusColor } from "./toolIcons";

export interface ToolCardProps {
  call: ToolCallData;
  onSelect?: () => void;
  onComment?: (anchor: CommentAnchor) => void;
}

/** One tool invocation: title row, touched locations, diff and raw I/O. */
export function ToolCard({ call, onSelect, onComment }: ToolCardProps) {
  const Icon = TOOL_ICONS[call.kind];
  return (
    <Flex
      gap="md"
      onClick={onSelect}
    >
      <Box w={28} />
      <Paper
        withBorder
        radius="md"
        flex={1}
        miw={0}
        style={{ overflow: "hidden" }}
      >
        <Flex align="center" gap="xs" px="sm" py="xs">
          <Icon
            size={14}
            aria-hidden
            style={{
              color: "var(--mantine-color-dimmed)",
              flexShrink: 0,
            }}
          />
          <Text size="sm" fw={500} truncate="end" flex={1} miw={0}>
            {call.title}
          </Text>
          {onComment ? (
            <ActionIcon
              variant="subtle"
              color="gray"
              size="xs"
              aria-label="Comment on tool call"
              style={{ marginLeft: "auto" }}
              onClick={() =>
                onComment?.({ kind: "tool_call", toolCallId: call.id })
              }
            >
              <MessageSquare size={12} aria-hidden />
            </ActionIcon>
          ) : null}
          <Badge
            variant="light"
            color={toolStatusColor(call.status)}
            size="xs"
            style={onComment ? undefined : { marginLeft: "auto" }}
          >
            {call.status.replace("_", " ")}
          </Badge>
        </Flex>
        {call.locations.length > 0 ? (
          <Box
            component="ul"
            px="sm"
            py="xs"
            style={{
              borderTop:
                "1px solid var(--mantine-color-default-border)",
              listStyle: "none",
              margin: 0,
            }}
          >
            {call.locations.map((location) => (
              <li
                key={`${location.path}:${location.line ?? 0}`}
                style={{
                  fontFamily: "var(--mantine-font-family-monospace)",
                  fontSize: "var(--mantine-font-size-md)",
                  color: "var(--mantine-color-dimmed)",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {location.path}
                {location.line ? `:${location.line}` : ""}
              </li>
            ))}
          </Box>
        ) : null}
        {call.diff ? (
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
                borderTop:
                  "1px solid var(--mantine-color-default-border)",
              }}
            >
              <Text
                size="sm"
                ff="monospace"
                c="dimmed"
                truncate="end"
                flex={1}
                miw={0}
              >
                {call.diff.path}
              </Text>
              <Text
                size="sm"
                ff="monospace"
                c="green.6"
                style={{ marginLeft: "auto", whiteSpace: "nowrap" }}
              >
                +{call.diff.additions}
              </Text>
              <Text
                size="sm"
                ff="monospace"
                c="red.6"
                style={{ whiteSpace: "nowrap" }}
              >
                −{call.diff.deletions}
              </Text>
              <ChevronRight
                size={12}
                aria-hidden
                style={{
                  color: "var(--mantine-color-dimmed)",
                  flexShrink: 0,
                }}
              />
            </summary>
            {call.diff.patch ? (
              <Box
                component="pre"
                p="xs"
                mah={256}
                style={{
                  margin: 0,
                  overflow: "auto",
                  fontFamily: "var(--mantine-font-family-monospace)",
                  fontSize: "var(--mantine-font-size-md)",
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-all",
                  borderTop:
                    "1px solid var(--mantine-color-default-border)",
                }}
              >
                {call.diff.patch}
              </Box>
            ) : null}
          </details>
        ) : null}
        {call.input || call.output ? (
          <details
            style={{
              borderTop:
                "1px solid var(--mantine-color-default-border)",
              fontSize: "var(--mantine-font-size-md)",
              color: "var(--mantine-color-dimmed)",
            }}
          >
            <summary
              style={{
                display: "flex",
                padding: "var(--mantine-spacing-xs) var(--mantine-spacing-sm)",
                listStyle: "none",
                cursor: "pointer",
                color: "var(--mantine-color-dimmed)",
              }}
            >
              Details
            </summary>
            {call.input ? (
              <Box
                component="pre"
                p="xs"
                mah={192}
                style={{
                  margin: 0,
                  overflow: "auto",
                  fontFamily: "var(--mantine-font-family-monospace)",
                  fontSize: "var(--mantine-font-size-md)",
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-all",
                  background: "var(--mantine-color-default)",
                  borderRadius: "var(--mantine-radius-md)",
                }}
              >
                {call.input}
              </Box>
            ) : null}
            {call.output ? (
              <Box
                component="pre"
                p="xs"
                mah={256}
                style={{
                  margin: 0,
                  overflow: "auto",
                  fontFamily: "var(--mantine-font-family-monospace)",
                  fontSize: "var(--mantine-font-size-md)",
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-all",
                  background: "var(--mantine-color-default)",
                  borderRadius: "var(--mantine-radius-md)",
                }}
              >
                {call.output}
              </Box>
            ) : null}
          </details>
        ) : null}
      </Paper>
    </Flex>
  );
}
