/**
 * Applies the stored colour scheme before the app renders.
 *
 * This lives in a file rather than inline in index.html because the server's
 * CSP is `script-src 'self'`; an inline bootstrap script is blocked, which
 * showed up as a flash of the wrong theme on every load.
 */
try {
  const theme = localStorage.getItem("threadbase.theme");
  if (theme === "dark" || theme === "light") {
    document.documentElement.dataset.mantineColorScheme = theme;
    document.documentElement.style.colorScheme = theme;
  }
  // Match the phone's browser/status bar to the app frame before the first
  // paint. React updates this again when the user toggles the theme.
  const dark = theme === "dark";
  document
    .querySelector('meta[name="theme-color"]')
    ?.setAttribute("content", dark ? "#1c1f24" : "#f4f4f5");
} catch {
  // Private mode: fall through to the default scheme.
}
