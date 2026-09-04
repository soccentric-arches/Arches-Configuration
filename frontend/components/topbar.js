import { el } from '../core/dom.js';
import { icon } from '../core/icons.js';
import { Button } from './button.js';
import { ThemeSwitch } from './theme-switch.js';

/** Topbar({ onToggleSidebar, onOpenPalette, onLogout }) -> { element, setTitle, setBadge, setSession, setBoard } */
export function Topbar({ onToggleSidebar, onOpenPalette, onLogout }) {
  const title = el('span', { class: 'page-title' }, '');
  const badge = el('span', { class: 'badge badge-info', hidden: true });
  const session = el('span', { class: 'muted text-sm' });
  // Mobile-only: opens the off-canvas drawer. The desktop collapse/expand
  // control lives inside the sidebar itself (components/sidebar.js) — the
  // topbar no longer duplicates it.
  const mobileToggle = Button({
    variant: 'ghost', size: 'sm', icon: 'menu', title: 'Open navigation',
    onClick: onToggleSidebar,
  });
  mobileToggle.classList.add('sidebar-mobile-toggle');

  // Brand block doubles as the board identity header: app name on the first
  // line, device model + serial on the second (setBoard, below). Like a real
  // board's management header, the device is identified at a glance.
  const brandSub = el('span', { class: 'brand-sub', hidden: true });
  const brand = el('div', { class: 'topbar-brand' },
    el('span', { class: 'logo-mark' }, 'W'),
    el('span', { class: 'brand-text-group' },
      el('span', { class: 'brand-text' }, 'WebConfig'),
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
    setTitle: (t) => { title.textContent = t; },
    setBadge: (t) => { if (t) { badge.textContent = t.toUpperCase(); badge.hidden = false; } else badge.hidden = true; },
    setSession: (username, role) => { session.textContent = username ? `${username} · ${role || ''}` : ''; },
    // setBoard(info) — feed the payload of GET /api/v1/system/info. Shows
    // model (falling back to board name / platform) plus serial; hides the
    // line entirely when nothing is known (pre-auth, fetch failure).
    setBoard: (info) => {
      const model = info && (info.model || info.board_name || info.platform) || '';
      const serial = info && info.serial_number ? ` · SN ${info.serial_number}` : '';
      const text = `${model}${serial}`;
      brandSub.textContent = text;
      brandSub.hidden = !text;
    },
  };
}