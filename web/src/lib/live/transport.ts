import type { ClientMessage } from "./types";

const LIVE_SOCKET_PATH = "/ws";
export const HEARTBEAT_MS = 25_000;
const RECONNECT_BASE_MS = 500;
const RECONNECT_MAX_MS = 10_000;
/** A connection that lived this long resets the backoff ladder. */
export const RECONNECT_STABLE_MS = 10_000;
export const SOCKET_CONNECTING = 0;
export const SOCKET_OPEN = 1;

export function socketUrl(): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}${LIVE_SOCKET_PATH}`;
}

export function sendJson(socket: WebSocket | null, message: ClientMessage): void {
  if (socket === null || socket.readyState !== SOCKET_OPEN) return;
  try {
    socket.send(JSON.stringify(message));
  } catch {
    // The socket is tearing down; `onclose` owns the reconnect.
  }
}

export function backoffDelay(attempt: number): number {
  const base = Math.min(
    RECONNECT_MAX_MS,
    RECONNECT_BASE_MS * 2 ** Math.min(attempt, 8),
  );
  const jittered = base * (0.75 + Math.random() * 0.5);
  return Math.min(RECONNECT_MAX_MS, Math.round(jittered));
}

export function errorMessage(cause: unknown, fallback: string): string {
  if (cause instanceof Error && cause.message !== "") return cause.message;
  return fallback;
}

