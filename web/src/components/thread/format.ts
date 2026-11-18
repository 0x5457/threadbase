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

const day = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });

/** Sidebar meta: pass, not timestamps. Coarse on purpose — a row that says
 *  "2h" is not read, it is scanned. */
export function relativeTime(ts: number): string {
  const delta = Date.now() - ts;
  if (!Number.isFinite(delta) || delta < 60_000) return "now";
  const minutes = Math.floor(delta / 60_000);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  return day.format(new Date(ts));
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
