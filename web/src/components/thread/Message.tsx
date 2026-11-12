import { ActionIcon, Avatar, Text } from "@mantine/core";
import {
  IconAlertCircle,
  IconClock,
  IconMessagePlus,
} from "@tabler/icons-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { memo } from "react";
import type { ViewItem } from "../../lib/thread-events";
import { initials, timeOf } from "./format";

const REMARK_PLUGINS = [remarkGfm];

export const Message = memo(function Message({
  item,
  onComment,
  active,
}: {
  item: Extract<ViewItem, { kind: "message" }>;
  onComment: (messageId: string) => void;
  active: boolean;
}) {
  const agent = item.role === "agent";
  return (
    <div className="block message-block">
      <div className="block-gutter">
        <Avatar
          size={18}
          radius="xl"
          color={agent ? "gray" : "brand"}
          variant={agent ? "light" : "filled"}
          title={`${item.name} · ${timeOf(item.ts)}`}
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
        <div className="prose">
          {agent ? (
            <ReactMarkdown remarkPlugins={REMARK_PLUGINS}>
              {item.text}
            </ReactMarkdown>
          ) : (
            <p>{item.text}</p>
          )}
          {item.streaming && active ? <span className="stream-caret" /> : null}
        </div>
      </div>
    </div>
  );
});

export const Thinking = memo(function Thinking() {
  return (
    <div className="block">
      <div className="block-gutter" />
      <div className="block-body">
        <Text size="sm" c="dimmed" className="thinking-row">
          <IconClock size={11} stroke={1.5} /> Thought for a moment
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
