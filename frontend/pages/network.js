// Networking page (spec §4.2 flagship): interface editor + commit/confirm
// rollback. Applying is provisional — the caller must confirm within the
// window or the engine bridge auto-rolls-back (see core/store.js).

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';

export async function render(api, store) {
  const { config, live } = await api.get('/api/v1/network');

  const liveNames = (live || []).map(l => l.name);
  const ifaces = (config.interfaces || []).slice();
  for (const n of liveNames) {
    if (!ifaces.some(i => i.name === n)) ifaces.push({ name: n, enabled: true, dhcp: true });
  }

  const ifaceHost = el('div', { class: 'card-grid' });
  function renderIfaces() {
    ifaceHost.replaceChildren(...ifaces.map((iface, idx) => {
      const en = Toggle({ label: 'Enabled', checked: iface.enabled });
      const dhcp = Toggle({ label: 'DHCP', checked: iface.dhcp });
      const addr = Field({ label: 'Static address', value: iface.address, placeholder: '192.168.1.50' });
      const prefix = Field({ label: 'Prefix', type: 'number', value: iface.prefix });
      const gw = Field({ label: 'Gateway', value: iface.gateway });
      const openSvcs = Field({ label: 'Open services (comma)', value: (iface.open_services || []).join(', ') });
      const sync = () => {
        ifaces[idx] = {
          name: iface.name, enabled: en.get(), dhcp: dhcp.get(),
          address: addr.get() || null, prefix: prefix.get() ? Number(prefix.get()) : null,
          gateway: gw.get() || null, mtu: iface.mtu, dns: iface.dns || [],
          open_services: openSvcs.get().split(',').map(s => s.trim()).filter(Boolean),
        };
      };
      for (const f of [en.input, dhcp.input, addr.input, prefix.input, gw.input, openSvcs.input]) {
        f.addEventListener('change', sync);
      }
      return Card({ title: iface.name, class: 'iface-card' },
        en.wrap, dhcp.wrap, addr.wrap, prefix.wrap, gw.wrap, openSvcs.wrap);
    }));
  }
  renderIfaces();

  const dns = Field({ label: 'DNS servers (comma)', value: (config.dns_servers || []).join(', ') });
  const confirmWindow = Field({ label: 'Confirm window (s)', type: 'number', value: config.confirm_window_secs || 90 });
  const apEn = Toggle({ label: 'AP provisioning mode', checked: config.ap_mode && config.ap_mode.enabled });
  const apSsid = Field({ label: 'AP SSID', value: config.ap_mode && config.ap_mode.ssid });
  const apPass = Field({ label: 'AP passphrase', type: 'password', value: config.ap_mode && config.ap_mode.passphrase });

  async function apply() {
    const body = {
      interfaces: ifaces,
      wifi: config.wifi || [],
      regulatory_domain: config.regulatory_domain || null,
      dns_servers: dns.get().split(',').map(s => s.trim()).filter(Boolean),
      firewall: config.firewall || { default_open: [] },
      confirm_window_secs: Number(confirmWindow.get()) || 90,
      ap_mode: {
        enabled: apEn.get(),
        trigger: (config.ap_mode && config.ap_mode.trigger) || 'no-config',
        ssid: apSsid.get(), passphrase: apPass.get(),
      },
      vlan: config.vlan || [], bridges: config.bridges || [], cellular: config.cellular || [],
    };
    await api.put('/api/v1/network', body);
    const res = await api.post('/api/v1/network/apply', {});
    if (res && res.provisional) store.showConfirm(res.confirm_window_secs || Number(confirmWindow.get()) || 90);
    else store.toast('Network applied', 'success');
  }
  async function rollback() {
    await api.post('/api/v1/network/rollback', {});
    store.hideConfirm();
    store.toast('Rolled back');
  }

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'Networking'),
      el('p', {}, 'Applying is provisional — reconnect on the new address and confirm within the window, or the change rolls back automatically.')),
    Card({ title: 'Interfaces', subtitle: `${ifaces.length} detected` }, ifaceHost),
    Card({ title: 'DNS + AP mode' }, dns.wrap, confirmWindow.wrap, apEn.wrap, apSsid.wrap, apPass.wrap),
    el('div', { class: 'form-actions' },
      Button({ onClick: apply }, 'Save & apply network settings'),
      Button({ variant: 'outline', onClick: rollback }, 'Roll back now')));
}
