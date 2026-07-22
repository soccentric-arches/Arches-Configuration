import { el } from '../core/dom.js';

/**
 * Tabs([{ id, label, content }], { initial }) -> Element
 * `content` is a pre-built node; all panels are rendered up front and
 * shown/hidden on click (cheap for a local config UI, no lazy mount needed).
 */
export function Tabs(tabs, { initial } = {}) {
  const activeId = { id: initial || (tabs[0] && tabs[0].id) };
  const buttons = new Map();
  const panels = new Map();

  const list = el('div', { class: 'tabs-list' });
  const panelHost = el('div', {});

  for (const t of tabs) {
    const btn = el('button', {
      type: 'button',
      class: `tab-btn${t.id === activeId.id ? ' active' : ''}`,
      onclick: () => select(t.id),
    }, t.label);
    buttons.set(t.id, btn);
    list.appendChild(btn);
    const panel = el('div', { hidden: t.id !== activeId.id }, t.content);
    panels.set(t.id, panel);
    panelHost.appendChild(panel);
  }

  function select(id) {
    activeId.id = id;
    for (const [tid, btn] of buttons) btn.classList.toggle('active', tid === id);
    for (const [tid, panel] of panels) panel.hidden = tid !== id;
  }

  return el('div', {}, list, panelHost);
}
