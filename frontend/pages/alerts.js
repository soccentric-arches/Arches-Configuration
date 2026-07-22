// Alerts page (spec §4.7): threshold rules with duration + history.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { Table } from '../components/table.js';
import { EmptyState } from '../components/empty-state.js';
import { ActionList, ActionRow } from '../components/action-row.js';

export async function render(api, store) {
  const data = await api.get('/api/v1/alerts');
  const rules = (data.rules || []).slice();
  const enabled = Toggle({ label: 'Enable alerting', checked: data.enabled === true });

  const tableHost = el('div');
  function renderRules() {
    const rows = rules.map((r, idx) => [
      r.id, r.metric, r.comparison, String(r.threshold), String(r.duration_secs || 0), (r.actions || []).join(', '),
      Button({ variant: 'link', size: 'sm', title: 'Remove this alert rule from the unsaved list', onClick: () => { rules.splice(idx, 1); renderRules(); } }, 'Delete'),
    ]);
    const content = rows.length
      ? Table({ columns: ['ID', 'Metric', 'Op', 'Threshold', 'Hold (s)', 'Actions', ''] }, rows).element
      : EmptyState({ title: 'No alert rules yet' });
    tableHost.replaceChildren(content);
  }
  renderRules();

  const id = Field({ label: 'ID' });
  const metric = Field({ label: 'Metric', value: 'temp' });
  const op = Field({ label: 'Comparison', type: 'select', value: '>', options: ['>', '<', '>=', '<=', '==', '!='].map(v => ({ value: v, label: v })) });
  const thr = Field({ label: 'Threshold', type: 'number', value: '85' });
  const dur = Field({ label: 'Hold (s)', type: 'number', value: '60' });

  const addCard = Card({ title: 'Add rule', subtitle: 'Define a metric threshold and how long it must hold before an alert fires.' },
    el('div', { class: 'field-row' }, id.wrap, metric.wrap, op.wrap, thr.wrap, dur.wrap),
    ActionList(ActionRow('Add alert rule', 'Add this threshold rule to the local list; save it below to activate it.', Button({
      variant: 'outline',
      onClick: () => {
        if (!id.get()) { store.toast('ID is required', 'error'); return; }
        rules.push({ id: id.get(), metric: metric.get(), comparison: op.get(), threshold: Number(thr.get()), duration_secs: Number(dur.get()) || 0, actions: ['ui'] });
        renderRules();
        id.set(''); thr.set('85'); dur.set('60');
      },
    }, 'Add to list'))));

  const rulesCard = Card({
    title: 'Alert rules', subtitle: 'Review the pending threshold rules before saving them to the device.', footer: ActionList(ActionRow('Save all rules', 'Persist the current alert rules and notification settings.', Button({
      onClick: async () => {
        await api.put('/api/v1/alerts', { enabled: enabled.get(), rules, webhooks: data.webhooks || [], mqtt_event_topic: data.mqtt_event_topic || null });
        store.toast('Alert rules saved', 'success');
      },
    }, 'Save all'))),
  }, enabled.wrap, tableHost);

  const historyItems = (data.history || []).slice(-20).reverse();
  const historyCard = Card({ title: 'Recent events', subtitle: 'The most recent alert transitions recorded by the daemon.' },
    historyItems.length
      ? el('div', { class: 'stack' }, ...historyItems.map(ev => el('div', { class: 'row muted', style: 'font-size:13px' },
        el('span', { class: 'mono' }, ev.ts), el('span', {}, ev.rule_id), el('span', {}, ev.state), el('span', {}, `val=${ev.value}`))))
      : EmptyState({ title: 'No events recorded yet' }));

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'Alarms'), el('p', {}, 'Threshold rules and alarm firing history.')),
    rulesCard, addCard, historyCard);
}
