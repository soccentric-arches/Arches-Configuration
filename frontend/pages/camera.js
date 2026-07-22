// Camera page: board-manifest camera inventory and platform capabilities.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Badge } from '../components/badge.js';
import { EmptyState } from '../components/empty-state.js';

export async function render(api) {
    const data = await api.get('/api/v1/io');
    const cameras = (data.manifest || []).filter(peripheral => peripheral.kind === 'camera');
    const panel = await api.get('/api/v1/io/platform-panel').catch(() => ({}));

    const cameraCards = cameras.map(camera => Card({
        title: camera.human_name || camera.id,
        subtitle: camera.device,
    },
        el('div', { class: 'row' }, el('span', {}, 'Manifest status'), Badge('available', 'success')),
        el('div', { class: 'row' }, el('span', {}, 'Capabilities'), el('span', { class: 'muted' }, Object.keys(camera.caps || {}).length ? Object.entries(camera.caps).map(([key, value]) => `${key}: ${value}`).join(', ') : 'Board-defined defaults'))));

    const panelCameras = panel.cameras || [];
    const capabilityCard = panelCameras.length ? Card({ title: 'Platform camera inventory', subtitle: 'Additional board-level camera information reported by the platform panel.' },
        ...panelCameras.map(camera => el('div', { class: 'row' }, el('strong', {}, camera.human_name || camera.id), el('span', { class: 'muted' }, camera.device)))) : null;

    return el('div', {},
        el('div', { class: 'page-header' }, el('h1', {}, 'Camera'), el('p', {}, 'Inspect camera devices defined by the board manifest.')),
        cameras.length ? el('div', { class: 'card-grid' }, ...cameraCards) : EmptyState({ title: 'No cameras on this board' }),
        capabilityCard);
}
