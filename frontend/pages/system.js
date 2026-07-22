// System settings page (spec §4.1): identity, time/NTP, SSH, and root password.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';

export async function render(api, store) {
  const cfg = await api.get('/api/v1/system');

  const hostname = Field({ label: 'Hostname', value: cfg.hostname, placeholder: 'e.g. arches-01' });
  const assetTag = Field({ label: 'Asset tag', value: cfg.asset_tag });
  const hardwareRevision = Field({ label: 'Hardware revision', value: cfg.hardware_revision, help: 'The physical board revision assigned to this device.' });
  const serialNumber = Field({ label: 'Serial number', value: cfg.serial_number, help: 'The device serial number used for inventory and service records.' });
  const identity = el('div', { class: 'form-grid' }, hostname.wrap, assetTag.wrap, hardwareRevision.wrap, serialNumber.wrap);

  const timezone = Field({ label: 'Timezone', value: cfg.time && cfg.time.timezone, placeholder: 'e.g. UTC, America/Denver' });
  const ntpEnabled = Toggle({ label: 'NTP enabled', checked: cfg.time && cfg.time.ntp_enabled });
  const manualDt = Field({ label: 'Manual datetime (air-gapped)', value: cfg.time && cfg.time.manual_datetime, help: 'Only used while NTP is disabled.' });
  const ntpServers = Field({ label: 'NTP servers (comma-separated)', value: (cfg.ntp_servers || []).join(', ') });
  const time = el('div', { class: 'form-grid' }, timezone.wrap, ntpEnabled.wrap, manualDt.wrap, ntpServers.wrap);

  const sshd = Toggle({ label: 'SSH daemon enabled', checked: cfg.ssh && cfg.ssh.sshd_enabled });
  const getty = Toggle({ label: 'Serial getty enabled', checked: cfg.ssh && cfg.ssh.serial_getty_enabled });
  const authKeys = Field({ label: 'Authorized keys (one per line)', type: 'textarea', value: cfg.ssh && cfg.ssh.authorized_keys });
  const ssh = el('div', { class: 'form-grid' }, sshd.wrap, getty.wrap, authKeys.wrap);

  async function save() {
    const body = {
      hostname: hostname.get() || null,
      asset_tag: assetTag.get() || null,
      hardware_revision: hardwareRevision.get() || null,
      serial_number: serialNumber.get() || null,
      time: {
        timezone: timezone.get() || null,
        ntp_enabled: ntpEnabled.get(),
        manual_datetime: manualDt.get() || null,
      },
      ssh: {
        sshd_enabled: sshd.get(),
        serial_getty_enabled: getty.get(),
        authorized_keys: authKeys.get(),
      },
      ntp_servers: ntpServers.get().split(',').map(s => s.trim()).filter(Boolean),
    };
    await api.put('/api/v1/system', body);
    store.toast('System settings saved', 'success');
  }
  async function apply() {
    await api.post('/api/v1/system/apply', {});
    store.toast('Applied (hostname + time)', 'success');
  }

  const identityCard = Card({
    title: 'Identity',
    subtitle: 'Set the name and asset tag used to identify this device.',
  }, identity);

  const timeCard = Card({
    title: 'Time and NTP',
    subtitle: 'Configure the device clock and time synchronization sources.',
  }, time);

  const sshCard = Card({
    title: 'SSH and console',
    subtitle: 'Control remote shell access and the serial console login.',
    footer: ActionList(
      ActionRow('Apply to device', 'Apply the hostname and time settings to the running device.', Button({ variant: 'outline', onClick: apply }, 'Apply')),
      ActionRow('Save settings', 'Store identity, time, NTP, and SSH settings for the next apply.', Button({ onClick: save }, 'Save'))),
  }, ssh);

  // ---------- Root password ----------
  const rp = Field({ label: 'New root password (min 8)', type: 'password' });
  const rootCard = Card({ title: 'Root password', subtitle: 'Change the Linux root password used for local administration.' }, rp.wrap,
    ActionList(ActionRow('Set root password', 'Replace the current root password with the new value.', Button({
      onClick: async () => {
        if (rp.get().length < 8) { store.toast('Password must be at least 8 characters', 'error'); return; }
        await api.post('/api/v1/system/root-password', { password: rp.get() });
        store.toast('Root password set', 'success');
        rp.set('');
      },
    }, 'Set password'))));

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'System settings'), el('p', {}, 'Manage device identity, time, SSH access, and root password.')),
    identityCard, timeCard, sshCard, rootCard);
}
