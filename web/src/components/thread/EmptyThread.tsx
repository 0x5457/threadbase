import { Box, Text } from "@mantine/core";
import { IconArrowBackUp } from "@tabler/icons-react";

export function EmptyThread({ agent }: { agent: string }) {
  return (
    <Box className="thread-empty">
      <IconArrowBackUp size={18} stroke={1.5} />
      <Text size="md" fw={500}>
        No activity yet
      </Text>
      <Text size="sm" c="dimmed" ta="center" maw={320}>
        Send a message below to start {agent}. Every tool call, approval and
        code change will appear in this thread.
      </Text>
    </Box>
  );
}
