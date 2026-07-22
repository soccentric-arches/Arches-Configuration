// Logs page (spec §4.6): journal viewer, journald storage config, audit log,
// and a full journal export download.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';

export async function render(api, store) {
  const cfg = await api.get('/api/v1/logs/config');

  // ---------- Journal viewer ----------
  const unit = Field({ label: 'Unit', placeholder: 'e.g. webconfigd.service' });
  const priority = Field({ label: 'Priority (0-7)', placeholder: '0=emerg … 7=debug' });
  const limit = Field({ label: 'Limit', type: 'number', value: '200' });
  const pre = el('pre', { class: 'log-viewer' });

  async function loadLogs() {
    const q = new URLSearchParams();
    if (unit.get()) q.set('unit', unit.get());
    if (priority.get()) q.set('priority', priority.get());
    if (limit.get()) q.set('limit', limit.get());
    const r = await api.get(`/api/v1/logs?${q.toString()}`);
    pre.textContent = (r.lines || []).join('\n');
  }

  const viewerCard = Card({
    title: 'System logs',
    footer: [
      Button({
        variant: 'outline',
        onClick: async () => { const r = await api.get('/api/v1/logs/download'); downloadText('journal-export.log', r.export || ''); },
      }, 'Download full export'),
      Button({ variant: 'outline', onClick: () => { pre.textContent = ''; } }, 'Clear'),
      Button({ onClick: loadLogs }, 'Load'),
    ],
  }, el('div', { class: 'field-row' }, unit.wrap, priority.wrap, limit.wrap), pre);

  // ---------- Journald config ----------
  const storage = Field({
    label: 'Storage', type: 'select', value: cfg.storage,
    options: [{ value: 'volatile', label: 'Volatile (ramdisk)' }, { value: 'persistent', label: 'Persistent (eMMC)' }],
  });
  const maxSize = Field({ label: 'Max size when persistent (bytes)', type: 'number', value: cfg.max_size_bytes });
  const syslogEnabled = Toggle({ label: 'Remote syslog', checked: !!cfg.remote_syslog });
  const syslogHost = Field({ label: 'Host', value: cfg.remote_syslog && cfg.remote_syslog.host });
  const syslogPort = Field({ label: 'Port', type: 'number', value: (cfg.remote_syslog && cfg.remote_syslog.port) || 514 });
  const syslogProto = Field({ label: 'Protocol', type: 'select', value: (cfg.remote_syslog && cfg.remote_syslog.protocol) || 'udp', options: [{ value: 'udp', label: 'UDP' }, { value: 'tcp', label: 'TCP' }] });

  const configCard = Card({
    title: 'Journald configuration',
    footer: Button({
      onClick: async () => {
        await api.put('/api/v1/logs/config', {
          storage: storage.get(),
          max_size_bytes: Number(maxSize.get()) || 0,
          remote_syslog: syslogEnabled.get() ? { host: syslogHost.get(), port: Number(syslogPort.get()) || 514, protocol: syslogProto.get() } : null,
        });
        store.toast('Logging config applied', 'success');
      },
    }, 'Save & apply'),
  }, storage.wrap, maxSize.wrap, syslogEnabled.wrap,
    el('div', { class: 'field-row' }, syslogHost.wrap, syslogPort.wrap, syslogProto.wrap));

  // ---------- Audit log ----------
  const auditPre = el('pre', { class: 'log-viewer' });
  async function loadAudit() {
    const entries = await api.get('/api/v1/logs/audit');
    auditPre.textContent = (entries || []).map(e => `${e.ts}  ${e.user}  ${e.action}${e.detail ? '  ' + e.detail : ''}`).join('\n');
  }
  await loadAudit();
  const auditCard = Card({ title: 'Audit log', footer: Button({ variant: 'outline', onClick: loadAudit }, 'Refresh') }, auditPre);

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'Logs'), el('p', {}, 'Journal viewer, storage policy, and the audit trail.')),
    viewerCard, configCard, auditCard);
}

function downloadText(name, text) {
  const blob = new Blob([text], { type: 'text/plain' });
  const url = URL.createObjectURL(blob);
  const a = el('a', { href: url, download: name });
  document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
