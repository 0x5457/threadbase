import { useCallback, useEffect, useRef, useState } from "react";
import type { Terminal } from "@xterm/xterm";
import type { FitAddon } from "@xterm/addon-fit";

type Options = {
  /** Watchers keep their keyboard out of the pty. */
  disabled: boolean;
  onData: (data: string) => void;
  onResize: (cols: number, rows: number) => void;
};

const FONT_STACK =
  'ui-monospace, "SFMono-Regular", "JetBrains Mono", Menlo, monospace';

/** Terminal colours follow the app palette instead of shipping a theme. */
function readTheme() {
  const style = getComputedStyle(document.documentElement);
  const read = (name: string, fallback: string) =>
    style.getPropertyValue(name).trim() || fallback;
  return {
    background: read("--paper", "#faf8f3"),
    foreground: read("--ink", "#33363d"),
    cursor: read("--brand", "#c2492c"),
    cursorAccent: read("--paper", "#faf8f3"),
    selectionBackground: read("--selected", "#e7e2d8"),
  };
}

/**
 * Mount xterm.js into a container and keep it fitted.
 *
 * xterm is loaded lazily: the drawer is the only consumer, and the editor-grade
 * emulator is the largest single dependency in the app.
 */
export function useXterm({ disabled, onData, onResize }: Options) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const [ready, setReady] = useState(false);

  const dataRef = useRef(onData);
  dataRef.current = onData;
  const resizeRef = useRef(onResize);
  resizeRef.current = onResize;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    let disposed = false;
    let observer: ResizeObserver | null = null;
    let frame = 0;

    void (async () => {
      const [{ Terminal }, { FitAddon }] = await Promise.all([
        import("@xterm/xterm"),
        import("@xterm/addon-fit"),
      ]);
      if (disposed) return;

      const terminal = new Terminal({
        allowProposedApi: true,
        convertEol: false,
        cursorBlink: true,
        fontFamily: FONT_STACK,
        fontSize: 12.5,
        lineHeight: 1.25,
        scrollback: 5000,
        theme: readTheme(),
      });
      const fit = new FitAddon();
      terminal.loadAddon(fit);
      terminal.open(container);
      fit.fit();
      termRef.current = terminal;
      fitRef.current = fit;

      terminal.onData((data) => dataRef.current(data));
      terminal.attachCustomKeyEventHandler((event) => {
        if (event.type !== "keydown") return true;
        const modifier = event.ctrlKey || event.metaKey;
        if (modifier && event.shiftKey && event.key.toLowerCase() === "c") {
          const selection = terminal.getSelection();
          if (selection) void navigator.clipboard.writeText(selection);
          return false;
        }
        if (modifier && event.shiftKey && event.key.toLowerCase() === "v") {
          void navigator.clipboard.readText().then((text) => {
            if (!text) return;
            const lines = text.split("\n").length;
            if (
              lines > 1 &&
              !window.confirm(`Paste ${lines} lines into the terminal?`)
            ) {
              return;
            }
            dataRef.current(text);
          });
          return false;
        }
        return true;
      });

      observer = new ResizeObserver(() => {
        if (frame) return;
        frame = window.requestAnimationFrame(() => {
          frame = 0;
          try {
            fit.fit();
          } catch {
            // fit() throws while the drawer is collapsed or hidden.
            return;
          }
          resizeRef.current(terminal.cols, terminal.rows);
        });
      });
      observer.observe(container);

      if (!disabled) terminal.focus();
      setReady(true);
      resizeRef.current(terminal.cols, terminal.rows);
    })();

    return () => {
      disposed = true;
      if (frame) window.cancelAnimationFrame(frame);
      observer?.disconnect();
      termRef.current?.dispose();
      termRef.current = null;
      fitRef.current = null;
      setReady(false);
    };
    // The terminal instance lives for the lifetime of the drawer; option
    // changes are handled by the effects below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (termRef.current) termRef.current.options.disableStdin = disabled;
  }, [disabled]);

  const write = useCallback((data: string) => {
    termRef.current?.write(data);
  }, []);

  const reset = useCallback(() => {
    termRef.current?.reset();
  }, []);

  const focus = useCallback(() => {
    termRef.current?.focus();
  }, []);

  const size = useCallback(
    () => ({
      cols: termRef.current?.cols ?? 120,
      rows: termRef.current?.rows ?? 30,
    }),
    [],
  );

  return { containerRef, ready, write, reset, focus, size };
}
