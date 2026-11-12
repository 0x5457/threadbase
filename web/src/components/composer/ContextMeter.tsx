import { Group, Text, Tooltip } from "@mantine/core";
import type { Usage } from "../../api/types/Usage";
import { formatTokens } from "../thread/format";

/**
 * Context-window gauge driven by the agent's `usage_updated` event. The bar
 * only appears when the adapter reports a context size; otherwise the total
 * token count stands in.
 */
export function ContextMeter({ usage }: { usage: Usage | null }) {
  if (!usage) return null;
  const used = usage.contextUsed;
  const size = usage.contextSize;
  const percent =
    used !== null && size !== null && size > 0
      ? Math.min(100, Math.round((used / size) * 100))
      : null;
  const label =
    percent !== null
      ? `ctx ${percent}%`
      : `${formatTokens(usage.totalTokens)} tok`;
  const detail = [
    percent !== null
      ? `Context ${formatTokens(used ?? 0)} / ${formatTokens(size ?? 0)} tokens`
      : `Context used ${formatTokens(used ?? usage.totalTokens)} tokens`,
    `in ${formatTokens(usage.inputTokens)}`,
    `out ${formatTokens(usage.outputTokens)}`,
    `total ${formatTokens(usage.totalTokens)}`,
  ].join(" · ");

  return (
    <Tooltip label={detail}>
      <Group
        gap={5}
        wrap="nowrap"
        className={`context-meter${percent !== null && percent >= 85 ? " hot" : ""}`}
        visibleFrom="sm"
      >
        {percent !== null ? (
          <span className="context-meter-bar" aria-hidden="true">
            <span style={{ width: `${percent}%` }} />
          </span>
        ) : null}
        <Text size="xs" c="dimmed">
          {label}
        </Text>
      </Group>
    </Tooltip>
  );
}
