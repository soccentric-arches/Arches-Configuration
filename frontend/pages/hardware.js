// Hardware inventory overview.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Badge } from '../components/badge.js';
import { EmptyState } from '../components/empty-state.js';

export async function render(api) {
  const data = await api.get('/api/v1/io');
  const peripherals = (data.manifest || []).filter(peripheral => peripheral.kind !== 'mcu' && peripheral.kind !== 'camera');
  const cards = peripherals.map(peripheral => Card({ title: peripheral.human_name || peripheral.id, subtitle: peripheral.device }, Badge(peripheral.kind, 'neutral'), el('div', { class: 'muted' }, 'Configure this peripheral from its dedicated Hardware page.')));
  return el('div', {}, el('div', { class: 'page-header' }, el('h1', {}, 'Hardware'), el('p', {}, 'Board hardware inventory and dedicated peripheral configuration pages.')),
    cards.length ? el('div', { class: 'card-grid' }, ...cards) : EmptyState({ title: 'No configurable peripherals on this board' }));
}
