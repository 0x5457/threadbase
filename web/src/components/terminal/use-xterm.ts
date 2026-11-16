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
  '"Threadbase Nerd Mono", ui-monospace, "SFMono-Regular", "JetBrains Mono", Menlo, monospace';
const FONT_SIZE = 12.5;

/** Terminal colours follow the app palette instead of shipping a theme. */
function readTheme() {
  const style = getComputedStyle(document.documentElement);
  const read = (name: string, fallback: string) =>
    style.getPropertyValue(name).trim() || fallback;
  return {
    background: read("--paper", "#282c33"),
    foreground: read("--ink", "#dce0e5"),
    cursor: read("--brand", "#74ade8"),
    cursorAccent: read("--paper", "#282c33"),
    selectionBackground: read("--selected", "#454a56"),
    // Zed's `terminal.ansi.*` palette, exposed as CSS variables so the drawer
    // follows the active colour scheme.
    black: read("--ansi-black", "#282c34"),
    red: read("--ansi-red", "#e06c75"),
    green: read("--ansi-green", "#98c379"),
    yellow: read("--ansi-yellow", "#e5c07b"),
    blue: read("--ansi-blue", "#61afef"),
    magenta: read("--ansi-magenta", "#c678dd"),
    cyan: read("--ansi-cyan", "#56b6c2"),
    white: read("--ansi-white", "#abb2bf"),
    brightBlack: read("--ansi-bright-black", "#636d83"),
    brightRed: read("--ansi-bright-red", "#ea858b"),
    brightGreen: read("--ansi-bright-green", "#aad581"),
    brightYellow: read("--ansi-bright-yellow", "#ffd885"),
    brightBlue: read("--ansi-bright-blue", "#85c1ff"),
    brightMagenta: read("--ansi-bright-magenta", "#d398eb"),
    brightCyan: read("--ansi-bright-cyan", "#6ed5de"),
    brightWhite: read("--ansi-bright-white", "#fafafa"),
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
  useEffect(() => {
    dataRef.current = onData;
  }, [onData]);
  const resizeRef = useRef(onResize);
  useEffect(() => {
    resizeRef.current = onResize;
  }, [onResize]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    let disposed = false;
    let frame = 0;

    // Own the subscription in the effect body so the cleanup below provably
    // disconnects it, even while the terminal import is still in flight.
    const observer = new ResizeObserver(() => {
      if (frame) return;
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        const terminal = termRef.current;
        const fit = fitRef.current;
        if (!terminal || !fit) return;
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
        fontSize: FONT_SIZE,
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

      // The soft keyboard types into this textarea. Mobile browsers only open it
      // for an editable field with sane autocorrect/spellcheck settings, and
      // `enterkeyhint` makes the return key say what it does.
      const textarea = terminal.textarea;
      if (textarea) {
        textarea.setAttribute("autocapitalize", "off");
        textarea.setAttribute("autocorrect", "off");
        textarea.setAttribute("autocomplete", "off");
        textarea.setAttribute("spellcheck", "false");
        textarea.setAttribute("inputmode", "text");
        textarea.setAttribute("enterkeyhint", "enter");
        textarea.setAttribute("aria-label", "Terminal input");
      }

      terminal.onData((data) => dataRef.current(data));
      terminal.attachCustomKeyEventHandler((event) => {
        if (event.type !== "keydown") return true;
        const modifier = event.ctrlKey || event.metaKey;
        // Enter is sent here rather than left to xterm: a soft keyboard can
        // still insert the newline into the helper textarea after the keydown
        // was cancelled, and xterm flushes that leftover later as a second
        // Enter. Sending the CR and emptying the textarea leaves nothing to
        // flush. Composing keys, and IME text still waiting in the textarea,
        // stay with xterm so a commit is never swallowed.
        const pending = textarea?.value.replace(/[\r\n]+/g, "") ?? "";
        if (event.key === "Enter" && !event.isComposing && !pending) {
          event.preventDefault();
          if (textarea) textarea.value = "";
          if (terminal.options.scrollOnUserInput) terminal.scrollToBottom();
          if (!terminal.options.disableStdin) dataRef.current("\r");
          return false;
        }
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

      if (!disabled) terminal.focus();
      setReady(true);
      resizeRef.current(terminal.cols, terminal.rows);
      // The Nerd Font has 1 MB of icon glyphs and may arrive after the first
      // paint. Re-measure and repaint once it is actually available, otherwise
      // xterm keeps the fallback font's cell metrics and shows tofu boxes.
      void document.fonts.ready.then(() => {
        if (disposed) return;
        try {
          fit.fit();
          terminal.refresh(0, Math.max(0, terminal.rows - 1));
        } catch {
          return;
        }
        resizeRef.current(terminal.cols, terminal.rows);
      });
    })();

    return () => {
      disposed = true;
      if (frame) window.cancelAnimationFrame(frame);
      observer.disconnect();
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
