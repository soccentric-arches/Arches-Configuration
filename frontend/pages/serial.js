// Serial peripheral configuration.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';
import { peripheralsOf, saveIo } from './hardware-utils.js';

export async function render(api, store) {
    const data = await api.get('/api/v1/io');
    const peripherals = peripheralsOf(data, 'serial');
    const rows = peripherals.map(peripheral => {
        const existing = (data.config && data.config.serial || []).find(item => item.device === peripheral.device) || {};
        const baud = Field({ label: 'Baud', type: 'number', value: existing.baud || 115200 });
        const mode = Field({ label: 'Mode', type: 'select', value: existing.mode || peripheral.caps?.modes?.[0] || 'rs232', options: (peripheral.caps?.modes || ['rs232']).map(value => ({ value, label: value.toUpperCase() })) });
        const owner = Field({ label: 'Owner', type: 'select', value: existing.owner || 'unassigned', options: ['unassigned', 'console', 'customer', 'modbus'].map(value => ({ value, label: value })) });
        return { peripheral, baud, mode, owner };
    });
    const save = async () => { await saveIo(api, data, { serial: rows.map(row => ({ device: row.peripheral.device, baud: Number(row.baud.get()) || 115200, mode: row.mode.get(), owner: row.owner.get() })) }); store.toast('Serial settings saved', 'success'); };

    return el('div', {}, el('div', { class: 'page-header' }, el('h1', {}, 'Serial Peripherals'), el('p', {}, 'Configure serial port speed, protocol mode, and ownership.')),
        rows.length ? rows.map(row => Card({ title: row.peripheral.human_name || row.peripheral.device, subtitle: row.peripheral.device }, row.baud.wrap, row.mode.wrap, row.owner.wrap)) : el('p', { class: 'muted' }, 'No serial peripherals on this board.'),
        rows.length ? Card({ title: 'Serial actions' }, ActionList(ActionRow('Save settings', 'Persist the serial peripheral configuration.', Button({ onClick: save }, 'Save settings')))) : null);
}
