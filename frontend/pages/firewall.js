// Firewall and exposed service policy.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';

export async function render(api, store) {
    const { config } = await api.get('/api/v1/network');
    const services = Field({ label: 'Default-open services (comma-separated)', value: ((config.firewall && config.firewall.default_open) || []).join(', '), help: 'Services opened by default on supported interfaces.' });
    const ifaceFields = (config.interfaces || []).map(iface => {
        const field = Field({ label: iface.name, value: (iface.open_services || []).join(', '), help: 'Services exposed on this interface.' });
        field.name = iface.name;
        return field;
    });

    async function save() {
        const interfaces = (config.interfaces || []).map((iface, index) => ({ ...iface, open_services: ifaceFields[index].get().split(',').map(value => value.trim()).filter(Boolean) }));
        await api.put('/api/v1/network', { ...config, interfaces, firewall: { ...(config.firewall || {}), default_open: services.get().split(',').map(value => value.trim()).filter(Boolean) } });
        const result = await api.post('/api/v1/network/apply', {});
        if (result && result.provisional) store.showConfirm(result.confirm_window_secs || config.confirm_window_secs || 90);
        else store.toast('Firewall settings applied', 'success');
    }

    return el('div', {},
        el('div', { class: 'page-header' }, el('h1', {}, 'Firewall'), el('p', {}, 'Control which services are exposed by default and on each network interface.')),
        Card({ title: 'Service policy', subtitle: 'Changes are applied through the network confirmation workflow.', footer: ActionList(ActionRow('Save and apply', 'Write the firewall policy and apply it to the device.', Button({ onClick: save }, 'Apply settings'))) }, services.wrap, ...ifaceFields.map(field => field.wrap)));
}
