import {
  ActionIcon,
  Avatar,
  Box,
  Group,
  Kbd,
  Menu,
  Text,
  Tooltip,
} from "@mantine/core";
import { useDebouncedValue } from "@mantine/hooks";
import { notifications } from "@mantine/notifications";
import {
  IconAdjustmentsHorizontal,
  IconArrowUp,
  IconAt,
  IconCheck,
  IconCommand,
  IconEye,
  IconMessagePlus,
} from "@tabler/icons-react";
import Placeholder from "@tiptap/extension-placeholder";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  getAgentConfig,
  getMe,
  listSkills,
  searchFiles,
  sendDraft,
  sendMessage,
} from "../api";
import { useDraftSession, userColor as colorForUser } from "../lib/draft";
import { ModelPicker } from "./ModelPicker";
import { PendingBlocks } from "./composer/PendingBlocks";
import { SuggestionMenu } from "./composer/SuggestionMenu";
import {
  commandsFor,
  detectSuggestion,
  type Suggestion,
  type SuggestionOption,
} from "./composer/commands";

function initials(name: string) {
  return name
    .split(/\s+/)
    .map((part) => part[0])
    .slice(0, 2)
    .join("")
    .toUpperCase();
}

interface ComposerProps {
  threadId?: string;
  projectId?: string;
  agentName?: string;
  modelName?: string | null;
  effort?: string | null;
  onModelChange?: (model: string) => void;
  onEffortChange?: (effort: string) => void;
  onSent?: () => void;
  onThreadComment?: () => void;
  /** Signed-in author; defaults to the cached `GET /api/auth/me` session. */
  userId?: string;
  userName?: string;
  userColor?: string;
  /** Viewers see the shared draft but cannot write or send. */
  canWrite?: boolean;
  /** Live presence count for this thread. */
  watching?: number;
  /** When false, plain Enter sends and Shift+Enter inserts a newline. */
  sendWithModifier?: boolean;
}

export function Composer({
  threadId,
  projectId,
  agentName,
  modelName,
  effort,
  onModelChange,
  onEffortChange,
  onSent,
  onThreadComment,
  watching,
  sendWithModifier = true,
  userId,
  userName,
  userColor,
  canWrite,
}: ComposerProps) {
  const [hasText, setHasText] = useState(false);
  const [sending, setSending] = useState(false);
  const [delivery, setDelivery] = useState<"auto" | "queue">("auto");
  const [suggestion, setSuggestion] = useState<Suggestion | null>(null);
  const [active, setActive] = useState(0);
  const agent = agentName ?? "agent";
  const modelLabel = [agent, modelName, effort].filter(Boolean).join(" · ");
  const writable = canWrite !== false;

  // AuthGate already fetched this; the query is a cache read for the composer.
  const me = useQuery({
    queryKey: ["me"],
    queryFn: getMe,
    staleTime: 60_000,
  });
  const currentUserId = userId ?? me.data?.user.id ?? null;
  const currentUserAvatar = me.data?.user.avatarUrl ?? null;
  const currentUserName =
    userName ?? me.data?.user.displayName ?? me.data?.user.username ?? null;
  const draft = useDraftSession(
    threadId && currentUserId ? threadId : null,
    currentUserId && currentUserName
      ? { id: currentUserId, name: currentUserName }
      : null,
  );
  const {
    setMyText: mirrorDraft,
    markSent: markDraftSent,
    setEditing: setDraftEditing,
  } = draft;
  const restoredThread = useRef<string | null>(null);

  const skills = useQuery({
    queryKey: ["skills", projectId],
    queryFn: () => listSkills(projectId as string),
    enabled: Boolean(projectId),
    staleTime: 60_000,
    retry: false,
  });

  // The running adapter is the source of truth for model choices; agents that
  // do not advertise them still accept a custom id in the picker.
  const agentConfig = useQuery({
    queryKey: ["agent-config", threadId],
    queryFn: () => getAgentConfig(threadId as string),
    enabled: Boolean(threadId) && Boolean(onModelChange),
    staleTime: 30_000,
    retry: false,
  });
  const modelOption = agentConfig.data?.options.find(
    (option) => option.category === "model",
  );
  const effortOption = agentConfig.data?.options.find(
    (option) => option.category === "thought_level",
  );
  const effectiveModel = modelName ?? modelOption?.currentValue ?? null;
  const effectiveEffort = effort ?? effortOption?.currentValue ?? null;

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

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: false,
        blockquote: false,
        codeBlock: false,
        horizontalRule: false,
        bulletList: false,
        orderedList: false,
        listItem: false,
      }),
      Placeholder.configure({
        placeholder: `Message ${agent}…`,
      }),
    ],
    editorProps: {
      attributes: {
        class: "composer-editor",
        "aria-label": "Message",
      },
    },
    // Viewers keep a read-only editor; `editable` is re-applied below because
    // the Tiptap React binding pins it to the instance's current value.
    editable: writable,
    onUpdate: ({ editor: current }) => {
      setHasText(current.getText().trim().length > 0);
      setSuggestion(detectSuggestion(current));
      // IME composition is reconciled on compositionend instead.
      if (!current.view.composing) mirrorDraft(current.getText());
    },
    onSelectionUpdate: ({ editor: current }) => {
      if (suggestion) setSuggestion(detectSuggestion(current));
    },
    onFocus: () => setDraftEditing(true),
    onBlur: ({ editor: current }) => {
      mirrorDraft(current.getText());
      setDraftEditing(false);
    },
  });

  useEffect(() => {
    editor?.setEditable(writable);
  }, [editor, writable]);

  // Reloading a thread keeps the author's pending text in the CRDT. Restore it
  // once so the draft stays editable — peers' text never enters this editor.
  useEffect(() => {
    if (!editor || !threadId) return;
    if (draft.restoredText === null) return;
    if (restoredThread.current === threadId) return;
    restoredThread.current = threadId;
    if (editor.getText().length > 0 || draft.restoredText.length === 0) return;
    editor.commands.setContent(draft.restoredText, { emitUpdate: false });
    setHasText(draft.restoredText.trim().length > 0);
  }, [draft.restoredText, editor, threadId]);

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
  }, [suggestion?.kind, suggestion?.query]);

  useEffect(() => {
    if (active >= options.length) {
      setActive(Math.max(0, options.length - 1));
    }
  }, [active, options.length]);

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
    [editor, suggestion],
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
  }, [active, choose, options, suggestion]);

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

  const send = useCallback(async () => {
    if (!editor || sending || !writable) return;
    if (editor.getText().trim().length === 0) return;
    const shared = Boolean(threadId) && draft.active;
    if (shared && !draft.synced) {
      notifications.show({
        message: "Draft is still syncing with the thread",
        color: "yellow",
      });
      return;
    }
    setSuggestion(null);
    const text = editor.getText().trim();

    if (!threadId) {
      editor.commands.clearContent();
      setHasText(false);
      notifications.show({ message: `Sent to ${agent}`, color: "teal" });
      return;
    }

    setSending(true);
    try {
      if (shared) {
        // The server reads the pending CRDT blocks, so the editor must stay
        // populated until the POST returns. Its tombstone clears the draft.
        const result = await sendDraft(threadId, delivery);
        // A successful send is visible in the timeline; only queueing, which
        // changes when the agent sees the message, is worth announcing.
        if (result.delivery === "queued") {
          notifications.show({ message: "Queued for the next turn", color: "teal" });
        }
        // Allocate the next draft block before the editor clears, so the
        // cleared text does not allocate a second empty block.
        markDraftSent();
        editor.commands.clearContent();
        setHasText(false);
      } else {
        editor.commands.clearContent();
        setHasText(false);
        const result = await sendMessage(threadId, text, delivery);
        if (result.delivery === "queued") {
          notifications.show({ message: "Queued for the next turn", color: "teal" });
        }
      }
      onSent?.();
    } catch (cause) {
      editor.commands.setContent(text);
      setHasText(true);
      if (shared) mirrorDraft(text);
      notifications.show({
        message:
          cause instanceof Error ? cause.message : "Could not send message",
        color: "red",
      });
    } finally {
      setSending(false);
    }
  }, [
    agent,
    delivery,
    draft.active,
    draft.synced,
    editor,
    markDraftSent,
    mirrorDraft,
    onSent,
    sending,
    threadId,
    writable,
  ]);

  const authorName = (authorId: string) =>
    draft.remoteEditors.find((peer) => peer.id === authorId)?.name ?? "Teammate";
  const authorAccent = (authorId: string) =>
    draft.remoteEditors.find((peer) => peer.id === authorId)?.color ??
    colorForUser(authorId);
  const editingNames = Array.from(
    new Set(draft.remoteEditors.map((peer) => peer.name)),
  );
  const liveLabel = draft.active
    ? `${draft.synced && draft.connected ? "live" : draft.connected ? "syncing" : "offline"}${editingNames.length ? ` · ${editingNames.join(", ")} editing` : ""}`
    : "";
  const sendBlocked = draft.active && !draft.synced;
  const sendLabel = !writable
    ? "Viewers cannot send"
    : sendBlocked
      ? "Waiting for the shared draft to sync"
      : "Send  ⌘↵";

  const loading =
    suggestion?.kind === "file" ? files.isFetching : skills.isFetching;

  return (
    <section
      className="composer"
      onKeyDownCapture={(event) => {
        if (
          !sendWithModifier &&
          event.key === "Enter" &&
          !event.shiftKey &&
          !event.metaKey &&
          !event.ctrlKey &&
          !event.nativeEvent.isComposing &&
          !suggestion
        ) {
          event.preventDefault();
          event.stopPropagation();
          void send();
        }
      }}
      onKeyDown={(event) => {
        if (event.nativeEvent.isComposing) return;
        if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
          event.preventDefault();
          void send();
        }
      }}
      onCompositionEnd={() => {
        if (editor) mirrorDraft(editor.getText());
      }}
    >
      <div className="composer-inner">
        {suggestion ? (
          <SuggestionMenu
            suggestion={suggestion}
            options={options}
            active={active}
            loading={loading}
            onActive={setActive}
            onChoose={choose}
          />
        ) : null}
        <PendingBlocks
          blocks={draft.remoteBlocks}
          authorName={authorName}
          authorAccent={authorAccent}
        />
        <div className="composer-row">
          <Avatar
            size={18}
            radius="xl"
            color="brand"
            src={currentUserAvatar ?? undefined}
            styles={
              userColor ?? currentUserId
                ? {
                    root: {
                      backgroundColor:
                        userColor ??
                        (currentUserId ? colorForUser(currentUserId) : undefined),
                      color: "#fff",
                    },
                  }
                : undefined
            }
          >
            {initials(currentUserName ?? "You")}
          </Avatar>
          <div className="composer-input">
            <EditorContent editor={editor} />
            {!hasText ? (
              <span className="composer-placeholder" aria-hidden="true">
                {writable ? `Message ${agent}…` : "Read-only access to this thread"}
              </span>
            ) : null}
          </div>
        </div>
        <div className="composer-tools">
          {onThreadComment ? (
            <Tooltip label="Comment on thread">
              <ActionIcon
                aria-label="Comment on thread"
                size={22}
                onClick={onThreadComment}
              >
                <IconMessagePlus size={14} stroke={1.5} />
              </ActionIcon>
            </Tooltip>
          ) : null}
          <Tooltip label="Reference a file">
            <ActionIcon
              aria-label="Reference a file"
              size={22}
              onClick={() => openTrigger("file")}
            >
              <IconAt size={14} stroke={1.5} />
            </ActionIcon>
          </Tooltip>
          <Tooltip label="Slash commands and skills">
            <ActionIcon
              aria-label="Slash commands and skills"
              size={22}
              onClick={() => openTrigger("command")}
            >
              <IconCommand size={14} stroke={1.5} />
            </ActionIcon>
          </Tooltip>
          <div className="spacer" />
          <Kbd visibleFrom="sm" mr={4}>
            ⌘↵
          </Kbd>
          <Menu position="top-end" width={230} shadow="md">
            <Menu.Target>
              <ActionIcon
                variant="subtle"
                color="gray"
                size={22}
                aria-label="Effort and delivery"
              >
                <IconAdjustmentsHorizontal size={13} stroke={1.5} />
              </ActionIcon>
            </Menu.Target>
            <Menu.Dropdown>
              <Menu.Label>Agent</Menu.Label>
              <Menu.Item disabled>{agent}</Menu.Item>
              <Menu.Divider />
              <Menu.Label>Delivery</Menu.Label>
              <Menu.Item
                onClick={() => setDelivery("auto")}
                rightSection={
                  delivery === "auto" ? (
                    <IconCheck size={12} stroke={1.5} />
                  ) : null
                }
              >
                Send now
              </Menu.Item>
              <Menu.Item
                onClick={() => setDelivery("queue")}
                rightSection={
                  delivery === "queue" ? (
                    <IconCheck size={12} stroke={1.5} />
                  ) : null
                }
              >
                Queue for next turn
              </Menu.Item>
            </Menu.Dropdown>
          </Menu>
          {watching ? (
            <Tooltip label={`${watching} ${watching === 1 ? "person" : "people"} watching`}>
              <Group gap={3} className="watching-label" wrap="nowrap">
                <IconEye size={12} stroke={1.5} />
                <Text size="xs" c="dimmed">
                  {watching}
                </Text>
              </Group>
            </Tooltip>
          ) : null}
          {draft.active ? (
            <Tooltip label={liveLabel}>
              <span
                className={`live-dot ${
                  draft.synced && draft.connected
                    ? "online"
                    : draft.connected
                      ? "syncing"
                      : ""
                }`}
              />
            </Tooltip>
          ) : null}
          {onModelChange ? (
            <>
              <ModelPicker
                value={effectiveModel}
                options={modelOption?.options ?? []}
                loading={agentConfig.isFetching}
                disabled={!writable}
                onChange={onModelChange}
              />
              {onEffortChange ? (
                <ModelPicker
                  label="Effort"
                  placeholder="Default effort"
                  value={effectiveEffort}
                  options={effortOption?.options ?? []}
                  loading={agentConfig.isFetching}
                  disabled={!writable}
                  onChange={onEffortChange}
                />
              ) : null}
            </>
          ) : (
            <Text size="sm" c="dimmed" visibleFrom="sm" mr={4}>
              {modelLabel}
            </Text>
          )}
          <Tooltip label={sendLabel}>
            <Box component="span" style={{ display: "inline-flex" }}>
              <ActionIcon
                aria-label="Send"
                variant="filled"
                color="dark"
                size={24}
                disabled={!hasText || sending || sendBlocked || !writable}
                loading={sending}
                onClick={() => void send()}
                className="send-btn"
              >
                <IconArrowUp size={14} stroke={2} />
              </ActionIcon>
            </Box>
          </Tooltip>
        </div>
      </div>
    </section>
  );
}
