// Command palette (Cmd/Ctrl+K) — fuzzy-jump to any page or run a quick
// action without touching the sidebar. A small, self-contained overlay;
// register actions once at boot via registerCommands().

import { el } from '../core/dom.js';
import { icon } from '../core/icons.js';

let commands = [];
let overlay = null;
let inputEl = null;
let listEl = null;
let selected = 0;
let filtered = [];

/** registerCommands([{ id, label, hint, iconName, group, run }]) — additive. */
export function registerCommands(cmds) {
  commands = commands.concat(cmds);
}

function fuzzyScore(query, text) {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (!q) return 1;
  if (t.includes(q)) return 100 - t.indexOf(q);
  // Loose subsequence match so "netcfm" still finds "Networking: confirm".
  let qi = 0;
  for (let ti = 0; ti < t.length && qi < q.length; ti++) if (t[ti] === q[qi]) qi++;
  return qi === q.length ? 10 : -1;
}

function render() {
  const query = inputEl.value;
  filtered = commands
    .map(c => ({ c, score: fuzzyScore(query, `${c.group || ''} ${c.label} ${c.hint || ''}`) }))
    .filter(x => x.score > 0)
    .sort((a, b) => b.score - a.score)
    .map(x => x.c)
    .slice(0, 8);
  selected = Math.min(selected, Math.max(0, filtered.length - 1));

  listEl.replaceChildren(...filtered.map((c, i) => el('div', {
    class: `cmdk-item${i === selected ? ' active' : ''}`,
    onmousemove: () => { selected = i; render(); },
    onclick: () => run(c),
  },
  c.iconName ? icon(c.iconName, { size: 16 }) : el('span', { class: 'cmdk-item-dot' }),
  el('div', { class: 'cmdk-item-text' },
    el('div', {}, c.label),
    c.hint ? el('div', { class: 'cmdk-item-hint' }, c.hint) : null),
  c.group ? el('span', { class: 'badge badge-neutral' }, c.group) : null)));

  if (!filtered.length) {
    listEl.replaceChildren(el('div', { class: 'cmdk-empty' }, 'No matching commands'));
  }
}

function run(cmd) {
  close();
  cmd.run();
}

function close() {
  if (!overlay) return;
  overlay.remove();
  overlay = null;
  document.removeEventListener('keydown', onKeyInPalette, true);
}

function onKeyInPalette(e) {
  if (e.key === 'Escape') { e.preventDefault(); close(); return; }
  if (e.key === 'ArrowDown') { e.preventDefault(); selected = Math.min(selected + 1, filtered.length - 1); render(); return; }
  if (e.key === 'ArrowUp') { e.preventDefault(); selected = Math.max(selected - 1, 0); render(); return; }
  if (e.key === 'Enter') { e.preventDefault(); if (filtered[selected]) run(filtered[selected]); }
}

export function openCommandPalette() {
  if (overlay) return;
  selected = 0;
  inputEl = el('input', { class: 'cmdk-input', placeholder: 'Search pages and actions…', autocomplete: 'off' });
  listEl = el('div', { class: 'cmdk-list' });
  const panel = el('div', { class: 'cmdk-panel' },
    el('div', { class: 'cmdk-search-row' }, icon('search', { size: 16 }), inputEl),
    listEl);
  overlay = el('div', { class: 'cmdk-overlay' }, panel);
  overlay.addEventListener('mousedown', (e) => { if (e.target === overlay) close(); });
  inputEl.addEventListener('input', () => { selected = 0; render(); });
  document.body.appendChild(overlay);
  document.addEventListener('keydown', onKeyInPalette, true);
  render();
  setTimeout(() => inputEl.focus(), 10);
}
