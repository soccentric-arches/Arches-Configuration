// Page renderers for the nine functional sections (spec §4.0..§4.9).
// Each page is a function (api, store) => Promise<HTMLElement> mounted into
// #view. Forms are hand-mapped to the real backend config shapes; the generic
// SchemaForm renderer is used for schema-driven app settings (spec §3:
// adding an app setting = zero UI code). Polling/SSE pages clean up via
// store.registerTimer / store.registerCleanup.

(function (global) {
  'use strict';

  const { el } = global.SchemaForm;

  function card(title, ...children) {
    return el('div', { class: 'card' }, el('h3', {}, title), ...children);
  }
  function dot(state) {
    const cls = state === 'ok' ? 'dot-ok' : state === 'warn' ? 'dot-warn'
      : state === 'bad' ? 'dot-bad' : 'dot-unknown';
    return el('span', { class: `state-dot ${cls}` });
  }
  function btn(label, onclick, opts) {
    const b = el('button', { onclick }, label);
    if (opts && opts.link) b.className = 'link-btn';
    if (opts && opts.disabled) b.disabled = true;
    return b;
  }
  function input(type, value) {
    return el('input', { type, value: value == null ? '' : String(value) });
  }
  // A labelled scalar field; returns { wrap, get, set }.
  function field(labelText, type, value) {
    const i = input(type, value);
    const w = el('label', {}, labelText, i);
    return { wrap: w, get: () => i.value, set: (v) => { i.value = v == null ? '' : String(v); } };
  }
  function toggle(labelText, checked) {
    const c = el('input', { type: 'checkbox', checked: checked === true });
    return { wrap: el('label', {}, labelText, c), get: () => c.checked };
  }

  // ===========================================================================
  // System (§4.1): identity, time/NTP, ssh, power, root password.
  // ===========================================================================
  async function systemPage(api, store) {
    const cfg = await api.get('/api/v1/system');
    const root = el('div');

    const hostname = field('Hostname', 'text', cfg.hostname);
    const assetTag = field('Asset tag', 'text', cfg.asset_tag);
    const timezone = field('Timezone', 'text', cfg.time && cfg.time.timezone);
    const ntpEnabled = toggle('NTP enabled', cfg.time && cfg.time.ntp_enabled);
    const manualDt = field('Manual datetime (air-gapped)', 'text', cfg.time && cfg.time.manual_datetime);
    const ntpServers = field('NTP servers (comma-separated)', 'text', (cfg.ntp_servers || []).join(', '));
    const sshd = toggle('SSH daemon enabled', cfg.ssh && cfg.ssh.sshd_enabled);
    const getty = toggle('Serial getty enabled', cfg.ssh && cfg.ssh.serial_getty_enabled);
    const authKeys = field('Authorized keys (one per line)', 'text', cfg.ssh && cfg.ssh.authorized_keys);

    root.appendChild(card('Identity', hostname.wrap, assetTag.wrap));
    root.appendChild(card('Time', timezone.wrap, ntpEnabled.wrap, manualDt.wrap, ntpServers.wrap));
    root.appendChild(card('SSH / console', sshd.wrap, getty.wrap, authKeys.wrap));

    const save = btn('Save', async () => {
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
      store.toast('System settings saved');
    });
    const apply = btn('Apply to device', async () => {
      await api.post('/api/v1/system/apply', {});
      store.toast('Applied (hostname + time)');
    });
    root.appendChild(el('div', { class: 'row' }, save, apply));

    // Power controls + root password.
    root.appendChild(card('Power',
      btn('Reboot', async () => {
        await api.post('/api/v1/system/reboot', {});
        store.showReconnect();
      }),
      btn('Shutdown', async () => { await api.post('/api/v1/system/shutdown', {}); store.toast('Shutting down'); }),
      btn('Factory reset', async () => {
        const mode = confirm('Erase settings+users? Cancel = settings only') ? 'settings+users' : 'settings';
        if (!confirm(`Type RESET in the next prompt to erase (${mode})`)) return;
        const c = prompt('Type RESET to confirm'); if (c !== 'RESET') return;
        await api.post('/api/v1/system/factory-reset', { confirm: 'RESET', mode });
        store.showReconnect();
      }, { link: true })));

    const rp = field('New root password (min 8)', 'password');
    root.appendChild(card('Root password', rp.wrap,
      btn('Set root password', async () => {
        if (rp.get().length < 8) { store.toast('min 8 chars'); return; }
        await api.post('/api/v1/system/root-password', { password: rp.get() });
        store.toast('Root password set');
      })));
    return root;
  }

  // ===========================================================================
  // Networking (§4.2 flagship): interface editor + commit/confirm rollback.
  // ===========================================================================
  async function networkPage(api, store) {
    const { config, live } = await api.get('/api/v1/network');
    const root = el('div');

    const liveNames = (live || []).map(l => l.name);
    // Ensure every live interface has a config entry.
    const ifaces = (config.interfaces || []).slice();
    for (const n of liveNames) {
      if (!ifaces.some(i => i.name === n)) ifaces.push({ name: n, enabled: true, dhcp: true });
    }

    const ifaceRows = el('div', { class: 'form' });
    function renderIfaces() {
      ifaceRows.innerHTML = '';
      ifaces.forEach((iface, idx) => {
        const en = el('input', { type: 'checkbox', checked: iface.enabled });
        const dhcp = el('input', { type: 'checkbox', checked: iface.dhcp });
        const addr = el('input', { type: 'text', value: iface.address || '' });
        const prefix = el('input', { type: 'number', value: iface.prefix == null ? '' : String(iface.prefix) });
        const gw = el('input', { type: 'text', value: iface.gateway || '' });
        const openSvcs = el('input', { type: 'text', value: (iface.open_services || []).join(', ') });
        const sync = () => {
          ifaces[idx] = {
            name: iface.name, enabled: en.checked, dhcp: dhcp.checked,
            address: addr.value || null, prefix: prefix.value ? Number(prefix.value) : null,
            gateway: gw.value || null, mtu: iface.mtu, dns: iface.dns || [],
            open_services: openSvcs.value.split(',').map(s => s.trim()).filter(Boolean),
          };
        };
        for (const e of [en, dhcp, addr, prefix, gw, openSvcs]) e.addEventListener('change', sync);
        ifaceRows.appendChild(el('div', { class: 'card' },
          el('h3', {}, iface.name),
          el('label', {}, 'Enabled', en),
          el('label', {}, 'DHCP', dhcp),
          el('label', {}, 'Static address', addr),
          el('label', {}, 'Prefix', prefix),
          el('label', {}, 'Gateway', gw),
          el('label', {}, 'Open services (comma)', openSvcs)));
      });
    }
    renderIfaces();
    root.appendChild(card('Interfaces', ifaceRows));

    const dns = field('DNS servers (comma)', 'text', (config.dns_servers || []).join(', '));
    const window = field('Confirm window (s)', 'number', config.confirm_window_secs || 90);
    const apEn = toggle('AP provisioning mode', config.ap_mode && config.ap_mode.enabled);
    const apSsid = field('AP SSID', 'text', config.ap_mode && config.ap_mode.ssid);
    const apPass = field('AP passphrase', 'password', config.ap_mode && config.ap_mode.passphrase);
    root.appendChild(card('DNS + AP mode', dns.wrap, window.wrap, apEn.wrap, apSsid.wrap, apPass.wrap));

    root.appendChild(el('p', { class: 'muted' },
      'Applying is provisional. Reconnect on the new address and confirm within ' +
      'the window, or the change rolls back automatically.'));

    const apply = btn('Save & apply network settings', async () => {
      const body = {
        interfaces: ifaces,
        wifi: config.wifi || [],
        regulatory_domain: config.regulatory_domain || null,
        dns_servers: dns.get().split(',').map(s => s.trim()).filter(Boolean),
        firewall: config.firewall || { default_open: [] },
        confirm_window_secs: Number(window.get()) || 90,
        ap_mode: {
          enabled: apEn.get(),
          trigger: (config.ap_mode && config.ap_mode.trigger) || 'no-config',
          ssid: apSsid.get(), passphrase: apPass.get(),
        },
        vlan: config.vlan || [], bridges: config.bridges || [], cellular: config.cellular || [],
      };
      await api.put('/api/v1/network', body);     // persist
      const res = await api.post('/api/v1/network/apply', {}); // apply (may sever)
      if (res && res.provisional) store.showConfirm(res.confirm_window_secs || Number(window.get()) || 90);
      else store.toast('Network applied');
    });
    const rollback = btn('Roll back now', async () => {
      await api.post('/api/v1/network/rollback', {});
      store.hideConfirm();
      store.toast('Rolled back');
    }, { link: true });
    root.appendChild(el('div', { class: 'row' }, apply, rollback));
    return root;
  }

  // ===========================================================================
  // Services & Apps (§4.3): curated list + enable/start/stop/restart + MQTT.
  // ===========================================================================
  async function servicesPage(api, store) {
    const [list, mqtt] = await Promise.all([
      api.get('/api/v1/services'),
      api.get('/api/v1/services/mqtt'),
    ]);
    const root = el('div');

    const tbl = el('table');
    tbl.appendChild(el('thead', {}, el('tr', {},
      el('th', {}, 'Unit'), el('th', {}, 'Active'), el('th', {}, 'Enabled'), el('th', {}, ''))));
    const body = el('tbody');
    for (const row of list || []) {
      const unit = row.unit;
      const st = row.status || {};
      const active = st.active;
      const stateDot = active === 'active' ? 'ok' : active === 'failed' ? 'bad' : 'unknown';
      const enCb = el('input', { type: 'checkbox', checked: st.enabled === true });
      enCb.addEventListener('change', async () => {
        await api.put(`/api/v1/services/${encodeURIComponent(unit)}/enabled`, { enabled: enCb.checked });
        store.toast(`${unit}: ${enCb.checked ? 'enabled' : 'disabled'} on boot`);
      });
      body.appendChild(el('tr', {},
        el('td', {}, unit),
        el('td', {}, dot(stateDot), st.sub || active || ''),
        el('td', {}, enCb),
        el('td', {},
          btn('Start', async () => { await api.post(`/api/v1/services/${encodeURIComponent(unit)}/start`, {}); store.reload(); }, { link: true }),
          btn('Stop', async () => { await api.post(`/api/v1/services/${encodeURIComponent(unit)}/stop`, {}); store.reload(); }, { link: true }),
          btn('Restart', async () => { await api.post(`/api/v1/services/${encodeURIComponent(unit)}/restart`, {}); store.reload(); }, { link: true }))));
    }
    tbl.appendChild(body);
    root.appendChild(card('Services', tbl));

    // MQTT / cloud panel.
    const host = field('Broker host', 'text', mqtt.host);
    const port = field('Port', 'number', mqtt.port);
    const user = field('Username', 'text', mqtt.username);
    const pass = field('Password', 'password', mqtt.password);
    const topic = field('Base topic', 'text', mqtt.base_topic);
    const tls = toggle('TLS', mqtt.tls);
    root.appendChild(card('MQTT / cloud', host.wrap, port.wrap, user.wrap, pass.wrap, topic.wrap, tls.wrap,
      el('div', { class: 'row' },
        btn('Save', async () => {
          await api.put('/api/v1/services/mqtt', {
            host: host.get() || null, port: port.get() ? Number(port.get()) : null,
            tls: tls.get(), username: user.get() || null, password: pass.get() || null,
            base_topic: topic.get() || null, client_cert_id: mqtt.client_cert_id, client_id: mqtt.client_id,
          });
          store.toast('MQTT saved');
        }),
        btn('Test connection', async () => {
          const r = await api.post('/api/v1/services/mqtt/test', {});
          store.toast(r.reachable ? 'Broker reachable' : 'Broker unreachable');
        }, { link: true }))));
    return root;
  }

  // ===========================================================================
  // Updates (§4.4): RAUC A/B slots, install, reboot-to-apply, OTA, backup.
  // ===========================================================================
  async function updatesPage(api, store) {
    const [slots, ota] = await Promise.all([
      api.get('/api/v1/updates/slots'),
      api.get('/api/v1/updates/ota/check'),
    ]);
    const root = el('div');

    const stbl = el('table');
    stbl.appendChild(el('thead', {}, el('tr', {},
      el('th', {}, 'Slot'), el('th', {}, 'Version'), el('th', {}, 'Running'), el('th', {}, 'Confirmed'), el('th', {}, 'State'))));
    const sb = el('tbody');
    for (const s of slots || []) {
      sb.appendChild(el('tr', {},
        el('td', {}, s.slot), el('td', {}, s.version),
        el('td', {}, s.running ? '●' : ''),
        el('td', {}, s.confirmed ? 'yes' : 'no'),
        el('td', {}, dot(s.state === 'good' ? 'ok' : s.state === 'bad' ? 'bad' : 'unknown'), s.state)));
    }
    stbl.appendChild(sb);
    root.appendChild(card('A/B slots', stbl));

    // v1 install takes a server-staged bundle path (the multipart upload stage
    // is a Yocto recipe TODO; spec §7 states the assumption). Accept a path.
    const path = field('Bundle path (on device)', 'text', '');
    root.appendChild(card('Install bundle',
      el('p', { class: 'muted' }, 'Compatibility (platform) + signature are verified before writing the inactive slot.'),
      path.wrap,
      el('div', { class: 'row' },
        btn('Install', async () => {
          if (!path.get()) { store.toast('enter bundle path'); return; }
          const r = await api.post('/api/v1/updates/install', { bundle_path: path.get() });
          store.toast(`Staged: ${r.manifest && r.manifest.platform} ${r.manifest && r.manifest.version}. Reboot to apply.`);
        }),
        btn('Reboot to apply', async () => {
          await api.post('/api/v1/updates/reboot-to-apply', {});
          store.showReconnect();
        }, { link: true }))));

    root.appendChild(card('OTA check',
      el('p', { class: 'muted' }, ota.checked ? `Server: ${ota.server_url}` : 'No OTA server configured (v1: notify-only).')));

    root.appendChild(card('Backup / restore',
      el('div', { class: 'row' },
        btn('Download config (secrets redacted)', async () => {
          const r = await api.get('/api/v1/updates/backup');
          downloadJson('webconfig-backup.json', r);
        }),
        btn('Diagnostics bundle', async () => {
          const r = await api.get('/api/v1/updates/diagnostics');
          downloadJson('webconfig-diagnostics.json', r);
        }, { link: true }))));
    return root;
  }

  function downloadJson(name, obj) {
    const blob = new Blob([JSON.stringify(obj, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = el('a', { href: url, download: name });
    document.body.appendChild(a); a.click(); a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  // ===========================================================================
  // Monitoring (§4.5): polled vitals + manifest tiles.
  // ===========================================================================
  function monitoringPage(api, store) {
    const root = el('div');
    const tiles = el('div', { class: 'grid' });
    root.appendChild(card('Vitals', tiles));

    async function poll() {
      try {
        const v = await api.get('/api/v1/monitoring/vitals');
        const memPct = v.mem_total_kb ? ((1 - v.mem_available_kb / v.mem_total_kb) * 100) : null;
        const diskPct = (v.mounts || []).map(m => m.total_bytes ? ((1 - m.avail_bytes / m.total_bytes) * 100) : 0)
          .reduce((a, b) => Math.max(a, b), 0);
        const maxTemp = (v.temperatures || []).map(t => t.temp_millideg_c / 1000)
          .reduce((a, b) => Math.max(a, b), -Infinity);
        tiles.innerHTML = '';
        tiles.appendChild(tile('CPU load (1m)', v.load && v.load[0] != null ? v.load[0].toFixed(2) : '—'));
        tiles.appendChild(tile('Memory', memPct != null ? `${memPct.toFixed(0)}%` : '—'));
        tiles.appendChild(tile('Disk (max)', diskPct ? `${diskPct.toFixed(0)}%` : '—'));
        tiles.appendChild(tile('Uptime', v.uptime_secs ? `${Math.round(v.uptime_secs / 60)} min` : '—'));
        if (Number.isFinite(maxTemp)) tiles.appendChild(tile('Hottest', `${maxTemp.toFixed(0)}°C`));
        for (const n of v.net || []) {
          tiles.appendChild(tile(`${n.iface} rx`, fmtBytes(n.rx_bytes)));
        }
      } catch (_) { /* transient */ }
    }
    poll();
    const handle = setInterval(poll, 3000);
    store.registerTimer(handle);
    return root;
  }
  function tile(label, value) {
    return el('div', { class: 'card' }, el('h3', {}, label),
      el('div', { style: 'font-size:22px;font-weight:700' }, value));
  }
  function fmtBytes(b) {
    if (!b) return '0 B';
    const u = ['B', 'KB', 'MB', 'GB']; let i = 0; let n = b;
    while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
    return `${n.toFixed(1)} ${u[i]}`;
  }

  // ===========================================================================
  // Logs (§4.6): journal snapshot + audit log + log config.
  // ===========================================================================
  async function logsPage(api, store) {
    const root = el('div');
    const pre = el('pre', { style: 'max-height:50vh;overflow:auto;background:var(--surface-2);padding:12px;border-radius:8px;font-size:12px' });
    const unit = field('Unit', 'text', '');
    const priority = field('Priority (0-7)', 'text', '');
    const limit = field('Limit', 'number', '200');
    root.appendChild(card('System logs',
      el('div', { class: 'row' }, unit.wrap, priority.wrap, limit.wrap,
        btn('Load', loadLogs), btn('Clear', () => { pre.innerHTML = ''; }, { link: true })),
      pre));

    async function loadLogs() {
      const q = new URLSearchParams();
      if (unit.get()) q.set('unit', unit.get());
      if (priority.get()) q.set('priority', priority.get());
      if (limit.get()) q.set('limit', limit.get());
      const r = await api.get(`/api/v1/logs?${q.toString()}`);
      pre.textContent = (r.lines || []).join('\n');
    }
    loadLogs();

    // Audit log.
    const auditPre = el('pre', { style: 'max-height:30vh;overflow:auto;background:var(--surface-2);padding:12px;border-radius:8px;font-size:12px' });
    root.appendChild(card('Audit log', auditPre,
      btn('Refresh', async () => {
        const entries = await api.get('/api/v1/logs/audit');
        auditPre.textContent = (entries || []).map(e => `${e.ts}  ${e.user}  ${e.action}${e.detail ? '  ' + e.detail : ''}`).join('\n');
      }, { link: true })));
    const entries = await api.get('/api/v1/logs/audit');
    auditPre.textContent = (entries || []).map(e => `${e.ts}  ${e.user}  ${e.action}${e.detail ? '  ' + e.detail : ''}`).join('\n');
    return root;
  }

  // ===========================================================================
  // Alerts (§4.7): rules + history.
  // ===========================================================================
  async function alertsPage(api, store) {
    const data = await api.get('/api/v1/alerts');
    const root = el('div');
    const rules = (data.rules || []).slice();

    const body = el('tbody');
    function renderRules() {
      body.innerHTML = '';
      rules.forEach((r, idx) => {
        const del = btn('Delete', () => { rules.splice(idx, 1); renderRules(); }, { link: true });
        body.appendChild(el('tr', {},
          el('td', {}, r.id), el('td', {}, r.metric), el('td', {}, r.comparison),
          el('td', {}, String(r.threshold)), el('td', {}, String(r.duration_secs || 0)),
          el('td', {}, (r.actions || []).join(',')), el('td', {}, del)));
      });
    }
    const tbl = el('table');
    tbl.appendChild(el('thead', {}, el('tr', {},
      el('th', {}, 'ID'), el('th', {}, 'Metric'), el('th', {}, 'Op'), el('th', {}, 'Threshold'),
      el('th', {}, 'Hold (s)'), el('th', {}, 'Actions'), el('th', {}, ''))));
    tbl.appendChild(body);
    renderRules();
    root.appendChild(card('Alert rules', tbl));

    // Add-rule form.
    const id = field('ID', 'text', '');
    const metric = field('Metric', 'text', 'temp');
    const op = field('Comparison', 'text', '>');
    const thr = field('Threshold', 'number', '85');
    const dur = field('Hold (s)', 'number', '60');
    root.appendChild(card('Add rule', id.wrap, metric.wrap, op.wrap, thr.wrap, dur.wrap,
      btn('Add', () => {
        if (!id.get()) { store.toast('id required'); return; }
        rules.push({ id: id.get(), metric: metric.get(), comparison: op.get(), threshold: Number(thr.get()), duration_secs: Number(dur.get()) || 0, actions: ['ui'] });
        renderRules();
      }),
      btn('Save all', async () => {
        await api.put('/api/v1/alerts', { rules, webhooks: data.webhooks || [], mqtt_event_topic: data.mqtt_event_topic || null });
        store.toast('Alert rules saved');
      })));

    const hist = el('ul');
    for (const ev of (data.history || []).slice(-20).reverse()) {
      hist.appendChild(el('li', {}, `${ev.ts}  ${ev.rule_id}  ${ev.state}  val=${ev.value}`));
    }
    root.appendChild(card('Recent events', hist));
    return root;
  }

  // ===========================================================================
  // Security (§4.8): session/WireGuard config + users/RBAC + exposure/trust.
  // ===========================================================================
  async function securityPage(api, store) {
    const [sec, users, exposure, trust] = await Promise.all([
      api.get('/api/v1/security'),
      api.get('/api/v1/system/users'),
      api.get('/api/v1/security/exposure'),
      api.get('/api/v1/security/trust'),
    ]);
    const root = el('div');

    const idle = field('Session idle timeout (s)', 'number', sec.session_idle_timeout_secs || 900);
    const plainHttp = toggle('Allow plain HTTP (first-boot opt-in)', sec.allow_plain_http);
    const wgEn = toggle('WireGuard enabled', sec.wireguard && sec.wireguard.enabled);
    const wgPort = field('WireGuard port', 'number', sec.wireguard && sec.wireguard.port);
    root.appendChild(card('Security config', idle.wrap, plainHttp.wrap, wgEn.wrap, wgPort.wrap,
      el('div', { class: 'row' },
        btn('Save', async () => {
          await api.put('/api/v1/security', {
            session_idle_timeout_secs: Number(idle.get()) || 0,
            allow_plain_http: plainHttp.get(),
            wireguard: { enabled: wgEn.get(), port: Number(wgPort.get()) || 0, peers: (sec.wireguard && sec.wireguard.peers) || [] },
            certificates: sec.certificates || [],
          });
          store.toast('Security saved');
        }),
        btn('Apply WireGuard', async () => {
          await api.post('/api/v1/security/apply', {});
          store.toast('WireGuard applied');
        }, { link: true }))));

    // Users / RBAC.
    const utbl = el('table');
    utbl.appendChild(el('thead', {}, el('tr', {}, el('th', {}, 'User'), el('th', {}, 'Role'), el('th', {}, ''))));
    const ub = el('tbody');
    for (const u of users || []) {
      ub.appendChild(el('tr', {},
        el('td', {}, u.username), el('td', {}, u.role),
        el('td', {}, btn('Delete', async () => {
          await api.del(`/api/v1/system/users/${encodeURIComponent(u.username)}`);
          store.reload();
        }, { link: true }))));
    }
    utbl.appendChild(ub);
    root.appendChild(card('Users', utbl,
      btn('Add user', async () => {
        const username = prompt('Username?'); if (!username) return;
        const password = prompt('Password (min 8)?'); if (!password) return;
        const role = prompt('Role: admin/operator/viewer', 'viewer') || 'viewer';
        await api.post('/api/v1/system/users', { username, password, role });
        store.reload();
      })));

    // Exposure + trust (read-only).
    root.appendChild(card('Exposure',
      el('p', {}, `UI always reachable: ${exposure.ui_always_reachable ? 'yes' : 'no'}`),
      el('p', {}, `Plain HTTP: ${exposure.plain_http ? 'allowed' : 'blocked'}`),
      el('ul', {}, ...(exposure.per_interface || []).map(p => el('li', {}, `${p.interface}: ${(p.open_services || []).join(', ') || 'none'}`)))));
    root.appendChild(card('Trust status',
      el('p', {}, `Secure boot: ${trust.secure_boot ? 'on' : 'off'}`),
      el('p', {}, `TPM: ${trust.tpm_present ? 'present' : 'absent'}`),
      el('p', { class: 'muted' }, `Verity: ${trust.verity_status} · Encryption: ${trust.encryption_status}`)));
    return root;
  }

  // ===========================================================================
  // I/O (§4.9): manifest-driven peripherals.
  // ===========================================================================
  async function ioPage(api, store) {
    const data = await api.get('/api/v1/io');
    const root = el('div');
    const grid = el('div', { class: 'grid' });
    for (const p of data.manifest || []) {
      grid.appendChild(el('div', { class: 'card' },
        el('h3', {}, p.kind), el('div', {}, p.human_name || p.id),
        el('div', { class: 'muted' }, p.device)));
    }
    root.appendChild(card('Board peripherals', grid,
      el('p', { class: 'muted' }, 'All I/O is rendered from the board manifest — zero hardcoded I/O.')));
    return root;
  }

  const PAGES = {
    system: systemPage, network: networkPage, services: servicesPage,
    updates: updatesPage, monitoring: monitoringPage, logs: logsPage,
    alerts: alertsPage, security: securityPage, io: ioPage,
  };
  global.Pages = { PAGES };
})(window);