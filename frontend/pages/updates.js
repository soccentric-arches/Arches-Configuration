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
import { ActionList, ActionRow } from '../components/action-row.js';

export async function render(api, store) {
  const [slots, otaConfig, ota] = await Promise.all([
    api.get('/api/v1/updates/slots'),
    api.get('/api/v1/updates/ota'),
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
    footer: ActionList(
      ActionRow('Reboot to apply', 'Activate the staged update on the next boot. The connection will briefly drop.', Button({
        variant: 'outline',
        onClick: async () => {
          if (!await confirmDialog({ title: 'Reboot to apply?', description: 'The staged slot becomes active on next boot.' })) return;
          await api.post('/api/v1/updates/reboot-to-apply', {});
          store.showReconnect();
        },
      }, 'Reboot to apply')),
      ActionRow('Install bundle', 'Verify and stage the signed bundle in the inactive update slot.', Button({
        onClick: async () => {
          if (!path.get()) { store.toast('Enter a bundle path', 'error'); return; }
          const r = await api.post('/api/v1/updates/install', { bundle_path: path.get() });
          store.toast(`Staged: ${r.manifest && r.manifest.platform} ${r.manifest && r.manifest.version}. Reboot to apply.`, 'success');
        },
      }, 'Install bundle'))),
  }, path.wrap);

  // ---------- OTA ----------
  const otaServer = Field({
    label: 'Remote update server URL',
    value: otaConfig.server_url,
    placeholder: 'https://updates.example.com/check',
    help: 'The server should return JSON with update_available, and optionally version and platform.',
  });
  const otaEnabled = Toggle({ label: 'Enable remote updates', checked: otaConfig.enabled === true });
  const otaStatus = el('div', { class: 'field-help' });
  const checkOta = async () => {
    const result = await api.get('/api/v1/updates/ota/check');
    otaStatus.textContent = result.checked
      ? (result.update_available ? `Update available${result.version ? `: ${result.version}` : ''}.` : 'No update is currently available.')
      : (result.reason || 'No update server configured.');
  };
  const otaCard = Card({
    title: 'Remote updates',
    subtitle: 'Connect to an update server and check whether a compatible update is available.',
    footer: ActionList(
      ActionRow('Save server', 'Store the remote update server URL on the device.', Button({
        onClick: async () => {
          await api.put('/api/v1/updates/ota', { ...otaConfig, enabled: otaEnabled.get(), server_url: otaServer.get() || null });
          store.toast('Update server saved', 'success');
        }
      }, 'Save')),
      ActionRow('Check for updates', 'Ask the configured server for the latest update status.', Button({ variant: 'outline', onClick: checkOta }, 'Check now'))),
  }, otaEnabled.wrap, otaServer.wrap, otaStatus);

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'Updates'), el('p', {}, 'A/B slot management, OTA, and config backup.')),
    Card({ title: 'A/B slots', subtitle: 'The active and inactive update slots, versions, and confirmation state.' }, slotsTable),
    el('div', { class: 'card-grid' }, installCard, otaCard));
}
