/// Shared formatting for timeline rows.

import dayjs from "dayjs";

export { initials } from "../../lib/initials";

export function describeTitle(title: string) {
  const flat = title.replace(/\s+/g, " ").trim();
  return flat.length > 96 ? `${flat.slice(0, 96)}…` : flat;
}

export function timeOf(ts: number) {
  return dayjs(ts).format("hh:mm A");
}
