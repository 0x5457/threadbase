import {
  Box,
  Group,
  Loader,
  Paper,
  Text,
  UnstyledButton,
} from "@mantine/core";
import { IconCommand, IconFile, IconSparkles } from "@tabler/icons-react";
import type { Suggestion, SuggestionOption } from "./commands";

function iconFor(kind: SuggestionOption["kind"]) {
  if (kind === "file") return <IconFile size={13} stroke={1.5} />;
  if (kind === "skill") return <IconSparkles size={13} stroke={1.5} />;
  return <IconCommand size={13} stroke={1.5} />;
}

export function SuggestionMenu({
  suggestion,
  options,
  active,
  loading,
  onActive,
  onChoose,
}: {
  suggestion: Suggestion;
  options: SuggestionOption[];
  active: number;
  loading: boolean;
  onActive: (index: number) => void;
  onChoose: (option: SuggestionOption) => void;
}) {
  return (
    <Paper className="composer-suggest" withBorder shadow="md" p={0}>
      <Group className="suggest-head" justify="space-between" px={8} py={4}>
        <Text size="xs" c="dimmed">
          {suggestion.kind === "file" ? "Files" : "Commands & skills"}
        </Text>
        <Text size="xs" c="dimmed" ff="monospace">
          {suggestion.query}
        </Text>
      </Group>
      <Box className="suggest-list">
        {options.map((option, index) => (
          <UnstyledButton
            key={option.id}
            className={`suggest-item${index === active ? " active" : ""}`}
            onMouseEnter={() => onActive(index)}
            onClick={() => onChoose(option)}
          >
            <Box className="suggest-icon">{iconFor(option.kind)}</Box>
            <Box className="suggest-body">
              <Text size="sm" className="suggest-label">
                {option.label}
              </Text>
              {option.description ? (
                <Text size="xs" c="dimmed" className="suggest-desc">
                  {option.description}
                </Text>
              ) : null}
            </Box>
            {option.source ? (
              <Text size="xs" c="dimmed" className="suggest-source">
                {option.source}
              </Text>
            ) : null}
          </UnstyledButton>
        ))}
        {loading ? (
          <Group justify="center" py="sm">
            <Loader size="xs" />
          </Group>
        ) : null}
        {!loading && options.length === 0 ? (
          <Text size="sm" c="dimmed" p="sm">
            No matches
          </Text>
        ) : null}
      </Box>
      <Group className="suggest-foot" px={8} py={3}>
        <Text size="xs" c="dimmed">
          ↑↓ navigate · ↵ insert · esc close
        </Text>
      </Group>
    </Paper>
  );
}
