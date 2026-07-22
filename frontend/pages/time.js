// Time and NTP settings page.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
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
    const currentTimezone = cfg.time && cfg.time.timezone;
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
    const ntpEnabled = Toggle({
        label: 'NTP enabled',
        checked: cfg.time && cfg.time.ntp_enabled,
    });
    const ptpEnabled = Toggle({
        label: 'PTP enabled',
        checked: cfg.time && cfg.time.ptp_enabled,
    });
    const ptpInterface = Field({
        label: 'PTP network interface',
        value: cfg.time && cfg.time.ptp_interface,
        placeholder: 'eth0',
        help: 'Interface connected to the PTP grandmaster. NTP is disabled while PTP is enabled.',
    });
    const ptpDomain = Field({
        label: 'PTP domain',
        type: 'number',
        value: cfg.time && cfg.time.ptp_domain,
        help: 'PTP domain number, normally 0.',
    });
    const gpsEnabled = Toggle({
        label: 'GPS/GNSS time enabled',
        checked: cfg.time && cfg.time.gps_enabled,
    });
    const gpsDevice = Field({
        label: 'GPS/GNSS device',
        value: cfg.time && cfg.time.gps_device,
        placeholder: '/dev/ttyUSB0',
        help: 'Serial device used by gpsd for the receiver time and position lock.',
    });
    const gpsPpsDevice = Field({
        label: 'GPS PPS device (optional)',
        value: cfg.time && cfg.time.gps_pps_device,
        placeholder: '/dev/pps0',
        help: 'Kernel PPS device for sub-second synchronization when the receiver exposes PPS.',
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
                ptp_enabled: ptpEnabled.get(),
                ptp_interface: ptpInterface.get() || null,
                ptp_domain: ptpDomain.get() ? Number(ptpDomain.get()) : null,
                gps_enabled: gpsEnabled.get(),
                gps_device: gpsDevice.get() || null,
                gps_pps_device: gpsPpsDevice.get() || null,
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
        title: 'Time and synchronization',
        subtitle: 'Keep the device clock accurate with NTP, PTP, or a GPS/GNSS lock.',
        footer: ActionList(
            ActionRow('Apply to device', 'Apply the timezone and clock synchronization settings now.', Button({ variant: 'outline', onClick: apply }, 'Apply')),
            ActionRow('Save settings', 'Store the time and NTP settings for the next apply.', Button({ onClick: save }, 'Save'))),
    }, el('div', { class: 'form-grid' }, timezone.wrap, el('div', { class: 'toggle-stack' }, ntpEnabled.wrap, ptpEnabled.wrap, gpsEnabled.wrap), ptpInterface.wrap, ptpDomain.wrap, gpsDevice.wrap, gpsPpsDevice.wrap, manualDt.wrap, ntpServers.wrap));

    return el('div', { class: 'time-page' },
        el('div', { class: 'page-header' },
            el('h1', {}, 'Time & Sync'),
            el('p', {}, 'Configure the device timezone, clock, NTP, PTP, and GPS/GNSS time sources.')),
        timeCard);
}
