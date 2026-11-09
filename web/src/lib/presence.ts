import { useEffect } from "react";
import { create } from "zustand";
import type { Focus } from "@/api/types/Focus";
import type { ServerWsMessage } from "@/api/types/ServerWsMessage";

interface PresenceState {
  focuses: Focus[];
  online: string[];
  projectId: string | null;
  setPresence: (projectId: string, focuses: Focus[], online: string[]) => void;
  clear: () => void;
}

export const usePresenceStore = create<PresenceState>((set) => ({
  focuses: [],
  online: [],
  projectId: null,
  setPresence: (projectId, focuses, online) =>
    set({ projectId, focuses, online }),
  clear: () => set({ focuses: [], online: [], projectId: null }),
}));

/**
 * Send this client's semantic focus and receive project-level presence.
 * The connection is ephemeral; nothing here is persisted or replayed.
 */
export function usePresence(
  projectId: string | undefined,
  threadId: string | undefined,
  hidden: boolean,
) {
  useEffect(() => {
    if (!projectId) {
      return;
    }
    let socket: WebSocket | null = null;
    let retry: number | undefined;
    let closed = false;
    const protocol = window.location.protocol === "https:" ? "wss" : "ws";
    const connect = () => {
      if (closed) {
        return;
      }
      socket = new WebSocket(`${protocol}://${window.location.host}/ws`);
      socket.onopen = () => {
        socket?.send(
          JSON.stringify({
            type: "presence",
            projectId,
            target: null,
            threadId: threadId ?? null,
            panel: "transcript",
            typing: false,
            hidden,
          }),
        );
      };
      socket.onmessage = (message) => {
        try {
          const parsed = JSON.parse(String(message.data)) as ServerWsMessage;
          if (parsed.type === "presence" && parsed.projectId === projectId) {
            usePresenceStore
              .getState()
              .setPresence(projectId, parsed.focuses, parsed.onlineUserIds);
          }
        } catch {
          // Ignore malformed presence frames; the next snapshot recovers.
        }
      };
      socket.onclose = () => {
        if (!closed) {
          usePresenceStore.getState().clear();
          retry = window.setTimeout(connect, 2_000);
        }
      };
    };
    connect();
    return () => {
      closed = true;
      if (retry !== undefined) {
        window.clearTimeout(retry);
      }
      socket?.close();
      usePresenceStore.getState().clear();
    };
  }, [projectId, threadId, hidden]);
}
