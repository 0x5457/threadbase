import { format, formatDistanceToNowStrict } from "date-fns";

export function toDate(value: number | bigint | null | undefined): Date | null {
  if (value === null || value === undefined) {
    return null;
  }
  const milliseconds = typeof value === "bigint" ? Number(value) : value;
  if (!Number.isFinite(milliseconds) || milliseconds <= 0) {
    return null;
  }
  return new Date(milliseconds);
}

export function formatRelativeTime(
  value: number | bigint | null | undefined,
): string {
  const date = toDate(value);
  if (!date) {
    return "never";
  }
  return formatDistanceToNowStrict(date, { addSuffix: true });
}

export function formatTimestamp(
  value: number | bigint | null | undefined,
): string {
  const date = toDate(value);
  if (!date) {
    return "—";
  }
  return format(date, "MMM d, HH:mm");
}

export function formatBytes(value: number | bigint | null | undefined): string {
  if (value === null || value === undefined) {
    return "—";
  }
  const bytes = Number(value);
  if (!Number.isFinite(bytes) || bytes < 0) {
    return "—";
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function formatTokenCount(value: number | bigint | null | undefined): string {
  if (value === null || value === undefined) {
    return "—";
  }
  return new Intl.NumberFormat("en-US").format(Number(value));
}

export function basename(path: string | null | undefined): string {
  if (!path) {
    return "";
  }
  const normalized = path.replace(/[\\/]+$/, "");
  const parts = normalized.split(/[\\/]/);
  return parts[parts.length - 1] || normalized;
}

export function initials(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) {
    return "?";
  }
  if (words.length === 1) {
    return words[0].slice(0, 2).toUpperCase();
  }
  return `${words[0][0] ?? ""}${words[words.length - 1][0] ?? ""}`.toUpperCase();
}

export function normalizeExternalUrl(url: string): string | null {
  const trimmed = url.trim();
  if (!trimmed) {
    return null;
  }
  try {
    const parsed = new URL(trimmed);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      return null;
    }
    return parsed.toString();
  } catch {
    return null;
  }
}
