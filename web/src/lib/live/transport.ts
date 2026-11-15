import type { ClientMessage } from "./types";

const LIVE_SOCKET_PATH = "/ws";
export const HEARTBEAT_MS = 25_000;
/** A connection that lived this long resets the reconnect backoff ladder. */
const RECONNECT_STABLE_MS = 10_000;

const SOCKET_OPEN = 1;

/** Reconnect behavior passed to partysocket's reconnecting WebSocket. */
export const RECONNECT_OPTIONS = {
  minReconnectionDelay: 500,
  maxReconnectionDelay: 10_000,
  reconnectionDelayGrowFactor: 2,
  minUptime: RECONNECT_STABLE_MS,
} as const;

interface SendableSocket {
  readyState: number;
  send(data: string): unknown;
}

export function socketUrl(): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}${LIVE_SOCKET_PATH}`;
}

export function sendJson(
  socket: SendableSocket | null,
  message: ClientMessage,
): void {
  if (socket === null || socket.readyState !== SOCKET_OPEN) return;
  try {
    socket.send(JSON.stringify(message));
  } catch {
    // The socket is tearing down; its close handler owns the reconnect.
  }
}

export function errorMessage(cause: unknown, fallback: string): string {
  if (cause instanceof Error && cause.message !== "") return cause.message;
  return fallback;
}
