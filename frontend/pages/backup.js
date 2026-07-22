// Configuration backup, restore, and diagnostics.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { confirmDialog } from '../components/modal.js';
import { ActionList, ActionRow } from '../components/action-row.js';

function downloadJson(name, obj) {
    const blob = new Blob([JSON.stringify(obj, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = el('a', { href: url, download: name });
    document.body.appendChild(a); a.click(); a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function render(api, store) {
    const includeSecrets = Toggle({ label: 'Include secrets in download' });
    const restoreFile = el('input', { type: 'file', accept: 'application/json', class: 'input' });

    const card = Card({ title: 'Backup and restore', subtitle: 'Export configuration or diagnostics, or replace the current configuration from a backup.' },
        el('div', { class: 'field-row' },
            el('div', {}, el('div', { class: 'field-label' }, 'Backup contents'), el('div', { class: 'field-help' }, 'Secrets are excluded unless explicitly included.')),
            includeSecrets.wrap),
        ActionList(
            ActionRow('Download configuration', 'Export the current configuration for safekeeping or another device.', Button({
                variant: 'outline', onClick: async () => {
                    const r = await api.get(`/api/v1/updates/backup?include_secrets=${includeSecrets.get()}`);
                    downloadJson('webconfig-backup.json', r);
                }
            }, 'Download config')),
            ActionRow('Download diagnostics', 'Export diagnostic information to investigate a device issue.', Button({
                variant: 'outline', onClick: async () => {
                    const r = await api.get('/api/v1/updates/diagnostics');
                    downloadJson('webconfig-diagnostics.json', r);
                }
            }, 'Diagnostics'))),
        el('label', { class: 'field' }, el('span', { class: 'field-label' }, 'Restore from a downloaded backup'), restoreFile),
        ActionList(ActionRow('Restore configuration', 'Overwrite the current configuration with the selected backup after confirmation.', Button({
            variant: 'destructive-outline',
            onClick: async () => {
                const file = restoreFile.files && restoreFile.files[0];
                if (!file) { store.toast('Choose a backup file first', 'error'); return; }
                if (!await confirmDialog({ title: 'Restore config?', description: 'This overwrites the current configuration.', variant: 'destructive' })) return;
                try {
                    await api.post('/api/v1/updates/restore', JSON.parse(await file.text()));
                    store.toast('Config restored', 'success');
                } catch (e) { store.toast(e.message, 'error'); }
            },
        }, 'Restore'))));

    return el('div', {},
        el('div', { class: 'page-header' }, el('h1', {}, 'Backup & Restore'), el('p', {}, 'Protect, inspect, and restore the device configuration.')),
        card);
}
