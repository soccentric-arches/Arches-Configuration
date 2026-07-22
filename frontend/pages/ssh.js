// SSH daemon and serial console settings.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';

export async function render(api, store) {
    const systemCfg = await api.get('/api/v1/system');
    const sshd = Toggle({ label: 'SSH daemon enabled', checked: systemCfg.ssh && systemCfg.ssh.sshd_enabled });
    const getty = Toggle({ label: 'Serial getty enabled', checked: systemCfg.ssh && systemCfg.ssh.serial_getty_enabled });
    const authKeys = Field({ label: 'Authorized keys (one per line)', type: 'textarea', value: systemCfg.ssh && systemCfg.ssh.authorized_keys });

    return el('div', {},
        el('div', { class: 'page-header' }, el('h1', {}, 'SSH & Console'), el('p', {}, 'Control remote shell access and the serial console login.')),
        Card({
            title: 'SSH and console',
            subtitle: 'Settings are stored with the system configuration.',
            footer: ActionList(
                ActionRow('Apply to device', 'Apply the general system configuration to the running device.', Button({ variant: 'outline', onClick: async () => { await api.post('/api/v1/system/apply', {}); store.toast('System settings applied', 'success'); } }, 'Apply')),
                ActionRow('Save settings', 'Store SSH and console settings for the next apply.', Button({
                    onClick: async () => {
                        await api.put('/api/v1/system', { ...systemCfg, ssh: { sshd_enabled: sshd.get(), serial_getty_enabled: getty.get(), authorized_keys: authKeys.get() } });
                        store.toast('SSH settings saved', 'success');
                    }
                }, 'Save'))),
        }, el('div', { class: 'form-grid' }, el('div', { class: 'toggle-stack' }, sshd.wrap, getty.wrap), authKeys.wrap)));
}
