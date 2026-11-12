import { ActionIcon, CopyButton, Tooltip } from "@mantine/core";
import { IconCheck, IconChevronDown, IconCopy } from "@tabler/icons-react";
import { memo } from "react";
import type { DiagnosticLine } from "../lib/thread-events";

/**
 * Collapsible diagnostics surface for adapter stderr. Collapsed by default so
 * install noise stays out of the conversation; the copy button copies the
 * retained window as plain text.
 */
export const AgentLog = memo(function AgentLog({
  lines,
}: {
  lines: DiagnosticLine[];
}) {
  if (lines.length === 0) return null;
  return (
    <details className="agent-log">
      <summary className="agent-log-head">
        <span className="agent-log-title">Agent log · {lines.length} lines</span>
        <IconChevronDown
          className="agent-log-chevron"
          size={11}
          stroke={1.5}
        />
        <CopyButton value={lines.map((line) => line.text).join("\n")}>
          {({ copied, copy }) => (
            <Tooltip label={copied ? "Copied" : "Copy log"}>
              <ActionIcon
                className="agent-log-copy"
                size={18}
                aria-label="Copy agent log"
                onClick={(event) => {
                  // The copy control lives inside <summary>: keep the click
                  // from toggling the disclosure while it copies.
                  event.preventDefault();
                  event.stopPropagation();
                  copy();
                }}
              >
                {copied ? (
                  <IconCheck size={12} stroke={1.5} />
                ) : (
                  <IconCopy size={12} stroke={1.5} />
                )}
              </ActionIcon>
            </Tooltip>
          )}
        </CopyButton>
      </summary>
      <div className="agent-log-body">
        {lines.map((line) => (
          <div
            key={line.id}
            className={
              line.level === "error" ? "agent-log-line error" : "agent-log-line"
            }
          >
            {line.text}
          </div>
        ))}
      </div>
    </details>
  );
});
