import {
  Box,
  Group,
  Loader,
  Popover,
  ScrollArea,
  Text,
  TextInput,
  UnstyledButton,
} from "@mantine/core";
import { IconCheck, IconChevronDown, IconSearch } from "@tabler/icons-react";
import { useEffect, useMemo, useRef, useState } from "react";

export interface ModelChoice {
  value: string;
  name: string;
}

interface ModelPickerProps {
  /** Currently effective value; empty/null means the agent default. */
  value: string | null;
  options: ModelChoice[];
  loading?: boolean;
  disabled?: boolean;
  onChange?: (value: string) => void;
  /** Label shown when nothing is selected, for example "Default effort". */
  placeholder?: string;
  /** Accessible name; "Model" for the model picker, "Effort" for the other. */
  label?: string;
}

function normalize(value: string) {
  return value.trim().toLowerCase();
}

/** Agents prefix option names with the setting they belong to
 *  ("Thinking: high", "Model: 6 Luna"). The picker already names that setting,
 *  so the prefix is noise. */
function stripPrefix(value: string) {
  return value.replace(
    /^(thinking|reasoning|effort|model|mode)\s*:\s*/i,
    "",
  );
}

/**
 * Compact model picker with client-side search. The agent's advertised
 * configuration options are the source of truth; when it advertises none the
 * field accepts a custom model id so the picker never becomes a dead end.
 */
export function ModelPicker({
  value,
  options,
  loading = false,
  disabled = false,
  onChange,
  placeholder = "Default model",
  label: name = "Model",
}: ModelPickerProps) {
  const [opened, setOpened] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);
  const listboxId = `model-picker-listbox-${name.toLowerCase()}`;

  const choices = useMemo(() => {
    const seen = new Map<string, ModelChoice>();
    for (const option of options) {
      if (!seen.has(option.value)) {
        seen.set(option.value, {
          value: option.value,
          name: stripPrefix(option.name?.trim() || option.value),
        });
      }
    }
    if (value && !seen.has(value)) {
      seen.set(value, { value, name: value });
    }
    return Array.from(seen.values());
  }, [options, value, name]);

  const queryText = query.trim();
  const needle = normalize(queryText);
  const filtered = useMemo(() => {
    if (!needle) return choices;
    return choices
      .filter(
        (choice) =>
          normalize(choice.name).includes(needle) ||
          normalize(choice.value).includes(needle),
      )
      .sort((left, right) => {
        const leftStarts =
          normalize(left.name).startsWith(needle) ||
          normalize(left.value).startsWith(needle);
        const rightStarts =
          normalize(right.name).startsWith(needle) ||
          normalize(right.value).startsWith(needle);
        return Number(rightStarts) - Number(leftStarts);
      });
  }, [choices, needle]);

  const custom =
    choices.length > 0 &&
    queryText.length > 0 &&
    !choices.some(
      (choice) =>
        choice.value === queryText ||
        normalize(choice.name) === normalize(queryText),
    );
  const total = filtered.length + (custom ? 1 : 0);

  const selected = choices.find((choice) => choice.value === value);
  const label =
    selected?.name ??
    value ??
    (loading && choices.length === 0 ? "Loading…" : placeholder);

  useEffect(() => {
    setActive(0);
  }, [needle]);

  useEffect(() => {
    if (!opened) {
      setQuery("");
      setActive(0);
      return;
    }
    const frame = requestAnimationFrame(() => searchRef.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [opened]);

  useEffect(() => {
    if (active >= total) setActive(Math.max(0, total - 1));
  }, [active, total]);

  useEffect(() => {
    if (!opened) return;
    const element = document.getElementById(`${listboxId}-option-${active}`);
    element?.scrollIntoView({ block: "nearest" });
  }, [active, opened]);

  const choose = (model: string) => {
    setOpened(false);
    if (model !== (value ?? "")) onChange?.(model);
  };

  const chooseActive = () => {
    if (active < filtered.length) {
      const choice = filtered[active];
      if (choice) choose(choice.value);
      return;
    }
    if (custom) choose(queryText);
  };

  return (
    <Popover
      opened={opened}
      onChange={setOpened}
      position="top-end"
      width={296}
      shadow="md"
      trapFocus
      returnFocus
      disabled={disabled}
    >
      <Popover.Target>
        <UnstyledButton
          type="button"
          className="model-picker"
          disabled={disabled}
          aria-label={`${name}: ${label}`}
          aria-haspopup="listbox"
          aria-expanded={opened}
          onClick={() => setOpened((current) => !current)}
        >
          <Text
            size="sm"
            c={value ? undefined : "dimmed"}
            className="model-picker-label"
          >
            {label}
          </Text>
          <IconChevronDown size={10} stroke={1.5} className="model-picker-chevron" />
        </UnstyledButton>
      </Popover.Target>
      <Popover.Dropdown p={6} className="model-picker-dropdown">
        <TextInput
          ref={searchRef}
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              if (total > 0) {
                setActive((current) => Math.min(current + 1, total - 1));
              }
            } else if (event.key === "ArrowUp") {
              event.preventDefault();
              setActive((current) => Math.max(current - 1, 0));
            } else if (event.key === "Enter") {
              event.preventDefault();
              chooseActive();
            }
          }}
          placeholder={`Search ${name.toLowerCase()}`}
          aria-label={`Search ${name.toLowerCase()}`}
          aria-controls={listboxId}
          aria-activedescendant={
            total > 0 ? `${listboxId}-option-${active}` : undefined
          }
          size="xs"
          leftSection={<IconSearch size={12} stroke={1.5} />}
        />
        <ScrollArea.Autosize mah={260} mt={4} type="auto">
          <Box id={listboxId} role="listbox" aria-label={`${name}s`}>
            {filtered.map((choice, index) => {
              const isActive = index === active;
              const isSelected = choice.value === value;
              return (
                <UnstyledButton
                  key={choice.value}
                  id={`${listboxId}-option-${index}`}
                  type="button"
                  role="option"
                  aria-selected={isSelected}
                  className={`model-picker-option${isActive ? " active" : ""}`}
                  onMouseEnter={() => setActive(index)}
                  onClick={() => choose(choice.value)}
                >
                  <Box className="model-picker-option-body">
                    <Text size="sm" className="model-picker-option-name">
                      {choice.name}
                    </Text>
                    {choice.name !== choice.value ? (
                      <Text
                        size="xs"
                        c="dimmed"
                        className="model-picker-option-value"
                      >
                        {choice.value}
                      </Text>
                    ) : null}
                  </Box>
                  {isSelected ? (
                    <IconCheck
                      size={12}
                      stroke={1.5}
                      className="model-picker-check"
                    />
                  ) : null}
                </UnstyledButton>
              );
            })}
            {filtered.length === 0 && loading ? (
              <Group justify="center" gap={6} py="sm">
                <Loader size="xs" />
                <Text size="xs" c="dimmed">
                  Reading agent models…
                </Text>
              </Group>
            ) : null}
            {filtered.length === 0 && !loading ? (
              <Text size="xs" c="dimmed" className="model-picker-empty">
                {choices.length === 0
                  ? "This agent does not advertise models. Type a model id and press Enter."
                  : "No matching models."}
              </Text>
            ) : null}
            {custom ? (
              <UnstyledButton
                id={`${listboxId}-option-${filtered.length}`}
                type="button"
                role="option"
                aria-selected={false}
                className={`model-picker-option custom${
                  active === filtered.length ? " active" : ""
                }`}
                onMouseEnter={() => setActive(filtered.length)}
                onClick={() => choose(queryText)}
              >
                <Box className="model-picker-option-body">
                  <Text size="sm" className="model-picker-option-name">
                    Use {queryText}
                  </Text>
                  <Text size="xs" c="dimmed">
                    Set this custom model id
                  </Text>
                </Box>
              </UnstyledButton>
            ) : null}
          </Box>
        </ScrollArea.Autosize>
        <Group className="model-picker-foot" gap={6} px={4} pt={4}>
          <Text size="xs" c="dimmed">
            ↑↓ navigate · ↵ choose · esc close
          </Text>
        </Group>
      </Popover.Dropdown>
    </Popover>
  );
}
