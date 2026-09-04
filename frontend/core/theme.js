// Light/dark theme: applied via <html data-theme="…"> and consumed purely
// through CSS tokens (styles/tokens.css), so switching is one attribute
// write. Persistence is localStorage — the same client-side pattern as the
// remembered username in core/auth.js. The inline script in index.html
// applies the stored value (or the OS preference on first visit) before
// first paint to avoid a theme flash; this module owns runtime switches.

const STORAGE_KEY = 'webconfig-theme';

/** currentTheme() -> 'light' | 'dark' */
export function currentTheme() {
  return document.documentElement.getAttribute('data-theme') === 'dark' ? 'dark' : 'light';
}

function apply(theme) {
  document.documentElement.setAttribute('data-theme', theme);
  // Keep the meta tag in sync so native widgets (form controls, scrollbars)
  // follow the palette even before CSS custom properties are read.
  const meta = document.querySelector('meta[name="color-scheme"]');
  if (meta) meta.setAttribute('content', theme);
  // The topbar toggle (and anything else) re-renders on this.
  document.dispatchEvent(new CustomEvent('themechange', { detail: { theme } }));
}

/** setTheme('light' | 'dark') — applies immediately and remembers the choice. */
export function setTheme(theme) {
  apply(theme);
  try { localStorage.setItem(STORAGE_KEY, theme); } catch (_) { /* storage unavailable — apply runtime-only */ }
}

/** toggleTheme() — flips between light and dark, persisting the new choice. */
export function toggleTheme() {
  setTheme(currentTheme() === 'dark' ? 'light' : 'dark');
}