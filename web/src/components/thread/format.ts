/// Shared formatting for timeline rows.

export { initials } from "../../lib/initials";

export function describeTitle(title: string) {
  const flat = title.replace(/\s+/g, " ").trim();
  return flat.length > 96 ? `${flat.slice(0, 96)}…` : flat;
}

const clock = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
});

export function timeOf(ts: number) {
  return clock.format(new Date(ts));
}

/** Compact duration for tool rows and run summaries. */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "";
  if (ms < 1000) return `${Math.max(1, Math.round(ms))}ms`;
  if (ms < 60_000) {
    const seconds = ms / 1000;
    return `${seconds < 10 ? seconds.toFixed(1) : Math.round(seconds)}s`;
  }
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.round((ms % 60_000) / 1000);
  return `${minutes}m ${seconds}s`;
}

/** 12345 -> "12.3k", 1234567 -> "1.2M". */
export function formatTokens(value: number): string {
  if (!Number.isFinite(value) || value < 0) return "0";
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  if (value >= 1_000) return `${Math.round(value / 100) / 10}k`;
  return String(Math.round(value));
}
