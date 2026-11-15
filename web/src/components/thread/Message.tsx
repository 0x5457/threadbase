import { ActionIcon, Avatar, Text } from "@mantine/core";
import {
  IconAlertCircle,
  IconClock,
  IconMessagePlus,
} from "@tabler/icons-react";
import ReactMarkdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import type { ViewItem } from "../../lib/thread-events";
import { useLongPress } from "../../lib/use-long-press";
import { initials, timeOf } from "./format";
import { ImageLightbox } from "./ImageLightbox";

const REMARK_PLUGINS = [remarkGfm, remarkBreaks];
const REHYPE_PLUGINS = [rehypeHighlight];

/**
 * Streaming deltas arrive in 50 ms batches. Running the full markdown +
 * syntax-highlight pipeline on every batch costs ~15 ms for a 5 KB message
 * (measured here; several times worse on a phone), so a long answer pegs the
 * main thread while it types. Let the rendered text trail the wire by at most
 * this interval instead; the final text is rendered immediately once the
 * message stops streaming.
 */
const STREAM_RENDER_MS = 250;

function useStreamedText(text: string, streaming: boolean) {
  const [rendered, setRendered] = useState(text);
  const lastAt = useRef(0);

  useEffect(() => {
    if (!streaming) {
      setRendered(text);
      return;
    }
    const elapsed = performance.now() - lastAt.current;
    if (elapsed >= STREAM_RENDER_MS) {
      lastAt.current = performance.now();
      setRendered(text);
      return;
    }
    // Trailing throttle: keep only the latest chunk, not every one in between.
    const timer = window.setTimeout(() => {
      lastAt.current = performance.now();
      setRendered(text);
    }, STREAM_RENDER_MS - elapsed);
    return () => window.clearTimeout(timer);
  }, [text, streaming]);

  return rendered;
}

export const Message = memo(function Message({
  item,
  onComment,
  onLongPress,
  active,
  continued = false,
}: {
  item: Extract<ViewItem, { kind: "message" }>;
  onComment: (messageId: string) => void;
  onLongPress: (item: Extract<ViewItem, { kind: "message" }>) => void;
  active: boolean;
  /** Same author as the row above: drop the repeated avatar and tighten up. */
  continued?: boolean;
}) {
  const agent = item.role === "agent";
  const press = useLongPress(() => onLongPress(item));
  // Waiting for the first chunk owns no text yet: a caret then would hang
  // under the previous block instead of tracking the agent's output.
  const streaming = item.streaming && active && item.text.trim().length > 0;
  // A trailing blank line from the composer would otherwise paint as a second
  // line inside the bubble; the prompt itself is already trimmed server-side.
  const text = agent ? item.text : item.text.replace(/^\n+|\n+$/g, "");
  const [lightbox, setLightbox] = useState<string | null>(null);
  // Markdown parsing and highlighting are the expensive part of a streaming
  // update, so they run on a throttled copy. Highlighting partial code fences
  // is wasted work too: only completed messages get rehype-highlight.
  const liveText = useStreamedText(text, item.streaming && agent);
  const streamingText = item.streaming && agent ? liveText : text;
  // ReactMarkdown re-runs its parse on every render, even when the text is
  // identical. Keep the element so a parent render between throttled chunks
  // (or a bundle poll) does not re-parse the whole message.
  const markdown = useMemo(
    () => (
      <ReactMarkdown
        remarkPlugins={REMARK_PLUGINS}
        rehypePlugins={item.streaming ? undefined : REHYPE_PLUGINS}
      >
        {streamingText}
      </ReactMarkdown>
    ),
    [streamingText, item.streaming],
  );

  return (
    <div
      className={`block message-block ${
        agent ? "message-agent" : "message-user"
      }${continued ? " message-continued" : ""}`}
    >
      <div className="block-gutter" {...press}>
        {continued ? null : (
          <>
            <Avatar
              size={32}
              radius={agent ? 4 : "xl"}
              color="gray"
              variant="light"
              src={item.avatarUrl ?? undefined}
              title={`${item.name} · ${timeOf(item.ts)}`}
              className={agent ? "avatar-agent" : "avatar-user"}
            >
              {agent ? "AI" : initials(item.name)}
            </Avatar>
            <span className="msg-time" title={new Date(item.ts).toLocaleString()}>
              {timeOf(item.ts)}
            </span>
          </>
        )}
      </div>
      <div className="block-body">
        <ActionIcon
          className="msg-comment-btn"
          size={18}
          aria-label="Comment on message"
          onClick={() => onComment(item.id)}
        >
          <IconMessagePlus size={12} stroke={1.5} />
        </ActionIcon>
        {item.attachments.length ? (
          <div className="message-images">
            {item.attachments.map((attachment) => {
              const url = `/api/attachments/${attachment.id}`;
              return (
                <button
                  key={attachment.id}
                  type="button"
                  className="message-image"
                  onClick={() => setLightbox(url)}
                  aria-label={`Preview image attachment (${attachment.mimeType})`}
                >
                  <img src={url} alt="" loading="lazy" />
                </button>
              );
            })}
          </div>
        ) : null}
        <div className={streaming ? "prose streaming" : "prose"}>
          {agent ? markdown : <p>{text}</p>}
        </div>
        {lightbox ? (
          <ImageLightbox src={lightbox} onClose={() => setLightbox(null)} />
        ) : null}
      </div>
    </div>
  );
});

export const Thinking = memo(function Thinking({
  count = 1,
  tools = 0,
  text = "",
}: {
  count?: number;
  tools?: number;
  text?: string;
}) {
  const thought = count > 1 ? `Thought ${count} times` : "Thought for a moment";
  const ran =
    tools > 0 ? ` · ran ${tools} ${tools === 1 ? "tool" : "tools"}` : "";
  return (
    <div className="block">
      <div className="block-gutter" />
      <div className="block-body">
        <Text
          size="sm"
          c="dimmed"
          className="thinking-row"
          title={text.trim().slice(0, 240) || undefined}
        >
          <IconClock size={11} stroke={1.5} /> {thought}
          {ran}
        </Text>
      </div>
    </div>
  );
});

export const Notice = memo(function Notice({
  item,
}: {
  item: Extract<ViewItem, { kind: "notice" }>;
}) {
  return (
    <div className="block">
      <div className="block-gutter" />
      <div className="block-body">
        <Text
          size="sm"
          c={item.level === "error" ? "red" : "dimmed"}
          className="notice-row"
        >
          {item.level === "error" ? (
            <IconAlertCircle size={12} stroke={1.5} />
          ) : null}
          {item.text}
        </Text>
      </div>
    </div>
  );
});
