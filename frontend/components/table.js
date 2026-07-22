import { el } from '../core/dom.js';

/**
 * Table({ columns: ['Unit', 'Active', ...] }, rows) where each row is an
 * array of cell nodes/strings in column order. Returns { element, tbody }
 * so callers can append/replace rows after the initial render.
 */
export function Table({ columns = [] } = {}, rows = []) {
  const thead = el('thead', {}, el('tr', {}, ...columns.map(c => el('th', {}, c))));
  const tbody = el('tbody', {}, ...rows.map(r => el('tr', {}, ...r.map(c => el('td', {}, c)))));
  const table = el('table', { class: 'table' }, thead, tbody);
  return { element: el('div', { class: 'table-wrap' }, table), tbody, table };
}

export function row(...cells) {
  return el('tr', {}, ...cells.map(c => el('td', {}, c)));
}
