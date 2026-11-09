import { useEffect, useRef } from "react";
import { create } from "zustand";
import type { Anchor } from "@/api/types/Anchor";
import type { Focus } from "@/api/types/Focus";
import type { Panel } from "@/api/types/Panel";
import type { ServerWsMessage } from "@/api/types/ServerWsMessage";

export interface LocalFocus {
  target: Anchor | null;
  panel: Panel;
  typing: boolean;
}

interface PresenceState {
  focuses: Focus[];
  online: string[];
  projectId: string | null;
  local: LocalFocus;
  setPresence: (projectId: string, focuses: Focus[], online: string[]) => void;
  setLocal: (patch: Partial<LocalFocus>) => void;
  clear: () => void;
}

const initialLocal: LocalFocus = {
  target: null,
  panel: "transcript",
  typing: false,
};

export const usePresenceStore = create<PresenceState>((set) => ({
  focuses: [],
  online: [],
  projectId: null,
  local: initialLocal,
  setPresence: (projectId, focuses, online) =>
    set({ projectId, focuses, online }),
  setLocal: (patch) =>
    set((state) => ({ local: { ...state.local, ...patch } })),
  clear: () => set({ focuses: [], online: [], projectId: null }),
}));

function sendFocus(
  socket: WebSocket,
  projectId: string,
  threadId: string | undefined,
  hidden: boolean,
  local: LocalFocus,
) {
  if (socket.readyState !== WebSocket.OPEN) {
    return;
  }
  socket.send(
    JSON.stringify({
      type: "presence",
      projectId,
      target: local.target,
      threadId: threadId ?? null,
      panel: local.panel,
      typing: local.typing,
      hidden,
    }),
  );
}

/**
 * Send this client's semantic focus (thread, panel, anchor, typing) and receive
 * project-level presence. The connection is ephemeral; nothing is persisted.
 */
export function usePresence(
  projectId: string | undefined,
  threadId: string | undefined,
  hidden: boolean,
  local: LocalFocus,
) {
  const socketRef = useRef<WebSocket | null>(null);
  const latest = useRef({ projectId, threadId, hidden, local });

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
      socketRef.current = socket;
      socket.onopen = () => {
        const current = latest.current;
        sendFocus(
          socket as WebSocket,
          projectId,
          current.threadId,
          current.hidden,
          current.local,
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
      socketRef.current = null;
      usePresenceStore.getState().clear();
    };
  }, [projectId]);

  useEffect(() => {
    latest.current = { projectId, threadId, hidden, local };
    const socket = socketRef.current;
    if (socket && projectId) {
      sendFocus(socket, projectId, threadId, hidden, local);
    }
  }, [projectId, threadId, hidden, local]);
}
