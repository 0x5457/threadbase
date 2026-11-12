import { useDebouncedValue } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import type { Editor } from "@tiptap/react";
import { useCallback, useEffect, useMemo } from "react";
import { listSkills, searchFiles } from "../../api";
import {
  commandsFor,
  type Suggestion,
  type SuggestionOption,
} from "./commands";

interface UseComposerSuggestionsOptions {
  editor: Editor | null;
  threadId?: string;
  projectId?: string;
  agent: string;
  suggestion: Suggestion | null;
  setSuggestion: (suggestion: Suggestion | null) => void;
  active: number;
  setActive: (updater: number | ((current: number) => number)) => void;
}

/** Feeds the `@file` / `/command` menu and owns its keyboard interaction. */
export function useComposerSuggestions({
  editor,
  threadId,
  projectId,
  agent,
  suggestion,
  setSuggestion,
  active,
  setActive,
}: UseComposerSuggestionsOptions) {
  const skills = useQuery({
    queryKey: ["skills", projectId],
    queryFn: () => listSkills(projectId as string),
    enabled: Boolean(projectId),
    staleTime: 60_000,
    retry: false,
  });

  const [fileQuery] = useDebouncedValue(
    suggestion?.kind === "file" ? suggestion.query : "",
    160,
  );
  const files = useQuery({
    queryKey: ["file-search", threadId, fileQuery],
    queryFn: () => searchFiles(threadId as string, fileQuery),
    enabled: Boolean(threadId && suggestion?.kind === "file"),
    staleTime: 5_000,
    placeholderData: keepPreviousData,
  });

  const options = useMemo<SuggestionOption[]>(() => {
    if (!suggestion) return [];
    const query = suggestion.query.toLowerCase();
    if (suggestion.kind === "file") {
      return (files.data?.files ?? []).map((file) => ({
        id: `file:${file}`,
        kind: "file" as const,
        label: file,
        description: "Add a file reference",
        insert: `@${file} `,
      }));
    }

    const commandOptions = commandsFor(agent)
      .filter(
        (command) =>
          !query || command.command.toLowerCase().includes(query),
      )
      .map((command) => ({
        id: `command:${command.command}`,
        kind: "command" as const,
        label: `/${command.command}`,
        description: command.description,
        insert: `/${command.command} `,
      }));
    const skillOptions = (skills.data?.skills ?? [])
      .filter(
        (skill) =>
          !query ||
          skill.name.toLowerCase().includes(query) ||
          skill.description.toLowerCase().includes(query),
      )
      .map((skill) => ({
        id: `skill:${skill.path}`,
        kind: "skill" as const,
        label: `/${skill.name}`,
        description: skill.description,
        source: skill.source,
        insert: `/${skill.name} `,
      }));
    return [...commandOptions, ...skillOptions].slice(0, 24);
  }, [agent, files.data, suggestion, skills.data]);

  useEffect(() => {
    setActive(0);
  }, [setActive, suggestion?.kind, suggestion?.query]);

  useEffect(() => {
    if (active >= options.length) {
      setActive(Math.max(0, options.length - 1));
    }
  }, [active, options.length, setActive]);

  const choose = useCallback(
    (option: SuggestionOption) => {
      if (!editor || !suggestion) return;
      const to = editor.state.selection.from;
      editor
        .chain()
        .focus()
        .deleteRange({ from: suggestion.from, to })
        .insertContent(option.insert)
        .run();
      setSuggestion(null);
    },
    [editor, setSuggestion, suggestion],
  );

  useEffect(() => {
    if (!suggestion) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.isComposing) return;
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        setSuggestion(null);
        return;
      }
      if (event.key === "ArrowDown") {
        event.preventDefault();
        event.stopPropagation();
        setActive((current) => Math.min(current + 1, options.length - 1));
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        event.stopPropagation();
        setActive((current) => Math.max(current - 1, 0));
        return;
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
        if (option) choose(option);
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [active, choose, options, setActive, setSuggestion, suggestion]);

  const openTrigger = (kind: "command" | "file") => {
    if (!editor) return;
    if (kind === "file" && !threadId) {
      notifications.show({
        message: "Open a thread to reference files",
        color: "yellow",
      });
      return;
    }
    editor
      .chain()
      .focus()
      .insertContent(kind === "file" ? "@" : "/")
      .run();
  };

  const loading =
    suggestion?.kind === "file" ? files.isFetching : skills.isFetching;

  return { options, loading, choose, openTrigger };
}
