// Top bar: brand (home button + board identity), page title, search, theme
// switch, session, logout. Navigation lives in the sidebar; on narrow
// screens the hamburger here opens it as an off-canvas drawer.

import { el } from '../core/dom.js';
import { icon } from '../core/icons.js';
import { XLogo } from './logo.js';
import { Button } from './button.js';
import { ThemeSwitch } from './theme-switch.js';

/** Topbar({ onToggleSidebar, onOpenPalette, onLogout, onHome }) -> { element, setTitle, setBadge, setSession, setBoard } */
export function Topbar({ onToggleSidebar, onOpenPalette, onLogout, onHome }) {
  const title = el('span', { class: 'page-title' }, '');
  const badge = el('span', { class: 'badge badge-info', hidden: true });
  const session = el('span', { class: 'muted text-sm' });
  // Mobile-only: opens the off-canvas drawer.
  const mobileToggle = Button({
    variant: 'ghost', size: 'sm', icon: 'menu', title: 'Open navigation',
    onClick: onToggleSidebar,
  });
  mobileToggle.classList.add('sidebar-mobile-toggle');

  // Brand block doubles as the board identity header AND the home button:
  // app name over device model + serial (setBoard, below).
  const brandSub = el('span', { class: 'brand-sub', hidden: true });
  const brand = el('button', {
    type: 'button', class: 'topbar-brand',
    title: 'Go to overview', 'aria-label': 'Go to overview',
    onclick: onHome,
  },
    el('span', { class: 'logo-mark' }, XLogo({ size: 30 })),
    el('span', { class: 'brand-text-group' },
      el('span', { class: 'brand-text' }, 'XConfig'),
      brandSub));

  const searchTrigger = el('button', {
    type: 'button', class: 'btn btn-outline search-trigger', onclick: onOpenPalette,
    'aria-label': 'Search pages and actions',
  }, el('span', { class: 'search-trigger-copy' }, icon('search', { size: 15 }), el('span', { class: 'search-trigger-label' }, 'Search pages and actions')),
    el('kbd', { class: 'cmdk-hint-kbd' }, 'Ctrl K'));

  const logoutButton = Button({
    variant: 'ghost', size: 'sm', icon: 'logout', title: 'Log out', onClick: onLogout,
  });
  logoutButton.classList.add('topbar-logout');

  const themeSwitch = ThemeSwitch();

  const element = el('div', { class: 'topbar' },
    brand,
    mobileToggle,
    el('div', { class: 'topbar-heading' }, title),
    badge,
    el('div', { class: 'spacer' }),
    searchTrigger,
    themeSwitch,
    session,
    logoutButton);

  return {
    element,
    setTitle: (t) => {
      if (title.textContent === t) return;
      title.textContent = t;
      // Re-trigger the slide-in so route changes feel fluid (see base.css).
      title.classList.remove('page-title-enter');
      void title.offsetWidth;
      title.classList.add('page-title-enter');
    },
    setBadge: (t) => {
      if (t) { badge.textContent = t.toUpperCase(); badge.hidden = false; }
      else badge.hidden = true;
    },
    setSession: (username, role) => {
      session.textContent = username ? `${username} · ${role || ''}` : '';
    },
    // setBoard(info) — payload of GET /api/v1/system/info: model (falling
    // back to board name / platform) plus serial; hidden when unknown.
    setBoard: (info) => {
      const model = info && (info.model || info.board_name || info.platform) || '';
      const serial = info && info.serial_number ? ` · SN ${info.serial_number}` : '';
      const text = `${model}${serial}`;
      brandSub.textContent = text;
      brandSub.hidden = !text;
    },
  };
}