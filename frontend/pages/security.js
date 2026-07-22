// Security page (spec §4.8): session/WireGuard config, TLS CSR generation,
// certificate inventory and exposure/trust (read-only).

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { Table } from '../components/table.js';
import { Badge } from '../components/badge.js';
import { EmptyState } from '../components/empty-state.js';
import { promptDialog } from '../components/modal.js';
import { ActionList, ActionRow } from '../components/action-row.js';

function downloadText(name, text) {
  const blob = new Blob([text], { type: 'text/plain' });
  const url = URL.createObjectURL(blob);
  const a = el('a', { href: url, download: name });
  document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export async function render(api, store) {
  const [sec, exposure, trust, wg, certs] = await Promise.all([
    api.get('/api/v1/security'),
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
    subtitle: 'Session policy, plain HTTP access, and WireGuard settings.',
    footer: ActionList(
      ActionRow('Apply WireGuard', 'Apply the configured WireGuard interface and peers to the device.', Button({ variant: 'outline', onClick: async () => { await api.post('/api/v1/security/apply', {}); store.toast('WireGuard applied', 'success'); } }, 'Apply')),
      ActionRow('Save security settings', 'Store session, HTTP, WireGuard, and certificate settings.', Button({
        onClick: async () => {
          await api.put('/api/v1/security', {
            session_idle_timeout_secs: Number(idle.get()) || 0,
            allow_plain_http: plainHttp.get(),
            wireguard: { enabled: wgEn.get(), port: Number(wgPort.get()) || 0, peers: (sec.wireguard && sec.wireguard.peers) || [] },
            certificates: sec.certificates || [],
          });
          store.toast('Security config saved', 'success');
        },
      }, 'Save')),
  }, idle.wrap, plainHttp.wrap, wgEn.wrap, wgPort.wrap,
    el('div', { class: 'row' }, el('span', { class: 'muted', style: 'font-size:13px' }, 'WireGuard public key:'),
      wg.public_key ? el('code', { class: 'mono' }, wg.public_key) : Badge(wg.has_private_key ? 'key present, no peer' : 'not generated yet', 'neutral')));

  // ---------- TLS: CSR generator + certificate inventory ----------
  const cn = Field({ label: 'Common name', placeholder: 'device.example.com' });
  const csrOutput = el('div', { hidden: true });
  const csrCard = Card({ title: 'Generate CSR', subtitle: 'For a customer CA to sign — no OpenSSL involved (rustls/rcgen).' },
    cn.wrap,
    ActionList(ActionRow('Generate CSR', 'Create a certificate signing request and private key for the supplied common name.', Button({
      onClick: async () => {
        if (!cn.get()) { store.toast('Enter a common name', 'error'); return; }
        const r = await api.post('/api/v1/security/csr', { cn: cn.get() });
        csrOutput.hidden = false;
        csrOutput.replaceChildren(
          el('div', { class: 'field-help' }, 'Send the CSR to your CA; keep the private key on this device.'),
          el('div', { class: 'row' },
            Button({ variant: 'outline', size: 'sm', title: 'Download the certificate signing request', onClick: () => downloadText('device.csr', r.csr) }, 'Download CSR'),
            Button({ variant: 'outline', size: 'sm', title: 'Download the private key and keep it protected', onClick: () => downloadText('device.key', r.private_key) }, 'Download private key')));
      },
    }, 'Generate'))),
    csrOutput);

  const certRows = (certs || []).map(c => [c.id, Badge(c.kind, 'neutral'), c.subject, c.not_after]);
  const certsCard = Card({ title: 'Certificates', subtitle: 'Certificates currently known to the device and their expiry dates.' },
    certRows.length ? Table({ columns: ['ID', 'Kind', 'Subject', 'Expires'] }, certRows).element : EmptyState({ title: 'No certificates on file' }));

  // ---------- Exposure + trust (read-only) ----------
  const exposureCard = Card({ title: 'Exposure', subtitle: 'Effective UI reachability and open services by interface.' },
    el('div', { class: 'field-row' },
      el('div', {}, el('div', { class: 'field-label' }, 'UI reachability'), el('div', { class: 'field-help' }, 'The WebConfig interface remains reachable for local recovery.'), Badge(exposure.ui_always_reachable ? 'yes' : 'no', exposure.ui_always_reachable ? 'info' : 'neutral')),
      el('div', {}, el('div', { class: 'field-label' }, 'Plain HTTP'), el('div', { class: 'field-help' }, 'Whether unencrypted HTTP access is permitted.'), Badge(exposure.plain_http ? 'allowed' : 'blocked', exposure.plain_http ? 'warning' : 'success'))),
    el('div', { class: 'stack' }, ...(exposure.per_interface || []).map(p =>
      el('div', { class: 'row', style: 'font-size:13px' }, el('strong', {}, p.interface + ':'), (p.open_services || []).join(', ') || 'none'))));

  const trustCard = Card({ title: 'Trust status', subtitle: 'Secure-boot, TPM, verity, and encryption posture reported by the device.' },
    el('div', { class: 'field-row' },
      el('div', {}, el('div', { class: 'field-label' }, 'Secure boot'), el('div', { class: 'field-help' }, 'Firmware verification state reported by the platform.'), Badge(trust.secure_boot ? 'on' : 'off', trust.secure_boot ? 'success' : 'destructive')),
      el('div', {}, el('div', { class: 'field-label' }, 'TPM'), el('div', { class: 'field-help' }, 'Whether a trusted platform module is available.'), Badge(trust.tpm_present ? 'present' : 'absent', trust.tpm_present ? 'success' : 'neutral')),
      el('div', {}, el('div', { class: 'field-label' }, 'Verity and encryption'), el('div', { class: 'field-help' }, 'Read-only integrity and storage protection states.'), el('div', {}, `${trust.verity_status} · ${trust.encryption_status}`))));

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'Security'), el('p', {}, 'Sessions, WireGuard, TLS, and exposure/trust posture.')),
    configCard,
    el('div', { class: 'card-grid' }, csrCard, certsCard, exposureCard, trustCard));
}
