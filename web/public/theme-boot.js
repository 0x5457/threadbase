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
} catch {
  // Private mode: fall through to the default scheme.
}
