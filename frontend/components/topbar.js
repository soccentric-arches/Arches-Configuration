import { el } from '../core/dom.js';
import { icon } from '../core/icons.js';
import { Button } from './button.js';

/** Topbar({ onToggleSidebar, onOpenPalette }) -> { element, setTitle, setBadge, setSession } */
export function Topbar({ onToggleSidebar, onOpenPalette }) {
  const title = el('span', { class: 'page-title' }, '');
  const badge = el('span', { class: 'badge badge-info', hidden: true });
  const session = el('span', { class: 'muted', style: 'font-size:13px' });

  const searchTrigger = el('button', {
    type: 'button', class: 'btn btn-outline btn-sm search-trigger', onclick: onOpenPalette,
  }, el('span', { class: 'row' }, icon('search', { size: 14 }), el('span', { class: 'search-trigger-label' }, 'Search…')),
  el('span', { class: 'cmdk-hint-kbd' }, '⌘K'));

  const element = el('div', { class: 'topbar' },
    Button({ variant: 'ghost', size: 'sm', icon: 'menu', onClick: onToggleSidebar, title: 'Toggle navigation' }),
    title,
    badge,
    el('div', { class: 'spacer' }),
    searchTrigger,
    session);

  return {
    element,
    setTitle: (t) => { title.textContent = t; },
    setBadge: (t) => { if (t) { badge.textContent = t.toUpperCase(); badge.hidden = false; } else badge.hidden = true; },
    setSession: (username, role) => { session.textContent = username ? `${username} · ${role || ''}` : ''; },
  };
}
