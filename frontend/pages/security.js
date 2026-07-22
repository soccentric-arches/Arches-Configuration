// Security page (spec §4.8): session/WireGuard config, TLS CSR generation,
// certificate inventory, users/RBAC, and exposure/trust (read-only).

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { Table } from '../components/table.js';
import { Badge } from '../components/badge.js';
import { EmptyState } from '../components/empty-state.js';
import { confirmDialog, formDialog, promptDialog } from '../components/modal.js';

function downloadText(name, text) {
  const blob = new Blob([text], { type: 'text/plain' });
  const url = URL.createObjectURL(blob);
  const a = el('a', { href: url, download: name });
  document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export async function render(api, store) {
  const [sec, users, exposure, trust, wg, certs] = await Promise.all([
    api.get('/api/v1/security'),
    api.get('/api/v1/system/users'),
    api.get('/api/v1/security/exposure'),
    api.get('/api/v1/security/trust'),
    api.get('/api/v1/security/wireguard/pubkey').catch(() => ({ public_key: '', has_private_key: false })),
    api.get('/api/v1/security/certs').catch(() => []),
  ]);

  // ---------- Session + WireGuard ----------
  const idle = Field({ label: 'Session idle timeout (s)', type: 'number', value: sec.session_idle_timeout_secs || 900 });
  const plainHttp = Toggle({ label: 'Allow plain HTTP (first-boot opt-in)', checked: sec.allow_plain_http });
  const wgEn = Toggle({ label: 'WireGuard enabled', checked: sec.wireguard && sec.wireguard.enabled });
  const wgPort = Field({ label: 'WireGuard port', type: 'number', value: sec.wireguard && sec.wireguard.port });

  const configCard = Card({
    title: 'Security config',
    footer: [
      Button({ variant: 'outline', onClick: async () => { await api.post('/api/v1/security/apply', {}); store.toast('WireGuard applied', 'success'); } }, 'Apply WireGuard'),
      Button({
        onClick: async () => {
          await api.put('/api/v1/security', {
            session_idle_timeout_secs: Number(idle.get()) || 0,
            allow_plain_http: plainHttp.get(),
            wireguard: { enabled: wgEn.get(), port: Number(wgPort.get()) || 0, peers: (sec.wireguard && sec.wireguard.peers) || [] },
            certificates: sec.certificates || [],
          });
          store.toast('Security config saved', 'success');
        },
      }, 'Save'),
    ],
  }, idle.wrap, plainHttp.wrap, wgEn.wrap, wgPort.wrap,
    el('div', { class: 'row' }, el('span', { class: 'muted', style: 'font-size:13px' }, 'WireGuard public key:'),
      wg.public_key ? el('code', { class: 'mono' }, wg.public_key) : Badge(wg.has_private_key ? 'key present, no peer' : 'not generated yet', 'neutral')));

  // ---------- TLS: CSR generator + certificate inventory ----------
  const cn = Field({ label: 'Common name', placeholder: 'device.example.com' });
  const csrOutput = el('div', { hidden: true });
  const csrCard = Card({ title: 'Generate CSR', subtitle: 'For a customer CA to sign — no OpenSSL involved (rustls/rcgen).' },
    cn.wrap,
    el('div', { class: 'form-actions' }, Button({
      onClick: async () => {
        if (!cn.get()) { store.toast('Enter a common name', 'error'); return; }
        const r = await api.post('/api/v1/security/csr', { cn: cn.get() });
        csrOutput.hidden = false;
        csrOutput.replaceChildren(
          el('div', { class: 'field-help' }, 'Send the CSR to your CA; keep the private key on this device.'),
          el('div', { class: 'row' },
            Button({ variant: 'outline', size: 'sm', onClick: () => downloadText('device.csr', r.csr) }, 'Download CSR'),
            Button({ variant: 'outline', size: 'sm', onClick: () => downloadText('device.key', r.private_key) }, 'Download private key')));
      },
    }, 'Generate')),
    csrOutput);

  const certRows = (certs || []).map(c => [c.id, Badge(c.kind, 'neutral'), c.subject, c.not_after]);
  const certsCard = Card({ title: 'Certificates' },
    certRows.length ? Table({ columns: ['ID', 'Kind', 'Subject', 'Expires'] }, certRows).element : EmptyState({ title: 'No certificates on file' }));

  // ---------- Users / RBAC ----------
  const userRows = (users || []).map((u) => {
    const roleField = Field({
      type: 'select', value: u.role,
      options: ['admin', 'operator', 'viewer'].map(v => ({ value: v, label: v })),
    });
    roleField.input.style.width = 'auto';
    roleField.input.addEventListener('change', async () => {
      try {
        await api.put(`/api/v1/system/users/${encodeURIComponent(u.username)}/role`, { role: roleField.get() });
        store.toast(`${u.username} is now ${roleField.get()}`, 'success');
      } catch (err) {
        roleField.set(u.role);
        store.toast(err.message, 'error');
      }
    });
    return [
      u.username, roleField.input,
      el('div', { class: 'table-actions' },
        Button({
          variant: 'link', size: 'sm',
          onClick: async () => {
            const pw = await promptDialog({ title: `Reset password for ${u.username}`, label: 'New password (min 8)', type: 'password', confirmLabel: 'Set password' });
            if (pw == null) return;
            if (pw.length < 8) { store.toast('Password must be at least 8 characters', 'error'); return; }
            await api.put(`/api/v1/system/users/${encodeURIComponent(u.username)}/password`, { password: pw });
            store.toast('Password updated', 'success');
          },
        }, 'Reset password'),
        Button({
          variant: 'link', size: 'sm',
          onClick: async () => {
            if (!await confirmDialog({ title: `Delete user ${u.username}?`, variant: 'destructive' })) return;
            await api.del(`/api/v1/system/users/${encodeURIComponent(u.username)}`);
            store.reload();
          },
        }, 'Delete')),
    ];
  });
  const usersCard = Card({
    title: 'Users',
    footer: Button({
      onClick: async () => {
        const r = await formDialog({
          title: 'Add user', confirmLabel: 'Create',
          fields: [
            { key: 'username', label: 'Username' },
            { key: 'password', label: 'Password (min 8)', type: 'password' },
            { key: 'role', label: 'Role: admin / operator / viewer', placeholder: 'viewer' },
          ],
        });
        if (!r || !r.username || !r.password) return;
        await api.post('/api/v1/system/users', { username: r.username, password: r.password, role: r.role || 'viewer' });
        store.reload();
      },
    }, 'Add user'),
  }, Table({ columns: ['User', 'Role', ''] }, userRows).element);

  // ---------- Exposure + trust (read-only) ----------
  const exposureCard = Card({ title: 'Exposure' },
    el('div', { class: 'row' }, el('span', {}, 'UI always reachable:'), Badge(exposure.ui_always_reachable ? 'yes' : 'no', exposure.ui_always_reachable ? 'info' : 'neutral')),
    el('div', { class: 'row' }, el('span', {}, 'Plain HTTP:'), Badge(exposure.plain_http ? 'allowed' : 'blocked', exposure.plain_http ? 'warning' : 'success')),
    el('div', { class: 'stack' }, ...(exposure.per_interface || []).map(p =>
      el('div', { class: 'row', style: 'font-size:13px' }, el('strong', {}, p.interface + ':'), (p.open_services || []).join(', ') || 'none'))));

  const trustCard = Card({ title: 'Trust status' },
    el('div', { class: 'row' }, el('span', {}, 'Secure boot:'), Badge(trust.secure_boot ? 'on' : 'off', trust.secure_boot ? 'success' : 'destructive')),
    el('div', { class: 'row' }, el('span', {}, 'TPM:'), Badge(trust.tpm_present ? 'present' : 'absent', trust.tpm_present ? 'success' : 'neutral')),
    el('p', { class: 'muted', style: 'font-size:13px' }, `Verity: ${trust.verity_status} · Encryption: ${trust.encryption_status}`));

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'Security'), el('p', {}, 'Sessions, WireGuard, TLS, users, and exposure/trust posture.')),
    configCard, csrCard, certsCard, usersCard, exposureCard, trustCard);
}
