// Local Updates page (spec §4.4): RAUC A/B slots and staged bundle install.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Button } from '../components/button.js';
import { Table } from '../components/table.js';
import { StatusDot } from '../components/badge.js';
import { confirmDialog } from '../components/modal.js';
import { ActionList, ActionRow } from '../components/action-row.js';

export async function render(api, store) {
  const slots = await api.get('/api/v1/updates/slots');

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

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'Updates'), el('p', {}, 'Manage local A/B slots and install signed bundles.')),
    Card({ title: 'A/B slots', subtitle: 'The active and inactive update slots, versions, and confirmation state.' }, slotsTable),
    installCard);
}
