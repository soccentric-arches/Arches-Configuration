// CAN peripheral configuration.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';
import { peripheralsOf, saveIo } from './hardware-utils.js';

export async function render(api, store) {
    const data = await api.get('/api/v1/io');
    const peripherals = peripheralsOf(data, 'can');
    const rows = peripherals.map(peripheral => {
        const existing = (data.config && data.config.can || []).find(item => item.name === peripheral.device) || {};
        return { peripheral, bitrate: Field({ label: 'Bitrate', type: 'number', value: existing.bitrate || 500000 }), fd: Toggle({ label: 'CAN FD', checked: existing.fd }), dataBitrate: Field({ label: 'FD data bitrate', type: 'number', value: existing.data_bitrate || 2000000 }), listenOnly: Toggle({ label: 'Listen only', checked: existing.listen_only }), upOnBoot: Toggle({ label: 'Up on boot', checked: existing.up_on_boot !== false }), existing };
    });
    const save = async () => { await saveIo(api, data, { can: rows.map(row => ({ name: row.peripheral.device, bitrate: Number(row.bitrate.get()) || 500000, fd: row.fd.get(), data_bitrate: Number(row.dataBitrate.get()) || 0, listen_only: row.listenOnly.get(), restart_ms: row.existing.restart_ms || 100, up_on_boot: row.upOnBoot.get() })) }); store.toast('CAN settings saved', 'success'); };
    const apply = async () => { await api.post('/api/v1/io/can/apply', {}); store.toast('CAN applied', 'success'); };

    return el('div', {}, el('div', { class: 'page-header' }, el('h1', {}, 'CAN'), el('p', {}, 'Configure CAN bitrate, FD mode, listen-only behavior, and startup.')),
        rows.length ? rows.map(row => Card({ title: row.peripheral.human_name || row.peripheral.device, subtitle: row.peripheral.device }, row.bitrate.wrap, row.fd.wrap, row.dataBitrate.wrap, row.listenOnly.wrap, row.upOnBoot.wrap)) : el('p', { class: 'muted' }, 'No CAN interfaces on this board.'),
        rows.length ? Card({ title: 'CAN actions' }, ActionList(ActionRow('Save settings', 'Persist CAN configuration.', Button({ onClick: save }, 'Save settings')), ActionRow('Apply CAN', 'Apply CAN configuration to the device.', Button({ variant: 'outline', onClick: apply }, 'Apply CAN')))) : null);
}
