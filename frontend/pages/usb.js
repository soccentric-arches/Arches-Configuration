// USB peripheral inventory.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Badge } from '../components/badge.js';
import { EmptyState } from '../components/empty-state.js';
import { peripheralsOf } from './hardware-utils.js';

export async function render(api) {
    const peripherals = peripheralsOf(await api.get('/api/v1/io'), 'usb');
    const cards = peripherals.map(peripheral => Card({ title: peripheral.human_name || peripheral.id, subtitle: peripheral.device },
        Badge('available', 'success'),
        el('div', { class: 'row' }, el('span', {}, 'Capabilities'), el('span', { class: 'muted' }, Object.keys(peripheral.caps || {}).length ? Object.entries(peripheral.caps).map(([key, value]) => `${key}: ${value}`).join(', ') : 'Board-defined defaults'))));
    return el('div', {}, el('div', { class: 'page-header' }, el('h1', {}, 'USB'), el('p', {}, 'Inspect USB peripherals defined by the board manifest.')),
        cards.length ? el('div', { class: 'card-grid' }, ...cards) : EmptyState({ title: 'No USB peripherals on this board' }));
}
