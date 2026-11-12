import { ActionIcon, Avatar, Text } from "@mantine/core";
import {
  IconAlertCircle,
  IconClock,
  IconMessagePlus,
} from "@tabler/icons-react";
import ReactMarkdown from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { memo } from "react";
import type { ViewItem } from "../../lib/thread-events";
import { useLongPress } from "../../lib/use-long-press";
import { initials, timeOf } from "./format";

const REMARK_PLUGINS = [remarkGfm, remarkBreaks];

export const Message = memo(function Message({
  item,
  onComment,
  onLongPress,
  active,
}: {
  item: Extract<ViewItem, { kind: "message" }>;
  onComment: (messageId: string) => void;
  onLongPress: (item: Extract<ViewItem, { kind: "message" }>) => void;
  active: boolean;
}) {
  const agent = item.role === "agent";
  const press = useLongPress(() => onLongPress(item));
  // Waiting for the first chunk owns no text yet: a caret then would hang
  // under the previous block instead of tracking the agent's output.
  const streaming = item.streaming && active && item.text.trim().length > 0;

  return (
    <div
      className={`block message-block ${agent ? "message-agent" : "message-user"}`}
    >
      <div className="block-gutter" {...press}>
        <Avatar
          size={16}
          radius={agent ? 4 : "xl"}
          color="gray"
          variant="light"
          src={item.avatarUrl ?? undefined}
          title={`${item.name} · ${timeOf(item.ts)}`}
          className={agent ? "avatar-agent" : "avatar-user"}
        >
          {agent ? "AI" : initials(item.name)}
        </Avatar>
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
                <a
                  key={attachment.id}
                  className="message-image"
                  href={url}
                  target="_blank"
                  rel="noreferrer"
                >
                  <img src={url} alt="" loading="lazy" />
                </a>
              );
            })}
          </div>
        ) : null}
        <div className={streaming ? "prose streaming" : "prose"}>
          {agent ? (
            <ReactMarkdown remarkPlugins={REMARK_PLUGINS}>
              {item.text}
            </ReactMarkdown>
          ) : (
            <p>{item.text}</p>
          )}
        </div>
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
