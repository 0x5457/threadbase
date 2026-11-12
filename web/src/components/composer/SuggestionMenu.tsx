import {
  Box,
  Group,
  Loader,
  Paper,
  Text,
  UnstyledButton,
} from "@mantine/core";
import { IconCommand, IconFile, IconSparkles } from "@tabler/icons-react";
import { forwardRef, useEffect, useImperativeHandle, useState } from "react";
import type { SuggestionOption } from "./commands";

export interface SuggestionMenuHandle {
  onKeyDown: (event: KeyboardEvent) => boolean;
}

export interface SuggestionMenuProps {
  kind: "command" | "file";
  query: string;
  options: SuggestionOption[];
  loading: boolean;
  onChoose: (option: SuggestionOption) => void;
}

function iconFor(kind: SuggestionOption["kind"]) {
  if (kind === "file") return <IconFile size={13} stroke={1.5} />;
  if (kind === "skill") return <IconSparkles size={13} stroke={1.5} />;
  return <IconCommand size={13} stroke={1.5} />;
}

const OPTION_ID = "composer-suggestion-option";

export const SuggestionMenu = forwardRef<
  SuggestionMenuHandle,
  SuggestionMenuProps
>(function SuggestionMenu(
  { kind, query, options, loading, onChoose },
  ref,
) {
  const [active, setActive] = useState(0);

  useEffect(() => {
    setActive(0);
  }, [options, query]);

  useEffect(() => {
    if (active >= options.length) {
      setActive(Math.max(0, options.length - 1));
    }
  }, [active, options.length]);

  useImperativeHandle(
    ref,
    () => ({
      onKeyDown: (event) => {
        if (event.isComposing) return false;
        if (event.key === "ArrowDown") {
          event.preventDefault();
          event.stopPropagation();
          setActive((current) =>
            options.length === 0
              ? 0
              : Math.min(current + 1, options.length - 1),
          );
          return true;
        }
        if (event.key === "ArrowUp") {
          event.preventDefault();
          event.stopPropagation();
          setActive((current) => Math.max(current - 1, 0));
          return true;
        }
        if (
          event.key === "Enter" &&
          !event.metaKey &&
          !event.ctrlKey &&
          !event.shiftKey
        ) {
          event.preventDefault();
          event.stopPropagation();
          const option = options[active];
          if (option) onChoose(option);
          return true;
        }
        return false;
      },
    }),
    [active, onChoose, options],
  );

  return (
    <Paper className="composer-suggest" withBorder shadow="md" p={0}>
      <Group className="suggest-head" justify="space-between" px={8} py={4}>
        <Text size="xs" c="dimmed">
          {kind === "file" ? "Files" : "Commands & skills"}
        </Text>
        <Text size="xs" c="dimmed" ff="monospace">
          {query}
        </Text>
      </Group>
      <Box
        className="suggest-list"
        role="listbox"
        aria-label={kind === "file" ? "Files" : "Commands and skills"}
        aria-activedescendant={
          options.length > 0 ? `${OPTION_ID}-${active}` : undefined
        }
      >
        {options.map((option, index) => (
          <UnstyledButton
            key={option.id}
            id={`${OPTION_ID}-${index}`}
            type="button"
            role="option"
            aria-selected={index === active}
            className={`suggest-item${index === active ? " active" : ""}`}
            onMouseEnter={() => setActive(index)}
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
});
