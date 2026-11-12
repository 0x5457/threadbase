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
import {
  OTHER_GROUP,
  RECENT_GROUP,
  groupByProvider,
  readRecent,
  recentValues,
  rememberRecent,
  sortByProvider,
  usePickerAgent,
} from "../lib/model-prefs";

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
  /** Called when the dropdown opens, before any choice is made. */
  onOpen?: () => void;
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
 *
 * Long lists (pi advertises 700+) are split into provider groups, with the last
 * few picks pinned on top; short single-provider lists (the effort picker) keep
 * the flat layout.
 */
export function ModelPicker({
  value,
  options,
  loading = false,
  disabled = false,
  onChange,
  onOpen,
  placeholder = "Default model",
  label: name = "Model",
}: ModelPickerProps) {
  const [opened, setOpened] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [recent, setRecent] = useState<string[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);
  const listboxId = `model-picker-listbox-${name.toLowerCase()}`;
  const agent = usePickerAgent();

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
  }, [options, value]);

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

  const byValue = useMemo(
    () => new Map(choices.map((choice) => [choice.value, choice])),
    [choices],
  );

  /**
   * Grouping only pays off when the list really spans providers: the effort
   * picker's low/medium/high would otherwise grow a meaningless "Other" header
   * and a Recent group duplicating the whole list.
   */
  const grouped = useMemo(() => {
    const providers = groupByProvider(sortByProvider(choices));
    return (
      providers.length > 0 &&
      !(providers.length === 1 && providers[0]?.label === OTHER_GROUP)
    );
  }, [choices]);

  /** Top of the list: the current selection pinned, then the last picks. A pick
   *  the agent no longer advertises is still offered, the way a custom id is,
   *  so the history does not shift under the user. */
  const recentChoices = useMemo(() => {
    if (!grouped) return [];
    return recentValues(recent, value)
      .map(
        (recentValue) =>
          byValue.get(recentValue) ?? { value: recentValue, name: recentValue },
      )
      .filter(
        (choice) =>
          !needle ||
          normalize(choice.name).includes(needle) ||
          normalize(choice.value).includes(needle),
      );
  }, [byValue, grouped, needle, recent, value]);

  /** Provider buckets in render order: the advertised order when idle, the
   *  match order while searching so the best hits lead. */
  const groups = useMemo(
    () =>
      grouped ? groupByProvider(needle ? filtered : sortByProvider(choices)) : [],
    [choices, filtered, grouped, needle],
  );

  /** Sections in render order, each knowing where it starts in the flat order.
   *  A null label is the ungrouped list of a short picker. */
  const sections = useMemo(() => {
    const built: {
      label: string | null;
      items: ModelChoice[];
      offset: number;
    }[] = [];
    let offset = 0;
    const add = (label: string | null, items: ModelChoice[]) => {
      if (items.length === 0) return;
      built.push({ label, items, offset });
      offset += items.length;
    };
    if (!grouped) {
      add(null, filtered);
      return built;
    }
    add(RECENT_GROUP, recentChoices);
    for (const group of groups) add(group.label, group.items);
    return built;
  }, [filtered, groups, grouped, recentChoices]);

  /** Flat order the keyboard walks: group headers are skipped, so ↑/↓ cross
   *  provider boundaries like they crossed options before. */
  const visible = useMemo(
    () => sections.flatMap((section) => section.items),
    [sections],
  );

  const custom =
    choices.length > 0 &&
    queryText.length > 0 &&
    !choices.some(
      (choice) =>
        choice.value === queryText ||
        normalize(choice.name) === normalize(queryText),
    );
  const total = visible.length + (custom ? 1 : 0);

  const selected = choices.find((choice) => choice.value === value);
  const label =
    selected?.name ??
    value ??
    (loading && choices.length === 0 ? "Loading…" : placeholder);

  useEffect(() => {
    setActive(0);
  }, [needle]);

  // Recents only change through this picker, so one read per open is enough.
  useEffect(() => {
    if (opened) setRecent(readRecent(agent, name));
  }, [agent, name, opened]);

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
    setRecent(rememberRecent(agent, name, model));
    if (model !== (value ?? "")) onChange?.(model);
  };

  const chooseActive = () => {
    if (active < visible.length) {
      const choice = visible[active];
      if (choice) choose(choice.value);
      return;
    }
    if (custom) choose(queryText);
  };

  const renderOption = (choice: ModelChoice, index: number) => {
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
            <Text size="xs" c="dimmed" className="model-picker-option-value">
              {choice.value}
            </Text>
          ) : null}
        </Box>
        {isSelected ? (
          <IconCheck size={12} stroke={1.5} className="model-picker-check" />
        ) : null}
      </UnstyledButton>
    );
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
      onOpen={onOpen}
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
        <ScrollArea.Autosize
          mah={260}
          mt={4}
          type="auto"
          styles={{ content: { minWidth: 0 } }}
        >
          <Box id={listboxId} role="listbox" aria-label={`${name}s`}>
            {sections.map((section) => (
              <Box
                key={section.label ?? "all"}
                className="model-picker-group"
                role={section.label ? "group" : "presentation"}
                aria-label={section.label ?? undefined}
              >
                {section.label ? (
                  <Text
                    size="xs"
                    aria-hidden
                    className="model-picker-group-label"
                  >
                    {section.label}
                  </Text>
                ) : null}
                {section.items.map((choice, index) =>
                  renderOption(choice, section.offset + index),
                )}
              </Box>
            ))}
            {visible.length === 0 && loading ? (
              <Group justify="center" gap={6} py="sm">
                <Loader size="xs" />
                <Text size="xs" c="dimmed">
                  Reading agent models…
                </Text>
              </Group>
            ) : null}
            {visible.length === 0 && !loading ? (
              <Text size="xs" c="dimmed" className="model-picker-empty">
                {choices.length === 0
                  ? "This agent does not advertise models. Type a model id and press Enter."
                  : "No matching models."}
              </Text>
            ) : null}
            {custom ? (
              <UnstyledButton
                id={`${listboxId}-option-${visible.length}`}
                type="button"
                role="option"
                aria-selected={false}
                className={`model-picker-option custom${
                  active === visible.length ? " active" : ""
                }`}
                onMouseEnter={() => setActive(visible.length)}
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
