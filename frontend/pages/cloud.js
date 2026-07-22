// Cloud connectivity page: MQTT broker configuration and connection testing.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';

export async function render(api, store) {
    const mqtt = await api.get('/api/v1/services/mqtt');
    const host = Field({ label: 'Broker host', value: mqtt.host, placeholder: 'mqtt.example.com', help: 'DNS name or IP address of the MQTT broker.' });
    const port = Field({ label: 'Port', type: 'number', value: mqtt.port || 1883, help: 'TCP port exposed by the broker; TLS commonly uses 8883.' });
    const user = Field({ label: 'Username', value: mqtt.username, help: 'Optional broker account used for authentication.' });
    const pass = Field({ label: 'Password', type: 'password', value: mqtt.password, help: 'Optional broker password; it is stored as a secret.' });
    const topic = Field({ label: 'Base topic', value: mqtt.base_topic, placeholder: 'devices/my-device', help: 'Topic prefix used for this device’s telemetry and events.' });
    const tls = Toggle({ label: 'Use TLS encryption', checked: mqtt.tls });

    const cloudCard = Card({
        title: 'MQTT cloud connection',
        subtitle: 'Connect this device to a cloud broker for telemetry and events.',
        footer: ActionList(
            ActionRow('Test connection', 'Check whether the configured broker is reachable from this device.', Button({
                variant: 'outline',
                onClick: async () => {
                    const result = await api.post('/api/v1/services/mqtt/test', {});
                    store.toast(result.reachable ? 'Broker reachable' : 'Broker unreachable', result.reachable ? 'success' : 'error');
                },
            }, 'Test connection')),
            ActionRow('Save cloud settings', 'Store the broker, credentials, topic, and TLS settings.', Button({
                onClick: async () => {
                    await api.put('/api/v1/services/mqtt', {
                        host: host.get() || null,
                        port: port.get() ? Number(port.get()) : null,
                        tls: tls.get(),
                        username: user.get() || null,
                        password: pass.get() || null,
                        base_topic: topic.get() || null,
                        client_cert_id: mqtt.client_cert_id,
                        client_id: mqtt.client_id,
                    });
                    store.toast('Cloud settings saved', 'success');
                },
            }, 'Save settings'))),
    }, el('div', { class: 'form-grid' }, host.wrap, port.wrap, user.wrap, pass.wrap, topic.wrap, tls.wrap));

    return el('div', {},
        el('div', { class: 'page-header' },
            el('h1', {}, 'Cloud'),
            el('p', {}, 'Configure MQTT connectivity for cloud telemetry and device events.')),
        cloudCard);
}