// Base clock settings page: timezone and manual time for air-gapped systems.
// NTP, PTP, and GPS/GNSS have dedicated pages; this page owns the timezone and
// manual datetime used for air-gapped systems.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';

const US_TIMEZONES = [
    ['America/Adak', 'Adak, Alaska'],
    ['America/Anchorage', 'Anchorage, Alaska'],
    ['America/Boise', 'Boise, Idaho'],
    ['America/Chicago', 'Central Time'],
    ['America/Denver', 'Mountain Time'],
    ['America/Detroit', 'Detroit, Michigan'],
    ['America/Indiana/Indianapolis', 'Indianapolis, Indiana'],
    ['America/Indiana/Knox', 'Knox, Indiana'],
    ['America/Indiana/Marengo', 'Marengo, Indiana'],
    ['America/Indiana/Petersburg', 'Petersburg, Indiana'],
    ['America/Indiana/Tell_City', 'Tell City, Indiana'],
    ['America/Indiana/Vevay', 'Vevay, Indiana'],
    ['America/Indiana/Vincennes', 'Vincennes, Indiana'],
    ['America/Indiana/Winamac', 'Winamac, Indiana'],
    ['America/Juneau', 'Juneau, Alaska'],
    ['America/Kentucky/Louisville', 'Louisville, Kentucky'],
    ['America/Kentucky/Monticello', 'Monticello, Kentucky'],
    ['America/Los_Angeles', 'Pacific Time'],
    ['America/Menominee', 'Menominee, Michigan'],
    ['America/Metlakatla', 'Metlakatla, Alaska'],
    ['America/New_York', 'Eastern Time'],
    ['America/Nome', 'Nome, Alaska'],
    ['America/North_Dakota/Beulah', 'Beulah, North Dakota'],
    ['America/North_Dakota/Center', 'Center, North Dakota'],
    ['America/North_Dakota/New_Salem', 'New Salem, North Dakota'],
    ['America/Phoenix', 'Phoenix, Arizona'],
    ['America/Sitka', 'Sitka, Alaska'],
    ['America/Yakutat', 'Yakutat, Alaska'],
    ['Pacific/Honolulu', 'Honolulu, Hawaii'],
];

export async function render(api, store) {
    const cfg = await api.get('/api/v1/system');
    const t = cfg.time || {};
    const currentTimezone = t.timezone;
    const timezoneOptions = US_TIMEZONES.map(([value, label]) => ({ value, label }));
    if (currentTimezone && !timezoneOptions.some(option => option.value === currentTimezone)) {
        timezoneOptions.unshift({ value: currentTimezone, label: `${currentTimezone} (current)` });
    }

    const timezone = Field({
        label: 'Timezone',
        type: 'select',
        value: currentTimezone,
        options: timezoneOptions,
        help: 'Choose a US timezone for displayed times and scheduled operations.',
    });
    const manualDt = Field({
        label: 'Manual datetime (air-gapped)',
        value: t.manual_datetime,
        help: 'Only used while NTP is disabled and the device cannot reach a time server.',
    });

    async function save() {
        await api.put('/api/v1/system', {
            hostname: cfg.hostname,
            asset_tag: cfg.asset_tag,
            hardware_revision: cfg.hardware_revision,
            serial_number: cfg.serial_number,
            time: {
                ...t,
                timezone: timezone.get() || null,
                manual_datetime: manualDt.get() || null,
            },
            ssh: cfg.ssh,
            ntp_servers: cfg.ntp_servers || [],
        });
        store.toast('Time settings saved', 'success');
    }

    async function apply() {
        await api.post('/api/v1/system/apply', {});
        store.toast('Time settings applied', 'success');
    }

    const timeCard = Card({
        title: 'Time and synchronization',
        subtitle: 'Set the timezone or adjust the clock manually for air-gapped systems.',
        footer: ActionList(
            ActionRow('Apply to device', 'Apply the timezone and clock synchronization settings now.', Button({ variant: 'outline', onClick: apply }, 'Apply')),
            ActionRow('Save settings', 'Store the time and synchronization settings for the next apply.', Button({ onClick: save }, 'Save'))),
    }, el('div', { class: 'form-grid' }, timezone.wrap, manualDt.wrap));

    return el('div', { class: 'time-page' },
        el('div', { class: 'page-header' },
            el('h1', {}, 'Clock'),
            el('p', {}, 'Configure the timezone and manual clock value. NTP, PTP, and GPS/GNSS have dedicated pages.')),
        timeCard);
}