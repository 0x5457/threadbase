import "@xterm/xterm/css/xterm.css";
import { ActionIcon, Button, Group, Text, Tooltip } from "@mantine/core";
import { useMediaQuery } from "@mantine/hooks";
import {
  IconKeyboard,
  IconKeyboardOff,
  IconMaximize,
  IconMinimize,
  IconRefresh,
  IconTerminal2,
  IconX,
} from "@tabler/icons-react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";
import {
  ApiError,
  attachTerminalSession,
  createTerminalSession,
  killTerminalSession,
  listTerminalSessions,
  terminalSocketUrl,
  type TerminalSession,
} from "../../api";
import { useXterm } from "./use-xterm";

// The phase union is generated from the `terminal` machine; no hand-written
// string set to drift from the catalogue.
import type { TerminalPhase } from "../../api/types/TerminalPhase";
import { can } from "../../lib/machine";

type ServerMessage =
  | {
      type: "ready";
      role: "controller" | "watcher";
      session: string;
      cwd: string;
      controller: string | null;
      viewers: number;
    }
  | { type: "output"; data: string }
  | { type: "control"; controller: string | null }
  | { type: "exit"; code: number };

/** Keys the browser or xterm swallows, one tap away from the drawer footer. */
const KEY_BAR = [
  { label: "esc", data: "\u001b" },
  { label: "tab", data: "\t" },
  { label: "⌃C", data: "\u0003" },
  { label: "⌃D", data: "\u0004" },
  { label: "⌃L", data: "\u000c" },
  { label: "⌃R", data: "\u0012" },
  { label: "⌃Z", data: "\u001a" },
  { label: "↑", data: "\u001b[A" },
  { label: "↓", data: "\u001b[B" },
  { label: "←", data: "\u001b[D" },
  { label: "→", data: "\u001b[C" },
  { label: "|", data: "|" },
  { label: "~", data: "~" },
];

const MIN_HEIGHT = 160;
const MAX_HEIGHT = 720;
/** A lone LF this soon after a CR is the same Enter, not a second one. */
const LEFTOVER_NEWLINE_MS = 200;

/**
 * Admins create, everyone else attaches to a shell that already exists. Keeping
 * this in one place means the drawer does not need to know the caller's role.
 */
async function openSession(
  threadId: string | null,
  size: { cols: number; rows: number },
) {
  try {
    return await createTerminalSession({
      threadId,
      cols: size.cols,
      rows: size.rows,
    });
  } catch (error) {
    if (error instanceof ApiError && error.status === 404) {
      throw new Error(
        "The terminal is turned off on this server (THREADBASE_TERMINAL=1).",
      );
    }
    if (!(error instanceof ApiError) || error.status !== 403) throw error;
    const listed = await listTerminalSessions(threadId);
    const existing = listed.sessions.find(
      (entry) => (entry.threadId ?? null) === threadId,
    );
    if (!existing) {
      throw new Error(
        threadId
          ? "Only an administrator can start a terminal in this thread."
          : "Only an administrator can start a terminal on this machine.",
      );
    }
    return attachTerminalSession(existing.id);
  }
}

function storedHeight(): number {
  try {
    const stored = Number(window.localStorage.getItem("threadbase.terminal-height"));
    return Number.isFinite(stored) && stored >= MIN_HEIGHT ? stored : 300;
  } catch {
    return 300;
  }
}

export function TerminalDrawer({
  threadId,
  fullscreen,
  onFullscreen,
  onClose,
}: {
  threadId: string | null;
  fullscreen: boolean;
  onFullscreen: (value: boolean) => void;
  onClose: () => void;
}) {
  const [height, setHeight] = useState(storedHeight);
  const [phase, setPhase] = useState<TerminalPhase>("connecting");
  const [role, setRole] = useState<"controller" | "watcher">("controller");
  // Only read from handlers (kill), so keep it out of the render state.
  const sessionRef = useRef<TerminalSession | null>(null);
  const [controller, setController] = useState<string | null>(null);
  const [viewers, setViewers] = useState(0);
  const [cwd, setCwd] = useState("");
  const [exitCode, setExitCode] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showKeys, setShowKeys] = useState(true);
  const [attempt, setAttempt] = useState(0);
  // Phones lay the drawer out full-screen; they drive the shell like any other
  // screen, which means the soft keyboard has to reach xterm's textarea.
  const phone = useMediaQuery("(max-width: 47.99em)");

  const socketRef = useRef<WebSocket | null>(null);
  const sendRef = useRef<(message: unknown) => void>(() => {});
  // When the last CR went out, so a leftover LF can be recognised below.
  const lastCarriageReturn = useRef(0);

  const send = useCallback((message: unknown) => sendRef.current(message), []);
  const onData = useCallback(
    (data: string) => {
      // A keyboard Enter is always a CR. A lone LF arriving right behind one is
      // the newline a soft keyboard or an IME left in xterm's helper textarea,
      // which xterm flushes as a second Enter — one keypress, two newlines at
      // the prompt. CR frames are never touched, so key repeat still works.
      if (data === "\r") {
        lastCarriageReturn.current = performance.now();
      } else if (
        data === "\n" &&
        performance.now() - lastCarriageReturn.current < LEFTOVER_NEWLINE_MS
      ) {
        return;
      }
      send({ type: "input", data });
    },
    [send],
  );
  const onResize = useCallback(
    (cols: number, rows: number) => send({ type: "resize", cols, rows }),
    [send],
  );

  const { containerRef, ready, write, reset, focus, size } = useXterm({
    disabled: role === "watcher",
    onData,
    onResize,
  });

  useEffect(() => {
    if (!ready) return;
    let cancelled = false;
    setPhase("connecting");
    setError(null);
    setExitCode(null);

    const connect = async () => {
      try {
        const created = await openSession(threadId, size());
        if (cancelled) return;
        sessionRef.current = created.session;
        setCwd(created.session.cwd);

        const socket = new WebSocket(
          terminalSocketUrl(created.session.id, created.ticket),
        );
        socketRef.current = socket;
        socket.onopen = () => {
          sendRef.current = (message) => {
            if (socket.readyState === WebSocket.OPEN) {
              socket.send(JSON.stringify(message));
            }
          };
          sendRef.current({
            type: "resize",
            cols: size().cols,
            rows: size().rows,
          });
        };
        socket.onmessage = (event) => {
          let message: ServerMessage;
          try {
            message = JSON.parse(String(event.data)) as ServerMessage;
          } catch {
            return;
          }
          switch (message.type) {
            case "ready":
              setRole(message.role);
              setController(message.controller);
              setViewers(message.viewers);
              setCwd(message.cwd);
              setPhase("live");
              break;
            case "output":
              write(message.data);
              break;
            case "control":
              setController(message.controller);
              break;
            case "exit":
              setExitCode(message.code);
              setPhase("exited");
              break;
          }
        };
        socket.onclose = () => {
          if (cancelled) return;
          setPhase((current) => (current === "exited" ? current : "error"));
          setError((current) => current ?? "The terminal connection closed.");
        };
      } catch (caught) {
        if (cancelled) return;
        setPhase("error");
        setError(
          caught instanceof Error ? caught.message : "Could not open the terminal",
        );
      }
    };

    void connect();
    return () => {
      cancelled = true;
      sendRef.current = () => {};
      socketRef.current?.close();
      socketRef.current = null;
    };
  }, [ready, threadId, attempt, size, write]);

  // Drag the top edge to trade transcript space for terminal height.
  const startResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const startY = event.clientY;
    const startHeight = height;
    let latest = startHeight;
    const target = event.currentTarget;
    target.setPointerCapture(event.pointerId);
    const move = (moveEvent: PointerEvent) => {
      const next = Math.max(
        MIN_HEIGHT,
        Math.min(
          MAX_HEIGHT,
          startHeight - (moveEvent.clientY - startY),
        ),
      );
      latest = next;
      setHeight(next);
    };
    const up = () => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", up);
      try {
        window.localStorage.setItem(
          "threadbase.terminal-height",
          String(latest),
        );
      } catch {
        // Private mode: keep the in-memory height.
      }
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", up);
  };

  const close = () => {
    onClose();
  };

  const reconnect = () => {
    reset();
    setAttempt((current) => current + 1);
  };

  const kill = async () => {
    const session = sessionRef.current;
    if (!session) {
      onClose();
      return;
    }
    try {
      await killTerminalSession(session.id);
    } catch {
      // The session is gone either way; closing the drawer is what matters.
    }
    onClose();
  };

  const canDrive = role === "controller" && can("terminal", phase, "drive");
  const watchNote =
    controller === null
      ? "Watching: nobody is driving. Only an administrator can type in this shell."
      : `Watching: ${controller} is driving. The keyboard is off for you.`;

  return (
    <section
      className="terminal-drawer"
      data-fullscreen={fullscreen || phone ? "true" : undefined}
      style={fullscreen || phone ? undefined : { height }}
      aria-label="Terminal"
    >
      {fullscreen ? null : (
        <div
          className="terminal-resize"
          role="separator"
          tabIndex={0}
          aria-orientation="horizontal"
          aria-label="Resize terminal"
          onPointerDown={startResize}
          onDoubleClick={() => setHeight(300)}
          onKeyDown={(event) => {
            if (event.key !== "ArrowUp" && event.key !== "ArrowDown") {
              return;
            }
            event.preventDefault();
            const delta = event.key === "ArrowUp" ? 16 : -16;
            setHeight((current) =>
              Math.max(MIN_HEIGHT, Math.min(MAX_HEIGHT, current + delta)),
            );
          }}
        />
      )}
      <div className="terminal-head">
        <IconTerminal2 size={13} stroke={1.5} className="terminal-icon" />
        <span className={`terminal-dot ${phase}`} aria-hidden />
        <Text size="xs" className="terminal-title">
          {threadId ? "terminal" : "terminal · home"}
        </Text>
        <Text
          size="xs"
          c="dimmed"
          className="terminal-cwd"
          title={cwd || "…"}
          onClick={() => {
            if (cwd) void navigator.clipboard.writeText(cwd);
          }}
        >
          {cwd || "…"}
        </Text>
        <div className="terminal-head-gap" />
        {phase === "live" ? (
          <Text size="xs" c="dimmed" className="terminal-role">
            {role === "controller" ? "driving" : "watching"}
            {viewers > 1 ? ` · ${viewers}` : ""}
          </Text>
        ) : null}
        <Tooltip label={showKeys ? "Hide keys" : "Show keys"}>
          <ActionIcon
            aria-label="Toggle the key bar"
            onClick={() => setShowKeys((current) => !current)}
          >
            {showKeys ? (
              <IconKeyboard size={14} stroke={1.5} />
            ) : (
              <IconKeyboardOff size={14} stroke={1.5} />
            )}
          </ActionIcon>
        </Tooltip>
        <Tooltip label="Reconnect">
          <ActionIcon aria-label="Reconnect" onClick={reconnect}>
            <IconRefresh size={14} stroke={1.5} />
          </ActionIcon>
        </Tooltip>
        <Tooltip label={fullscreen ? "Leave fullscreen" : "Fullscreen"}>
          <ActionIcon
            aria-label="Toggle fullscreen"
            onClick={() => onFullscreen(!fullscreen)}
          >
            {fullscreen ? (
              <IconMinimize size={14} stroke={1.5} />
            ) : (
              <IconMaximize size={14} stroke={1.5} />
            )}
          </ActionIcon>
        </Tooltip>
        <Tooltip label="Close (the shell keeps running)">
          <ActionIcon aria-label="Close terminal" onClick={close}>
            <IconX size={14} stroke={1.5} />
          </ActionIcon>
        </Tooltip>
      </div>

      {phase === "error" ? (
        <div className="terminal-notice" data-kind="error">
          <Text size="xs">{error ?? "The terminal is unavailable."}</Text>
          <Button variant="light" size="compact-xs" onClick={reconnect}>
            Retry
          </Button>
        </div>
      ) : null}

      {phase === "exited" ? (
        <div className="terminal-notice" data-kind="exited">
          <Text size="xs">
            The shell exited{exitCode === null ? "" : ` (${exitCode})`}.
          </Text>
          <Group gap={6}>
            <Button variant="light" size="compact-xs" onClick={reconnect}>
              New session
            </Button>
            <Button
              variant="subtle"
              color="red"
              size="compact-xs"
              onClick={() => void kill()}
            >
              Close session
            </Button>
          </Group>
        </div>
      ) : null}

      {phase === "live" && role === "watcher" ? (
        <div className="terminal-notice" data-kind="watching">
          <Text size="xs">{watchNote}</Text>
        </div>
      ) : null}

      <div
        className="terminal-body"
        role="group"
        aria-label="Terminal session"
        ref={containerRef}
        onPointerDown={() => {
          if (canDrive) focus();
        }}
        onTouchStart={() => {
          if (canDrive) focus();
        }}
      />

      {showKeys ? (
        <div className="terminal-keys">
          {KEY_BAR.map((key) => (
            <button
              key={key.label}
              type="button"
              className="terminal-key"
              disabled={!canDrive}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => {
                send({ type: "input", data: key.data });
                focus();
              }}
            >
              {key.label}
            </button>
          ))}
          <div className="terminal-head-gap" />
          {role === "controller" ? (
            <button
              type="button"
              className="terminal-key terminal-key-danger"
              onClick={() => void kill()}
            >
              kill
            </button>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
