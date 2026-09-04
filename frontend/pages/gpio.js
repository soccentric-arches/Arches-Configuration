// GPIO peripheral configuration.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';
import { peripheralsOf, saveIo, wireEntryGate } from './hardware-utils.js';

export async function render(api, store) {
    const data = await api.get('/api/v1/io');
    const peripherals = peripheralsOf(data, 'gpio');
    const subsystemEnabled = Toggle({ label: 'Enable GPIO subsystem', checked: data.config && data.config.gpio_enabled !== false });
    const rows = peripherals.map(peripheral => {
        const existing = (data.config && data.config.gpio || []).find(item => item.name === peripheral.id) || {};
        return { peripheral, existing, enabled: Toggle({ label: 'Enable GPIO pin', checked: existing.enabled === true }), initial: Toggle({ label: 'Initial state (on boot)', checked: existing.initial_state === true }), direction: peripheral.caps?.direction || existing.direction || 'out', activeLow: peripheral.caps?.active_low || false };
    });
    // Enable-before-configure: the subsystem toggle unlocks the pins; each
    // pin's enable toggle unlocks its startup state.
    wireEntryGate({ subsystem: subsystemEnabled, rows, controls: row => [row.initial] });
    const save = async () => {
        // Disabled AND never-configured pins are omitted (nothing to store).
        const gpio = rows
            .filter(row => row.enabled.get() || row.existing.name != null)
            .map(row => ({ name: row.peripheral.id, enabled: row.enabled.get(), direction: row.direction, active_low: row.activeLow, initial_state: row.direction === 'out' ? row.initial.get() : null }));
        await saveIo(api, data, { gpio_enabled: subsystemEnabled.get(), gpio });
        store.toast('GPIO settings saved', 'success');
    };

    return el('div', {}, el('div', { class: 'page-header' }, el('h1', {}, 'GPIO'), el('p', {}, 'Configure GPIO startup state and toggle output pins when supported.')),
        Card({ title: 'GPIO subsystem', subtitle: 'Enable GPIO before configuring individual pins.' }, subsystemEnabled.wrap),
        rows.length ? rows.map(row => Card({ title: row.peripheral.human_name || row.peripheral.id, subtitle: `${row.peripheral.device} · ${row.direction}` }, el('div', { class: 'toggle-stack' }, row.enabled.wrap, row.direction === 'out' ? row.initial.wrap : null), row.direction !== 'out' ? el('p', { class: 'muted' }, 'Input pin') : null, row.direction === 'out' ? Button({ variant: 'outline', size: 'sm', onClick: async () => { const result = await api.post(`/api/v1/io/gpio/${encodeURIComponent(row.peripheral.id)}/toggle`, {}); store.toast(`${row.peripheral.id} → ${result.value}`, 'success'); } }, 'Toggle now') : null)) : el('p', { class: 'muted' }, 'No GPIO pins on this board.'),
        rows.length ? Card({ title: 'GPIO actions' }, ActionList(ActionRow('Save settings', 'Persist GPIO startup settings.', Button({ onClick: save }, 'Save settings')))) : null);
}
