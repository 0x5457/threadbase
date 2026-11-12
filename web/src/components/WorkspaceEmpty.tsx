import { Center, Stack, Text } from "@mantine/core";
import { IconMessageCircle } from "@tabler/icons-react";

export function WorkspaceEmpty() {
  return (
    <Center h="100%">
      <Stack align="center" gap={6}>
        <IconMessageCircle size={22} stroke={1.5} color="var(--ink-faint)" />
        <Text size="md" fw={500}>
          No thread selected
        </Text>
        <Text size="sm" c="dimmed" ta="center" maw={320}>
          Pick a thread from the sidebar, or open the Inbox to see what needs
          you.
        </Text>
      </Stack>
    </Center>
  );
}
