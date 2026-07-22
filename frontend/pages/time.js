// Time and NTP settings page.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';

export async function render(api, store) {
    const cfg = await api.get('/api/v1/system');

    const timezone = Field({
        label: 'Timezone',
        value: cfg.time && cfg.time.timezone,
        placeholder: 'e.g. UTC, America/Denver',
        help: 'The local timezone used for displayed times and scheduled operations.',
    });
    const ntpEnabled = Toggle({
        label: 'NTP enabled',
        checked: cfg.time && cfg.time.ntp_enabled,
    });
    const manualDt = Field({
        label: 'Manual datetime (air-gapped)',
        value: cfg.time && cfg.time.manual_datetime,
        help: 'Only used while NTP is disabled and the device cannot reach a time server.',
    });
    const ntpServers = Field({
        label: 'NTP servers (comma-separated)',
        value: (cfg.ntp_servers || []).join(', '),
        help: 'Enter hostnames or addresses in the order they should be tried.',
    });

    async function save() {
        await api.put('/api/v1/system', {
            hostname: cfg.hostname,
            asset_tag: cfg.asset_tag,
            hardware_revision: cfg.hardware_revision,
            serial_number: cfg.serial_number,
            time: {
                timezone: timezone.get() || null,
                ntp_enabled: ntpEnabled.get(),
                manual_datetime: manualDt.get() || null,
            },
            ssh: cfg.ssh,
            ntp_servers: ntpServers.get().split(',').map(s => s.trim()).filter(Boolean),
        });
        store.toast('Time settings saved', 'success');
    }

    async function apply() {
        await api.post('/api/v1/system/apply', {});
        store.toast('Time settings applied', 'success');
    }

    const timeCard = Card({
        title: 'Time and NTP',
        subtitle: 'Keep the device clock accurate and choose how it synchronizes.',
        footer: ActionList(
            ActionRow('Apply to device', 'Apply the timezone and clock synchronization settings now.', Button({ variant: 'outline', onClick: apply }, 'Apply')),
            ActionRow('Save settings', 'Store the time and NTP settings for the next apply.', Button({ onClick: save }, 'Save'))),
    }, el('div', { class: 'form-grid' }, timezone.wrap, ntpEnabled.wrap, manualDt.wrap, ntpServers.wrap));

    return el('div', { class: 'time-page' },
        el('div', { class: 'page-header' },
            el('h1', {}, 'Time & NTP'),
            el('p', {}, 'Configure the device timezone, clock, and network time sources.')),
        timeCard);
}
