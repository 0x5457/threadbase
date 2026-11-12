import { shift } from "@floating-ui/dom";
import { PluginKey } from "@tiptap/pm/state";
import {
  Extension,
  ReactRenderer,
  type Editor,
} from "@tiptap/react";
import Suggestion, {
  exitSuggestion,
  type SuggestionKeyDownProps,
  type SuggestionProps,
} from "@tiptap/suggestion";
import { notifications } from "@mantine/notifications";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useMemo, useRef, useState } from "react";
import { listSkills, searchFiles } from "../../api";
import { commandsFor, type SuggestionOption } from "./commands";
import {
  SuggestionMenu,
  type SuggestionMenuHandle,
  type SuggestionMenuProps,
} from "./SuggestionMenu";

const COMMAND_PLUGIN_KEY = new PluginKey("composer-command-suggestions");
const FILE_PLUGIN_KEY = new PluginKey("composer-file-suggestions");

const SUGGESTION_LIMIT = 24;

interface SuggestionContext {
  agent: string;
  projectId?: string;
  threadId?: string;
  queryClient: QueryClient;
  onActiveChange: (active: boolean) => void;
}

async function commandOptions(
  context: SuggestionContext,
  query: string,
): Promise<SuggestionOption[]> {
  const needle = query.toLowerCase();
  const commandOptions = commandsFor(context.agent)
    .filter(
      (command) => !needle || command.command.toLowerCase().includes(needle),
    )
    .map((command) => ({
      id: `command:${command.command}`,
      kind: "command" as const,
      label: `/${command.command}`,
      description: command.description,
      insert: `/${command.command} `,
    }));

  let skills: { name: string; description: string; path: string; source: string }[] = [];
  if (context.projectId) {
    try {
      const response = await context.queryClient.fetchQuery({
        queryKey: ["skills", context.projectId],
        queryFn: () => listSkills(context.projectId as string),
        staleTime: 60_000,
        retry: false,
      });
      skills = response.skills;
    } catch {
      // A failed lookup leaves the commands usable; the menu just has no skills.
    }
  }

  const skillOptions = skills
    .filter(
      (skill) =>
        !needle ||
        skill.name.toLowerCase().includes(needle) ||
        skill.description.toLowerCase().includes(needle),
    )
    .map((skill) => ({
      id: `skill:${skill.path}`,
      kind: "skill" as const,
      label: `/${skill.name}`,
      description: skill.description,
      source: skill.source,
      insert: `/${skill.name} `,
    }));

  return [...commandOptions, ...skillOptions].slice(0, SUGGESTION_LIMIT);
}

async function fileOptions(
  context: SuggestionContext,
  query: string,
): Promise<SuggestionOption[]> {
  if (!context.threadId) return [];
  try {
    const response = await context.queryClient.fetchQuery({
      queryKey: ["file-search", context.threadId, query],
      queryFn: () => searchFiles(context.threadId as string, query),
      staleTime: 5_000,
    });
    return response.files
      .map((file) => ({
        id: `file:${file}`,
        kind: "file" as const,
        label: file,
        description: "Add a file reference",
        insert: `@${file} `,
      }))
      .slice(0, SUGGESTION_LIMIT);
  } catch {
    return [];
  }
}

function menuProps(
  kind: "command" | "file",
  props: SuggestionProps<SuggestionOption>,
): SuggestionMenuProps {
  return {
    kind,
    query: props.query,
    options: props.items,
    loading: props.loading,
    onChoose: (option) => props.command(option),
  };
}

function suggestionRenderer(
  kind: "command" | "file",
  context: SuggestionContext,
) {
  return () => {
    let component: ReactRenderer<SuggestionMenuHandle, SuggestionMenuProps> | null =
      null;
    let unmount: (() => void) | null = null;

    return {
      onStart: (props: SuggestionProps<SuggestionOption>) => {
        component = new ReactRenderer(SuggestionMenu, {
          props: menuProps(kind, props),
          editor: props.editor,
          className: "composer-suggest-layer",
        });
        unmount = props.mount(component.element);
        context.onActiveChange(true);
      },
      onUpdate: (props: SuggestionProps<SuggestionOption>) => {
        component?.updateProps(menuProps(kind, props));
      },
      onKeyDown: ({ event }: SuggestionKeyDownProps) =>
        component?.ref?.onKeyDown(event) ?? false,
      onExit: () => {
        unmount?.();
        unmount = null;
        component?.destroy();
        component = null;
        context.onActiveChange(false);
      },
    };
  };
}

function composerSuggestions(context: SuggestionContext) {
  return Extension.create({
    name: "composerSuggestions",
    addProseMirrorPlugins() {
      return [
        Suggestion<SuggestionOption>({
          editor: this.editor,
          pluginKey: COMMAND_PLUGIN_KEY,
          char: "/",
          allowedPrefixes: [" "],
          decorationClass: "composer-suggestion",
          floatingUi: { middleware: [shift({ padding: 8 })] },
          items: ({ query }) => commandOptions(context, query),
          command: ({ editor, range, props }) => {
            editor
              .chain()
              .focus()
              .deleteRange(range)
              .insertContent(props.insert)
              .run();
          },
          render: suggestionRenderer("command", context),
        }),
        Suggestion<SuggestionOption>({
          editor: this.editor,
          pluginKey: FILE_PLUGIN_KEY,
          char: "@",
          allowedPrefixes: [" "],
          debounce: 160,
          decorationClass: "composer-suggestion",
          floatingUi: { middleware: [shift({ padding: 8 })] },
          items: ({ query }) => fileOptions(context, query),
          command: ({ editor, range, props }) => {
            editor
              .chain()
              .focus()
              .deleteRange(range)
              .insertContent(props.insert)
              .run();
          },
          render: suggestionRenderer("file", context),
        }),
      ];
    },
  });
}

/** Hide any open command/file menu before the composer clears its draft. */
export function dismissComposerSuggestions(editor: Editor | null): void {
  if (!editor) return;
  exitSuggestion(editor.view, COMMAND_PLUGIN_KEY);
  exitSuggestion(editor.view, FILE_PLUGIN_KEY);
}

interface UseComposerSuggestionsOptions {
  threadId?: string;
  projectId?: string;
  agent: string;
}

/** Provides the Tiptap suggestion extension and the toolbar trigger buttons. */
export function useComposerSuggestions({
  threadId,
  projectId,
  agent,
}: UseComposerSuggestionsOptions) {
  const queryClient = useQueryClient();
  const [active, setActive] = useState(false);
  const contextRef = useRef<SuggestionContext>({
    agent,
    projectId,
    threadId,
    queryClient,
    onActiveChange: setActive,
  });
  contextRef.current.agent = agent;
  contextRef.current.projectId = projectId;
  contextRef.current.threadId = threadId;
  contextRef.current.queryClient = queryClient;
  contextRef.current.onActiveChange = setActive;
  const extension = useMemo(
    () => composerSuggestions(contextRef.current),
    [],
  );

  const openTrigger = (kind: "command" | "file", editor: Editor | null) => {
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

  return { extension, openTrigger, active };
}
