// Read-only data display — the value IS the point, so it comes first:
// value (prominent) → label (small uppercase) → supporting help (muted, last).
// The old pattern (label → help → data at the bottom) buried the data under
// the same-styled explanatory text; this component inverts that everywhere.

import { el } from '../core/dom.js';

/**
 * DataItem(value, label, help?) -> element
 *   value: string | number | Node (e.g. a Badge or code element)
 *   label: short name of the datum
 *   help:  optional supporting sentence, always last and muted
 */
export function DataItem(value, label, help) {
    const valueNode = value instanceof Node
        ? value
        : el('div', { class: 'data-item-value' }, (value == null || value === '') ? '—' : String(value));
    return el('div', { class: 'data-item' },
        valueNode,
        el('div', { class: 'data-item-label' }, label),
        help ? el('div', { class: 'data-item-help' }, help) : null);
}