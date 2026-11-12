import type { ThreadEvent } from "../../api";
import type { EventStore } from "./types";


export function emptyStore(): EventStore {
  return { bySeq: new Map(), ordered: [], lastSeq: 0 };
}

function lastSeqOf(ordered: ThreadEvent[]): number {
  return ordered.length === 0 ? 0 : ordered[ordered.length - 1].seq;
}

export function replaceEvents(events: readonly ThreadEvent[]): EventStore {
  const bySeq = new Map<number, ThreadEvent>();
  for (const event of events) bySeq.set(event.seq, event);
  const ordered = [...bySeq.values()].sort((a, b) => a.seq - b.seq);
  return { bySeq, ordered, lastSeq: lastSeqOf(ordered) };
}

function insertOrdered(ordered: ThreadEvent[], event: ThreadEvent): void {
  const last = ordered[ordered.length - 1];
  if (last === undefined || event.seq > last.seq) {
    ordered.push(event);
    return;
  }
  let low = 0;
  let high = ordered.length;
  while (low < high) {
    const mid = (low + high) >> 1;
    if (ordered[mid].seq < event.seq) low = mid + 1;
    else high = mid;
  }
  ordered.splice(low, 0, event);
}

/**
 * Merges events in seq order, dropping seqs already held. Returns the same
 * store when nothing was added so React can skip the re-render.
 */
export function mergeEvents(
  store: EventStore,
  incoming: readonly ThreadEvent[],
): EventStore {
  if (incoming.length === 0) return store;
  if (store.ordered.length === 0) return replaceEvents(incoming);
  const bySeq = new Map(store.bySeq);
  let ordered = store.ordered;
  for (const event of incoming) {
    if (bySeq.has(event.seq)) continue;
    if (ordered === store.ordered) ordered = store.ordered.slice();
    bySeq.set(event.seq, event);
    insertOrdered(ordered, event);
  }
  if (ordered === store.ordered) return store;
  return { bySeq, ordered, lastSeq: lastSeqOf(ordered) };
}

