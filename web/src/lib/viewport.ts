/**
 * Keeps the app shell inside the visual viewport on phones, so the composer
 * stays above the software keyboard. iOS does not resize the layout viewport
 * when the keyboard opens; Android Chrome does (interactive-widget).
 */
export function installVisualViewportSync() {
  const viewport = window.visualViewport;
  if (!viewport) return;

  const root = document.documentElement;
  const phone = window.matchMedia("(max-width: 48em)");
  let frame = 0;

  const update = () => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      if (!phone.matches) {
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
  phone.addEventListener("change", update);
}
