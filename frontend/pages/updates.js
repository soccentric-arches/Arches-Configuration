// Updates page (spec §4.4): RAUC A/B slots, install, reboot-to-apply, OTA
// check, and config backup/restore.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { Table } from '../components/table.js';
import { StatusDot } from '../components/badge.js';
import { confirmDialog } from '../components/modal.js';

function downloadJson(name, obj) {
  const blob = new Blob([JSON.stringify(obj, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = el('a', { href: url, download: name });
  document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export async function render(api, store) {
  const [slots, ota] = await Promise.all([
    api.get('/api/v1/updates/slots'),
    api.get('/api/v1/updates/ota/check'),
  ]);

  const slotRows = (slots || []).map(s => [
    s.slot, s.version,
    s.running ? el('span', {}, '●') : '',
    s.confirmed ? 'yes' : 'no',
    el('span', { class: 'row' }, StatusDot(s.state === 'good' ? 'ok' : s.state === 'bad' ? 'bad' : 'unknown'), s.state),
  ]);
  const slotsTable = Table({ columns: ['Slot', 'Version', 'Running', 'Confirmed', 'State'] }, slotRows).element;

  // ---------- Install ----------
  const path = Field({ label: 'Bundle path (on device)', placeholder: '/data/updates/bundle.raucb' });
  const installCard = Card({
    title: 'Install bundle',
    subtitle: 'Compatibility (platform) + signature are verified before writing the inactive slot.',
    footer: [
      Button({
        variant: 'outline',
        onClick: async () => {
          if (!await confirmDialog({ title: 'Reboot to apply?', description: 'The staged slot becomes active on next boot.' })) return;
          await api.post('/api/v1/updates/reboot-to-apply', {});
          store.showReconnect();
        },
      }, 'Reboot to apply'),
      Button({
        onClick: async () => {
          if (!path.get()) { store.toast('Enter a bundle path', 'error'); return; }
          const r = await api.post('/api/v1/updates/install', { bundle_path: path.get() });
          store.toast(`Staged: ${r.manifest && r.manifest.platform} ${r.manifest && r.manifest.version}. Reboot to apply.`, 'success');
        },
      }, 'Install'),
    ],
  }, path.wrap);

  // ---------- OTA ----------
  const otaCard = Card({ title: 'OTA check' },
    el('p', { class: 'muted' }, ota.checked ? `Server: ${ota.server_url}` : 'No OTA server configured (v1: notify-only).'));

  // ---------- Backup / restore ----------
  const includeSecrets = Toggle({ label: 'Include secrets in download' });
  const restoreFile = el('input', { type: 'file', accept: 'application/json', class: 'input' });
  const backupCard = Card({ title: 'Backup / restore' },
    el('div', { class: 'row' },
      Button({
        variant: 'outline',
        onClick: async () => {
          const r = await api.get(`/api/v1/updates/backup?include_secrets=${includeSecrets.get()}`);
          downloadJson('webconfig-backup.json', r);
        },
      }, 'Download config'),
      includeSecrets.wrap),
    el('div', { class: 'row' },
      Button({
        variant: 'outline',
        onClick: async () => {
          const r = await api.get('/api/v1/updates/diagnostics');
          downloadJson('webconfig-diagnostics.json', r);
        },
      }, 'Diagnostics bundle')),
    el('label', { class: 'field' },
      el('span', { class: 'field-label' }, 'Restore from a downloaded backup'),
      restoreFile,
      el('div', { class: 'form-actions' }, Button({
        variant: 'destructive-outline',
        onClick: async () => {
          const file = restoreFile.files && restoreFile.files[0];
          if (!file) { store.toast('Choose a backup file first', 'error'); return; }
          if (!await confirmDialog({ title: 'Restore config?', description: 'This overwrites the current configuration.', variant: 'destructive' })) return;
          try {
            const text = await file.text();
            await api.post('/api/v1/updates/restore', JSON.parse(text));
            store.toast('Config restored', 'success');
          } catch (e) { store.toast(e.message, 'error'); }
        },
      }, 'Restore'))));

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'Updates'), el('p', {}, 'A/B slot management, OTA, and config backup.')),
    Card({ title: 'A/B slots' }, slotsTable),
    installCard, otaCard, backupCard);
}
