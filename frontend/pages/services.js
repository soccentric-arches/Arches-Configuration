// Services & Apps page (spec §4.3): curated systemd unit list + start/stop/
// restart/enable, MQTT/cloud config, and schema-driven app settings (spec
// §3: adding a new app setting requires zero UI code — the descriptor from
// GET /schemas is rendered generically by schema-form.js).

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { Table } from '../components/table.js';
import { StatusDot } from '../components/badge.js';
import { EmptyState } from '../components/empty-state.js';
import { renderSchemaForm } from '../components/schema-form.js';

export async function render(api, store) {
  const [list, mqtt, schemas] = await Promise.all([
    api.get('/api/v1/services'),
    api.get('/api/v1/services/mqtt'),
    api.get('/api/v1/schemas').catch(() => []),
  ]);

  // ---------- Services table (with an expandable per-unit journal row) ----------
  let servicesTable;
  if ((list || []).length) {
    const { element, tbody } = Table({ columns: ['Unit', 'Active', 'Enabled', ''] }, []);
    for (const row of list) {
      const unit = row.unit;
      const st = row.status || {};
      const active = st.active;
      const state = active === 'active' ? 'ok' : active === 'failed' ? 'bad' : 'unknown';
      const enabledToggle = Toggle({
        checked: st.enabled === true,
        onChange: async (checked) => {
          await api.put(`/api/v1/services/${encodeURIComponent(unit)}/enabled`, { enabled: checked });
          store.toast(`${unit}: ${checked ? 'enabled' : 'disabled'} on boot`, 'success');
        },
      });
      const act = (verb) => Button({
        variant: 'link', size: 'sm',
        onClick: async () => { await api.post(`/api/v1/services/${encodeURIComponent(unit)}/${verb}`, {}); store.reload(); },
      }, verb[0].toUpperCase() + verb.slice(1));

      const journalPre = el('pre', { class: 'log-viewer', style: 'margin-top:8px' });
      const journalRow = el('tr', { hidden: true }, el('td', { colspan: '4' }, journalPre));
      const journalToggle = Button({
        variant: 'link', size: 'sm',
        onClick: async () => {
          journalRow.hidden = !journalRow.hidden;
          if (!journalRow.hidden) {
            const r = await api.get(`/api/v1/services/${encodeURIComponent(unit)}/journal`);
            journalPre.textContent = (r.tail || []).join('\n') || 'No recent log lines.';
          }
        },
      }, 'Journal');

      tbody.appendChild(el('tr', {},
        el('td', {}, unit),
        el('td', {}, el('span', { class: 'row' }, StatusDot(state), st.sub || active || '—')),
        el('td', {}, enabledToggle.wrap),
        el('td', {}, el('div', { class: 'table-actions' }, act('start'), act('stop'), act('restart'), journalToggle))));
      tbody.appendChild(journalRow);
    }
    servicesTable = element;
  } else {
    servicesTable = EmptyState({ title: 'No services in the allowlist', description: 'The board manifest defines which units are manageable here.' });
  }

  // ---------- MQTT / cloud ----------
  const host = Field({ label: 'Broker host', value: mqtt.host });
  const port = Field({ label: 'Port', type: 'number', value: mqtt.port });
  const user = Field({ label: 'Username', value: mqtt.username });
  const pass = Field({ label: 'Password', type: 'password', value: mqtt.password });
  const topic = Field({ label: 'Base topic', value: mqtt.base_topic });
  const tls = Toggle({ label: 'TLS', checked: mqtt.tls });

  const mqttCard = Card({
    title: 'MQTT / cloud',
    footer: [
      Button({
        variant: 'outline',
        onClick: async () => {
          const r = await api.post('/api/v1/services/mqtt/test', {});
          store.toast(r.reachable ? 'Broker reachable' : 'Broker unreachable', r.reachable ? 'success' : 'error');
        },
      }, 'Test connection'),
      Button({
        onClick: async () => {
          await api.put('/api/v1/services/mqtt', {
            host: host.get() || null, port: port.get() ? Number(port.get()) : null,
            tls: tls.get(), username: user.get() || null, password: pass.get() || null,
            base_topic: topic.get() || null, client_cert_id: mqtt.client_cert_id, client_id: mqtt.client_id,
          });
          store.toast('MQTT settings saved', 'success');
        },
      }, 'Save'),
    ],
  }, el('div', { class: 'form-grid' }, host.wrap, port.wrap, user.wrap, pass.wrap, topic.wrap, tls.wrap));

  // ---------- Schema-driven app settings (spec §3) ----------
  const appSettingsCards = await Promise.all((schemas || []).map(async (s) => {
    const values = await api.get(`/api/v1/apps/${encodeURIComponent(s.id)}/settings`).catch(() => ({}));
    const form = renderSchemaForm(s.descriptor, values);
    return Card({
      title: s.descriptor.title || s.id,
      footer: Button({
        onClick: async () => {
          if (!form.validate()) { store.toast('Fix the highlighted fields', 'error'); return; }
          await api.put(`/api/v1/apps/${encodeURIComponent(s.id)}/settings`, form.getValue());
          store.toast(`${s.descriptor.title || s.id} saved`, 'success');
        },
      }, 'Save'),
    }, form.element);
  }));

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'Services & Apps'), el('p', {}, 'Manage curated units and cloud/app integrations.')),
    Card({ title: 'Services' }, servicesTable),
    mqttCard,
    ...appSettingsCards);
}
