/**
 * Keeps the app shell pinned to the *visible* viewport.
 *
 * `100dvh` tracks browser chrome, but on iOS the software keyboard changes
 * only the visual viewport: the layout viewport stays at full height and the
 * bottom composer would sit underneath the keyboard. Mirroring the visual
 * viewport height into `--tb-app-height` shrinks the shell — and with it the
 * timeline and composer — to exactly the area above the keyboard.
 *
 * The value is written from `visualViewport`, where supported, and the CSS
 * falls back to `100dvh` (and then `100%`) everywhere else.
 */
export function installVisualViewportSync(): () => void {
  const root = document.documentElement;
  const viewport = window.visualViewport;

  if (!viewport) {
    return () => {};
  }

  let frame = 0;

  const apply = () => {
    frame = 0;
    // A pinch-zoomed viewport reports only the zoomed-in slice. Shrinking the
    // shell then would be wrong — the user zoomed in, the keyboard did not
    // open. Keep the last keyboard-free height instead.
    if (viewport.scale !== 1) {
      return;
    }
    root.style.setProperty("--tb-app-height", `${viewport.height}px`);
  };

  const schedule = () => {
    if (frame !== 0) {
      return;
    }
    frame = window.requestAnimationFrame(apply);
  };

  viewport.addEventListener("resize", schedule);
  viewport.addEventListener("scroll", schedule);
  apply();

  return () => {
    window.cancelAnimationFrame(frame);
    viewport.removeEventListener("resize", schedule);
    viewport.removeEventListener("scroll", schedule);
    root.style.removeProperty("--tb-app-height");
  };
}
