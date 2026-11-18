import {
  Box,
  Button,
  Combobox,
  Group,
  ScrollArea,
  Text,
  UnstyledButton,
  useCombobox,
} from "@mantine/core";
import { IconCheck, IconChevronDown, IconSearch } from "@tabler/icons-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useDelayedFlag } from "../lib/use-delayed-flag";
import { Bone } from "./loading/Skeleton";
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
  error?: string | null;
  disabled?: boolean;
  onChange?: (value: string) => void;
  /** Called when the dropdown opens, before any choice is made. */
  onOpen?: () => void;
  /** Retry a failed agent-config load. */
  onRetry?: () => void;
  /** Label shown when nothing is selected, for example "Default effort". */
  placeholder?: string;
  /** A compact suffix shown next to the value, for example the effort. */
  badge?: ReactNode;
  /** Extra agent settings rendered under the searchable model list. */
  footer?: ReactNode;
  /** The thread's delivery mode is queue; show the state dot on the trigger. */
  queue?: boolean;
  /** Accessible name for the trigger when the picker owns more than a model. */
  triggerLabel?: string;
  /** Native tooltip explaining what the unified picker controls. */
  triggerTitle?: string;
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
  return value.replace(/^(thinking|reasoning|effort|model|mode)\s*:\s*/i, "");
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
  error = null,
  disabled = false,
  onChange,
  onOpen,
  onRetry,
  placeholder = "Default model",
  label: name = "Model",
  triggerLabel,
  triggerTitle,
  badge,
  footer,
  queue = false,
}: ModelPickerProps) {
  const [opened, setOpened] = useState(false);
  const [query, setQuery] = useState("");
  const triggerRef = useRef<HTMLButtonElement>(null);
  const [recent, setRecent] = useState<string[]>([]);
  const wasOpened = useRef(false);
  const agent = usePickerAgent();

  const combobox = useCombobox({
    opened,
    onOpenedChange: setOpened,
    onDropdownOpen: () => onOpen?.(),
  });

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

  /** Sections in render order. A null label is the ungrouped list of a short
   *  picker. */
  const sections = useMemo(() => {
    const built: { label: string | null; items: ModelChoice[] }[] = [];
    if (!grouped) {
      if (filtered.length > 0) built.push({ label: null, items: filtered });
      return built;
    }
    if (recentChoices.length > 0) {
      built.push({ label: RECENT_GROUP, items: recentChoices });
    }
    for (const group of groups) {
      if (group.items.length > 0) built.push(group);
    }
    return built;
  }, [filtered, groups, grouped, recentChoices]);

  const visible = useMemo(
    () => sections.flatMap((section) => section.items),
    [sections],
  );

  const custom =
    queryText.length > 0 &&
    !choices.some(
      (choice) =>
        choice.value === queryText ||
        normalize(choice.name) === normalize(queryText),
    );

  const selected = choices.find((choice) => choice.value === value);
  const label = selected?.name ?? value ?? placeholder;
  const showLoading = useDelayedFlag(
    loading && visible.length === 0,
    300,
    500,
  );

  /** Keyboard order starts at the first match after every query change. */
  useEffect(() => {
    if (!opened) return;
    combobox.selectFirstOption();
  }, [opened, needle, visible.length]);

  // Recents only change through this picker, so one read per open is enough.
  // Reading localStorage is intentionally tied to the open transition.
  // react-doctor-disable-next-line react-doctor/no-event-handler
  useEffect(() => {
    if (opened) setRecent(readRecent(agent, name));
  }, [agent, name, opened]);

  useEffect(() => {
    if (!opened) {
      setQuery("");
      combobox.resetSelectedOption();
      if (wasOpened.current) combobox.focusTarget();
      wasOpened.current = false;
      return;
    }
    wasOpened.current = true;
    const frame = requestAnimationFrame(() => combobox.searchRef.current?.focus());
    return () => cancelAnimationFrame(frame);
  }, [opened]);

  const choose = (model: string) => {
    setOpened(false);
    setRecent(rememberRecent(agent, name, model));
    if (model !== (value ?? "")) onChange?.(model);
  };

  const renderOption = (choice: ModelChoice) => {
    const isSelected = choice.value === value;
    return (
      <Combobox.Option
        key={choice.value}
        value={choice.value}
        selected={isSelected}
        className="model-picker-option"
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
      </Combobox.Option>
    );
  };

  return (
    <Combobox
      store={combobox}
      onOptionSubmit={choose}
      position="top-end"
      width={296}
      shadow="md"
      disabled={disabled}
      readOnly={disabled}
    >
      <Combobox.Target targetType="button" withExpandedAttribute>
        <UnstyledButton
          ref={triggerRef}
          type="button"
          className="model-picker"
          data-queue={queue ? "true" : undefined}
          disabled={disabled}
          aria-label={triggerLabel ?? `${name}: ${label}`}
          title={triggerTitle}
          onClick={() => combobox.toggleDropdown()}
        >
          <Text
            size="sm"
            c={value ? undefined : "dimmed"}
            className="model-picker-label"
          >
            {label}
          </Text>
          {badge ? <span className="model-picker-effort">{badge}</span> : null}
          <IconChevronDown size={10} stroke={1.5} className="model-picker-chevron" />
        </UnstyledButton>
      </Combobox.Target>
      <Combobox.Dropdown
        p={6}
        className="model-picker-dropdown"
        onKeyDown={(event) => {
          // Escape closes from the effort/delivery chips too, not only from
          // the search field, and returns focus to the trigger.
          if (event.key === "Escape") {
            event.stopPropagation();
            combobox.closeDropdown();
            triggerRef.current?.focus();
          }
        }}
      >
        {/*
         * The catalogue can hold hundreds of models, and Mantine keeps the
         * dropdown mounted (hidden) after its first render. Building every
         * option row while the composer mounts put ~2 900 nodes into a thread
         * that never opened the picker — by far the largest slice of the
         * page's DOM. Only an open picker needs the list.
         */}
        {opened ? (
          <>
        <Combobox.Search
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder={`Search ${name.toLowerCase()}`}
          aria-label={`Search ${name.toLowerCase()}`}
          size="xs"
          leftSection={<IconSearch size={12} stroke={1.5} />}
        />
        <ScrollArea.Autosize
          mah={260}
          mt={4}
          type="auto"
          styles={{ content: { minWidth: 0 } }}
        >
          <Combobox.Options aria-label={`${name}s`}>
            {sections.map((section) =>
              section.label === null ? (
                section.items.map((choice) => renderOption(choice))
              ) : (
                <Combobox.Group key={section.label} label={section.label}>
                  {section.items.map((choice) => renderOption(choice))}
                </Combobox.Group>
              ),
            )}
            {visible.length === 0 && loading && showLoading ? (
              <div className="model-picker-skeleton" aria-busy="true">
                {[0, 1, 2].map((row) => (
                  <div key={row} className="model-picker-skeleton-row">
                    <Bone width={`${62 + row * 8}%`} height={11} />
                    <Bone width={`${34 + row * 5}%`} height={9} />
                  </div>
                ))}
              </div>
            ) : null}
            {visible.length === 0 && !loading && error ? (
              <Group className="model-picker-error" gap={8} p="sm" wrap="nowrap">
                <Text size="xs" c="red" style={{ flex: 1 }}>
                  {error}
                </Text>
                {onRetry ? (
                  <Button
                    variant="default"
                    size="compact-xs"
                    onClick={onRetry}
                  >
                    Retry
                  </Button>
                ) : null}
              </Group>
            ) : null}
            {visible.length === 0 && !loading && !error ? (
              <Combobox.Empty className="model-picker-empty">
                {choices.length === 0
                  ? "This agent does not advertise models. Type a model id and press Enter."
                  : "No matching models."}
              </Combobox.Empty>
            ) : null}
            {custom ? (
              <Combobox.Option value={queryText} className="model-picker-option custom">
                <Box className="model-picker-option-body">
                  <Text size="sm" className="model-picker-option-name">
                    Use {queryText}
                  </Text>
                  <Text size="xs" c="dimmed">
                    Set this custom model id
                  </Text>
                </Box>
              </Combobox.Option>
            ) : null}
          </Combobox.Options>
        </ScrollArea.Autosize>
        {footer ? <div className="model-picker-settings">{footer}</div> : null}
        <Group className="model-picker-foot" gap={6} px={4} pt={4}>
          <Text size="xs" c="dimmed">
            ↑↓ navigate · ↵ choose · esc close
          </Text>
        </Group>
          </>
        ) : null}
      </Combobox.Dropdown>
    </Combobox>
  );
}
