import { Avatar, Button, Group, Text } from "@mantine/core";
import {
  IconAlertCircle,
  IconArrowLeft,
  IconChevronDown,
  IconGitBranch,
  IconInfoCircle,
} from "@tabler/icons-react";
import { useQuery } from "@tanstack/react-query";
import { memo, useMemo, useState } from "react";
import ReactMarkdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { getSubagentLog, type Subagent } from "../api";
import type { SubagentLogEntryDto } from "../api/types/SubagentLogEntryDto";
import { stateLabel } from "../lib/machine";
import { formatDuration, timeOf } from "./thread/format";
import { StatusDot } from "./thread/StatusDot";

const REMARK_PLUGINS = [remarkGfm, remarkBreaks];
const REHYPE_PLUGINS = [rehypeHighlight];

type Entry = SubagentLogEntryDto;

/** Tool calls are the noisy middle of a transcript; keep the run readable by
 *  folding consecutive results behind one summary until asked to expand. */
const ToolRun = memo(function ToolRun({ entries }: { entries: Entry[] }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="block subagent-tool-run">
      <div className="block-gutter" />
      <div className="block-body">
        <button
          type="button"
          className="subagent-tool-toggle"
          aria-expanded={open}
          onClick={() => setOpen((value) => !value)}
        >
          <IconChevronDown
            size={11}
            stroke={1.5}
            className={open ? "chevron open" : "chevron"}
          />
          {entries.length} tool {entries.length === 1 ? "call" : "calls"}
        </button>
        {open ? (
          <div className="subagent-tool-list">
            {entries.map((entry, index) => (
              <pre key={index} className="subagent-tool-output">
                {entry.text}
              </pre>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
});

/** The harness frames steering with a heading that would read as noise once
 *  the bubble already says where it came from. */
function parentText(text: string) {
  return text
    .replace(/^Mid-run steering from the parent orchestrator:\s*/i, "")
    .trim();
}

function message(entry: Entry, index: number) {
  const agent = entry.role === "agent";
  const parent = entry.role === "parent";
  if (entry.role === "notice") {
    return (
      <div key={`notice:${index}`} className="block">
        <div className="block-gutter" />
        <div className="block-body">
          <Text size="sm" c="dimmed" className="notice-row subagent-notice">
            <IconInfoCircle size={12} stroke={1.5} />
            {entry.text}
          </Text>
        </div>
      </div>
    );
  }
  if (entry.role === "error") {
    // The launch itself failed, so there is no conversation to show: keep the
    // adapter's reason in the same column instead of an empty transcript.
    return (
      <div key={`error:${index}`} className="block subagent-error">
        <div className="block-gutter" />
        <div className="block-body">
          <Group gap={6} wrap="nowrap" align="flex-start">
            <IconAlertCircle size={13} stroke={1.5} className="subagent-error-icon" />
            <Text size="sm" className="subagent-error-text">
              {entry.text}
            </Text>
          </Group>
        </div>
      </div>
    );
  }
  return (
    <div
      key={`${entry.role}:${index}`}
      className={`block message-block ${
        agent ? "message-agent" : `message-user${parent ? " subagent-parent" : ""}`
      }`}
    >
      <div className="block-gutter">
        <Avatar
          size={32}
          radius={agent ? 4 : "xl"}
          color="gray"
          variant="light"
          className={agent ? "avatar-agent" : "avatar-user"}
        >
          {agent ? "AI" : parent ? "P" : "T"}
        </Avatar>
        {entry.ts ? (
          <span className="msg-time" title={new Date(entry.ts).toLocaleString()}>
            {timeOf(entry.ts)}
          </span>
        ) : null}
      </div>
      <div className="block-body">
        {agent ? (
          <div className="prose">
            <ReactMarkdown remarkPlugins={REMARK_PLUGINS} rehypePlugins={REHYPE_PLUGINS}>
              {entry.text}
            </ReactMarkdown>
          </div>
        ) : (
          <>
            <span className="subagent-bubble-label">
              {parent ? "Parent" : "Task"}
            </span>
            <div className="prose subagent-task-text">
              <p>{parent ? parentText(entry.text) : entry.text.replace(/^Task:\s*/i, "")}</p>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

/**
 * A subagent's own conversation, opened like a thread from the sidebar tree.
 *
 * The parent thread owns the run and its input, so the view is read-only: no
 * composer, no queue. pi keeps every detached run's session on disk, which is
 * what the log endpoint replays here.
 */
export const SubagentThreadView = memo(function SubagentThreadView({
  threadId,
  subagent,
  parentTitle,
  onBack,
}: {
  threadId: string;
  subagent: Subagent;
  parentTitle: string;
  onBack: () => void;
}) {
  const id = String(subagent.id);
  const log = useQuery({
    queryKey: ["subagent-log", threadId, id],
    queryFn: () => getSubagentLog(threadId, id),
    refetchInterval: subagent.status === "running" ? 3_000 : false,
    staleTime: 2_000,
  });
  const entries = log.data?.entries ?? [];

  // Consecutive tool rows become one collapsible run; everything else keeps
  // its own row in the transcript.
  const rows = useMemo(() => {
    const out: Array<{ kind: "entry"; entry: Entry } | { kind: "tools"; entries: Entry[] }> =
      [];
    let tools: Entry[] = [];
    const flush = () => {
      if (tools.length) out.push({ kind: "tools", entries: tools });
      tools = [];
    };
    for (const entry of entries) {
      if (entry.role === "tool") {
        tools.push(entry);
        continue;
      }
      flush();
      out.push({ kind: "entry", entry });
    }
    flush();
    return out;
  }, [entries]);

  // Backfilled rows have identical created/updated stamps, so take the span
  // from the transcript itself instead of reporting "0ms".
  const firstTs = entries.find((entry) => entry.ts > 0)?.ts ?? 0;
  const lastTs = entries.reduce((last, entry) => (entry.ts > last ? entry.ts : last), 0);
  const duration = lastTs > firstTs ? formatDuration(lastTs - firstTs) : "";

  return (
    <>
      <div className="subagent-view">
        <div className="subagent-bar">
          <Button
            variant="subtle"
            size="compact-sm"
            color="gray"
            leftSection={<IconArrowLeft size={13} stroke={1.5} />}
            onClick={onBack}
            className="subagent-back"
            aria-label={`Back to ${parentTitle}`}
            title={parentTitle}
          >
            <span className="subagent-back-long">{parentTitle}</span>
            <span className="subagent-back-short">Back</span>
          </Button>
          <span className="subagent-bar-divider" />
          <IconGitBranch size={12} stroke={1.5} />
          <Text size="sm" fw={500} className="subagent-bar-title">
            {subagent.title}
          </Text>
          <StatusDot
            status={subagent.status}
            label={stateLabel("subagent", subagent.status)}
          />
          {duration ? (
            <Text size="xs" c="dimmed" className="subagent-meta">
              {duration}
            </Text>
          ) : null}
          {subagent.usage ? (
            <Text size="xs" c="dimmed" className="subagent-meta subagent-tokens">
              {subagent.usage.totalTokens.toLocaleString()} tokens
            </Text>
          ) : null}
        </div>
        <div className="thread-scroll-wrap subagent-scroll-wrap">
          <div className="thread-scroll subagent-scroll">
            <div className="doc timeline-doc subagent-doc">
              {log.isPending ? (
                <Text size="sm" c="dimmed" className="subagent-empty">
                  Loading the subagent's transcript…
                </Text>
              ) : log.error ? (
                <Group gap={6} className="subagent-empty" wrap="nowrap">
                  <IconAlertCircle size={13} stroke={1.5} />
                  <Text size="sm" c="red">
                    {log.error instanceof Error
                      ? log.error.message
                      : "Could not load this subagent's transcript"}
                  </Text>
                </Group>
              ) : rows.length === 0 ? (
                <Text size="sm" c="dimmed" className="subagent-empty">
                  No transcript is available for this subagent. The adapter did
                  not leave a session on disk, so only its status is known.
                </Text>
              ) : (
                rows.map((row, index) =>
                  row.kind === "tools" ? (
                    <ToolRun key={`tools:${index}`} entries={row.entries} />
                  ) : (
                    message(row.entry, index)
                  ),
                )
              )}
            </div>
          </div>
        </div>
      </div>
      <div className="composer-stack">
        <div className="subagent-readonly">
          <span className="subagent-readonly-long">
            This subagent runs under the parent thread — its conversation is
            read-only.
          </span>
          <span className="subagent-readonly-short">
            Read-only · replies go through the parent thread
          </span>
        </div>
      </div>
    </>
  );
});
