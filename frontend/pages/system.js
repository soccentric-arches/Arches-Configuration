// System page (spec §4.1): identity, time/NTP, ssh, power, root password.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { Badge } from '../components/badge.js';
import { Tabs } from '../components/tabs.js';
import { confirmDialog, promptDialog } from '../components/modal.js';

function fmtUptime(secs) {
  if (!secs) return '—';
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return [d && `${d}d`, h && `${h}h`, `${m}m`].filter(Boolean).join(' ');
}

export async function render(api, store) {
  const [cfg, info] = await Promise.all([
    api.get('/api/v1/system'),
    api.get('/api/v1/system/info').catch(() => null),
  ]);

  const infoCard = info ? Card({ title: 'Device' },
    el('div', { class: 'field-row' },
      el('div', {}, el('div', { class: 'field-label' }, 'Platform'), Badge(info.platform, 'info')),
      el('div', {}, el('div', { class: 'field-label' }, 'Model'), el('div', {}, info.model || '—')),
      el('div', {}, el('div', { class: 'field-label' }, 'Board'), el('div', {}, info.board_name || '—')),
      el('div', {}, el('div', { class: 'field-label' }, 'Uptime'), el('div', {}, fmtUptime(info.uptime_secs))),
      el('div', {}, el('div', { class: 'field-label' }, 'RTC'), Badge(info.has_rtc ? 'present' : 'absent', info.has_rtc ? 'success' : 'neutral')))) : null;

  const hostname = Field({ label: 'Hostname', value: cfg.hostname, placeholder: 'e.g. arches-01' });
  const assetTag = Field({ label: 'Asset tag', value: cfg.asset_tag });
  const identity = el('div', { class: 'form-grid' }, hostname.wrap, assetTag.wrap);

  const timezone = Field({ label: 'Timezone', value: cfg.time && cfg.time.timezone, placeholder: 'e.g. UTC, America/Denver' });
  const ntpEnabled = Toggle({ label: 'NTP enabled', checked: cfg.time && cfg.time.ntp_enabled });
  const manualDt = Field({ label: 'Manual datetime (air-gapped)', value: cfg.time && cfg.time.manual_datetime, help: 'Only used while NTP is disabled.' });
  const ntpServers = Field({ label: 'NTP servers (comma-separated)', value: (cfg.ntp_servers || []).join(', ') });
  const time = el('div', { class: 'form-grid' }, timezone.wrap, ntpEnabled.wrap, manualDt.wrap, ntpServers.wrap);

  const sshd = Toggle({ label: 'SSH daemon enabled', checked: cfg.ssh && cfg.ssh.sshd_enabled });
  const getty = Toggle({ label: 'Serial getty enabled', checked: cfg.ssh && cfg.ssh.serial_getty_enabled });
  const authKeys = Field({ label: 'Authorized keys (one per line)', type: 'textarea', value: cfg.ssh && cfg.ssh.authorized_keys });
  const ssh = el('div', { class: 'form-grid' }, sshd.wrap, getty.wrap, authKeys.wrap);

  const tabs = Tabs([
    { id: 'identity', label: 'Identity', content: identity },
    { id: 'time', label: 'Time', content: time },
    { id: 'ssh', label: 'SSH / console', content: ssh },
  ]);

  async function save() {
    const body = {
      hostname: hostname.get() || null,
      asset_tag: assetTag.get() || null,
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

  const settingsCard = Card({
    title: 'System settings',
    subtitle: 'Hostname, time/NTP, and SSH access.',
    footer: [
      Button({ variant: 'outline', onClick: apply }, 'Apply to device'),
      Button({ onClick: save }, 'Save'),
    ],
  }, tabs);

  // ---------- Power ----------
  const powerCard = Card({ title: 'Power', subtitle: 'Reboot, shut down, or factory-reset this device.' },
    el('div', { class: 'row' },
      Button({
        variant: 'outline', icon: 'power',
        onClick: async () => {
          if (!await confirmDialog({ title: 'Reboot device?', description: 'The UI will disconnect until it comes back up.', confirmLabel: 'Reboot' })) return;
          await api.post('/api/v1/system/reboot', {});
          store.showReconnect();
        },
      }, 'Reboot'),
      Button({
        variant: 'outline',
        onClick: async () => {
          if (!await confirmDialog({ title: 'Shut down device?', confirmLabel: 'Shut down', variant: 'destructive' })) return;
          await api.post('/api/v1/system/shutdown', {});
          store.toast('Shutting down');
        },
      }, 'Shutdown'),
      Button({
        variant: 'destructive-outline',
        onClick: async () => {
          const wipeUsers = await confirmDialog({
            title: 'Factory reset',
            description: 'Erase settings and users, or just settings? This cannot be undone.',
            confirmLabel: 'Erase settings + users', cancelLabel: 'Settings only', variant: 'destructive',
          });
          const typed = await promptDialog({ title: 'Type RESET to confirm', label: 'Confirmation phrase' });
          if (typed !== 'RESET') { store.toast('Factory reset cancelled'); return; }
          await api.post('/api/v1/system/factory-reset', { confirm: 'RESET', mode: wipeUsers ? 'settings+users' : 'settings' });
          store.showReconnect();
        },
      }, 'Factory reset')));

  // ---------- Root password ----------
  const rp = Field({ label: 'New root password (min 8)', type: 'password' });
  const rootCard = Card({ title: 'Root password' }, rp.wrap,
    el('div', { class: 'form-actions' }, Button({
      onClick: async () => {
        if (rp.get().length < 8) { store.toast('Password must be at least 8 characters', 'error'); return; }
        await api.post('/api/v1/system/root-password', { password: rp.get() });
        store.toast('Root password set', 'success');
        rp.set('');
      },
    }, 'Set root password')));

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'System'), el('p', {}, 'Identity, time, SSH access, and power controls.')),
    infoCard, settingsCard, powerCard, rootCard);
}
