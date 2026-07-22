// Board LED control through manifest-declared GPIO outputs.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';
import { EmptyState } from '../components/empty-state.js';
import { peripheralsOf, saveIo } from './hardware-utils.js';

function isLed(peripheral) {
    const text = `${peripheral.id} ${peripheral.human_name}`.toLowerCase();
    return peripheral.kind === 'gpio' && (text.includes('led') || peripheral.caps?.function === 'led');
}

export async function render(api, store) {
    const data = await api.get('/api/v1/io');
    const leds = peripheralsOf(data, 'gpio').filter(isLed);
    const rows = leds.map(peripheral => {
        const existing = (data.config && data.config.gpio || []).find(item => item.name === peripheral.id) || {};
        return { peripheral, initial: Toggle({ label: 'On at boot', checked: existing.initial_state === true }), existing };
    });
    const save = async () => { await saveIo(api, data, { gpio: [...(data.config && data.config.gpio || []).filter(item => !rows.some(row => row.peripheral.id === item.name)), ...rows.map(row => ({ name: row.peripheral.id, direction: 'out', active_low: row.peripheral.caps?.active_low || false, initial_state: row.initial.get() }))] }); store.toast('Board LED settings saved', 'success'); };

    return el('div', {},
        el('div', { class: 'page-header' }, el('h1', {}, 'Board LEDs'), el('p', {}, 'Control status LEDs declared as GPIO outputs by the board manifest.')),
        rows.length ? rows.map(row => Card({ title: row.peripheral.human_name || row.peripheral.id, subtitle: row.peripheral.device }, row.initial.wrap, Button({ variant: 'outline', size: 'sm', onClick: async () => { const result = await api.post(`/api/v1/io/gpio/${encodeURIComponent(row.peripheral.id)}/toggle`, {}); store.toast(`${row.peripheral.id} → ${result.value}`, 'success'); } }, 'Toggle now'))) : EmptyState({ title: 'No board LEDs reported by this board' }),
        rows.length ? Card({ title: 'LED actions' }, ActionList(ActionRow('Save settings', 'Persist the startup state for each board LED.', Button({ onClick: save }, 'Save settings')))) : null);
}
