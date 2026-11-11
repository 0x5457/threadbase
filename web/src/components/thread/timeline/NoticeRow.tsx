import {
  AlertTriangle,
  ArrowLeftRight,
  CircleDot,
  ShieldAlert,
} from "lucide-react";
import { Box, Flex, Text } from "@mantine/core";
import type { NoticeItem } from "./types";

const TONE_COLOR: Record<NoticeItem["tone"], string> = {
  danger: "var(--mantine-color-red-filled)",
  warning: "var(--mantine-color-yellow-filled)",
  info: "var(--mantine-color-blue-filled)",
  muted: "var(--mantine-color-dimmed)",
};

const TONE_ICON = {
  danger: AlertTriangle,
  warning: ShieldAlert,
  info: ArrowLeftRight,
  muted: CircleDot,
} as const;

/**
 * A single-line timeline meta row (usage, turn ended, routing, …).
 * Deliberately borderless: these must read as prose, not as cards.
 */
export function NoticeRow({ item }: { item: NoticeItem }) {
  const Icon = TONE_ICON[item.tone];
  return (
    <Flex gap="md">
      <Box w={28} />
      <Flex align="center" gap={6} py={2} flex={1} miw={0}>
        <Icon
          size={12}
          aria-hidden
          style={{ color: TONE_COLOR[item.tone], flexShrink: 0 }}
        />
        <Text
          size="xs"
          style={{
            whiteSpace: "pre-wrap",
            overflowWrap: "break-word",
            lineHeight: 1.5,
          }}
        >
          <Text component="span" fw={500}>
            {item.label}
          </Text>
          {item.text ? (
            <Text component="span" c="dimmed">
              {" "}
              · {item.text}
            </Text>
          ) : null}
        </Text>
      </Flex>
    </Flex>
  );
}
