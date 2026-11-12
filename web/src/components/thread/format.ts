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
