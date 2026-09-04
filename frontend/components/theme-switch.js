// Segmented light/dark switch for the title bar. Two explicit options —
// Light and Dark — with the active one highlighted and aria-pressed, synced
// to <html data-theme> via the 'themechange' event from core/theme.js.

import { el } from '../core/dom.js';
import { icon } from '../core/icons.js';
import { currentTheme, setTheme } from '../core/theme.js';

export function ThemeSwitch() {
  const lightBtn = el('button', { type: 'button', class: 'theme-opt' },
    icon('sun', { size: 13 }), el('span', { class: 'theme-opt-label' }, 'Light'));
  const darkBtn = el('button', { type: 'button', class: 'theme-opt' },
    icon('moon', { size: 13 }), el('span', { class: 'theme-opt-label' }, 'Dark'));

  const element = el('div', { class: 'theme-switch', role: 'group', 'aria-label': 'Color theme' },
    lightBtn, darkBtn);

  const sync = () => {
    const theme = currentTheme();
    lightBtn.classList.toggle('theme-opt-active', theme === 'light');
    darkBtn.classList.toggle('theme-opt-active', theme === 'dark');
    lightBtn.setAttribute('aria-pressed', String(theme === 'light'));
    darkBtn.setAttribute('aria-pressed', String(theme === 'dark'));
  };
  lightBtn.addEventListener('click', () => setTheme('light'));
  darkBtn.addEventListener('click', () => setTheme('dark'));
  document.addEventListener('themechange', sync);
  sync();

  return element;
}