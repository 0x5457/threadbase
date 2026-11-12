/**
 * Keeps the app shell inside the visual viewport on phones and tablets, so the
 * composer stays above the software keyboard. iOS does not resize the layout
 * viewport when the keyboard opens (Android Chrome does, through
 * interactive-widget); matching coarse pointers covers iPads as well, where the
 * width is often above the phone breakpoint.
 */
export function installVisualViewportSync() {
  const viewport = window.visualViewport;
  if (!viewport) return;

  const root = document.documentElement;
  const compact = window.matchMedia(
    "(max-width: 74.99em), (pointer: coarse)",
  );
  let frame = 0;

  const update = () => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      if (!compact.matches) {
        root.style.removeProperty("--app-height");
        root.style.removeProperty("--app-offset");
        return;
      }
      root.style.setProperty("--app-height", `${Math.round(viewport.height)}px`);
      root.style.setProperty(
        "--app-offset",
        `${Math.round(viewport.offsetTop)}px`,
      );
    });
  };

  update();
  viewport.addEventListener("resize", update);
  viewport.addEventListener("scroll", update);
  window.addEventListener("orientationchange", update);
  compact.addEventListener("change", update);
}
