import { ActionIcon, Avatar, Text } from "@mantine/core";
import {
  IconAlertCircle,
  IconArrowBackUp,
  IconClock,
  IconFileText,
  IconMessagePlus,
  IconPencil,
} from "@tabler/icons-react";
import ReactMarkdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { memo, useMemo, useState } from "react";
import {
  attachmentName,
  attachmentSize,
  isPreviewableImage,
} from "../../lib/attachments";
import type { ViewItem } from "../../lib/thread-events";
import { useLongPress } from "../../lib/use-long-press";
import { initials, timeOf } from "./format";
import { ImageLightbox } from "./ImageLightbox";

const REMARK_PLUGINS = [remarkGfm, remarkBreaks];
const REHYPE_PLUGINS = [rehypeHighlight];

export const Message = memo(function Message({
  item,
  onComment,
  onEdit,
  onRewind,
  onLongPress,
  active,
  continued = false,
}: {
  item: Extract<ViewItem, { kind: "message" }>;
  onComment: (messageId: string) => void;
  onEdit?: (item: Extract<ViewItem, { kind: "message" }>) => void;
  /** Undo this prompt's turn and every later one, files included. */
  onRewind?: (item: Extract<ViewItem, { kind: "message" }>) => void;
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
  const imageAttachments = item.attachments.filter((attachment) =>
    isPreviewableImage(attachment.mimeType),
  );
  const fileAttachments = item.attachments.filter(
    (attachment) => !isPreviewableImage(attachment.mimeType),
  );
  // Markdown + syntax highlighting is the expensive part of an agent
  // message, and a streaming turn re-renders every token batch. Keep the
  // streamed text plain and parse markdown once, when the message completes.
  // The final render is the only one that pays the full pipeline.
  const markdown = useMemo(
    () =>
      item.streaming ? null : (
        <ReactMarkdown
          remarkPlugins={REMARK_PLUGINS}
          rehypePlugins={REHYPE_PLUGINS}
        >
          {text}
        </ReactMarkdown>
      ),
    [item.streaming, text],
  );

  // A reader long-presses the words, not the avatar. Agent output keeps native
  // text selection; a prompt gets the whole-bubble action sheet so Edit is
  // reachable on a phone.
  return (
    <div
      className={`block message-block ${
        agent ? "message-agent" : "message-user"
      }${continued ? " message-continued" : ""}`}
      {...(!agent ? press : {})}
      onContextMenu={
        agent ? undefined : (event) => event.preventDefault()
      }
    >
      <div className="block-gutter" {...(agent ? press : {})}>
        {continued ? null : (
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
        )}
        {/* Every row is stamped, including the ones that drop the repeated
            avatar: when a message landed is the one thing a reader scans for. */}
        <time
          className="msg-time"
          dateTime={new Date(item.ts).toISOString()}
          title={new Date(item.ts).toLocaleString()}
        >
          {timeOf(item.ts)}
        </time>
      </div>
      <div className="block-body">
        {!agent && onEdit ? (
          <ActionIcon
            className="msg-edit-btn"
            size={18}
            aria-label="Edit message"
            onClick={() => onEdit(item)}
          >
            <IconPencil size={12} stroke={1.5} />
          </ActionIcon>
        ) : null}
        {!agent && onRewind ? (
          <ActionIcon
            className="msg-rewind-btn"
            size={18}
            aria-label="Rewind to before this turn"
            onClick={() => onRewind(item)}
          >
            <IconArrowBackUp size={12} stroke={1.5} />
          </ActionIcon>
        ) : null}
        <ActionIcon
          className="msg-comment-btn"
          size={18}
          aria-label="Comment on message"
          onClick={() => onComment(item.id)}
        >
          <IconMessagePlus size={12} stroke={1.5} />
        </ActionIcon>
        {imageAttachments.length ? (
          <div className="message-images">
            {imageAttachments.map((attachment) => {
              const url = `/api/attachments/${attachment.id}`;
              const name = attachmentName(attachment);
              return (
                <button
                  key={attachment.id}
                  type="button"
                  className="message-image"
                  onClick={() => setLightbox(url)}
                  aria-label={`Preview ${name}`}
                  title={name}
                >
                  <img src={url} alt={name} loading="lazy" />
                </button>
              );
            })}
          </div>
        ) : null}
        {fileAttachments.length ? (
          <div className="message-files">
            {fileAttachments.map((attachment) => {
              const name = attachmentName(attachment);
              return (
                <a
                  key={attachment.id}
                  className="message-file"
                  href={`/api/attachments/${attachment.id}`}
                  download={name}
                  title={`Download ${name}`}
                >
                  <IconFileText size={14} stroke={1.5} />
                  <span className="message-file-name">{name}</span>
                  <span className="message-file-size">
                    {attachmentSize(attachment.byteSize)}
                  </span>
                </a>
              );
            })}
          </div>
        ) : null}
        <div className={streaming ? "prose streaming" : "prose"}>
          {agent ? (
            item.streaming ? (
              <p className="prose-stream">{text}</p>
            ) : (
              markdown
            )
          ) : (
            <p>{text}</p>
          )}
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
