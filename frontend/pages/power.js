// Power controls: reboot, shutdown, and factory reset.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Button } from '../components/button.js';
import { confirmDialog, promptDialog } from '../components/modal.js';
import { ActionList, ActionRow } from '../components/action-row.js';

export function render(api, store) {
    const reboot = Button({
        variant: 'outline', icon: 'power',
        onClick: async () => {
            if (!await confirmDialog({
                title: 'Reboot device?',
                description: 'The device will restart. The WebConfig connection will return when it is ready.',
                confirmLabel: 'Reboot',
            })) return;
            await api.post('/api/v1/system/reboot', {});
            store.showReconnect();
        },
    }, 'Reboot');

    const shutdown = Button({
        variant: 'outline',
        onClick: async () => {
            if (!await confirmDialog({
                title: 'Shut down device?',
                description: 'The device will power off and will need to be started manually.',
                confirmLabel: 'Shut down',
                variant: 'destructive',
            })) return;
            await api.post('/api/v1/system/shutdown', {});
            store.toast('Shutting down');
        },
    }, 'Shutdown');

    const systemReset = Button({
        variant: 'outline',
        onClick: async () => {
            const typed = await promptDialog({
                title: 'Reset system settings?',
                label: 'Type RESET to return the device to default settings',
                confirmLabel: 'Reset system',
            });
            if (typed !== 'RESET') {
                store.toast('System reset cancelled');
                return;
            }
            await api.post('/api/v1/system/factory-reset', { confirm: 'RESET', mode: 'settings' });
            store.showReconnect();
        },
    }, 'System reset');

    const factoryReset = Button({
        variant: 'destructive-outline',
        onClick: async () => {
            if (!await confirmDialog({
                title: 'Factory reset device?',
                description: 'This erases all settings and user accounts and returns the device to its installed defaults.',
                confirmLabel: 'Continue',
                variant: 'destructive',
            })) return;
            const typed = await promptDialog({ title: 'Type RESET to confirm', label: 'Confirmation phrase' });
            if (typed !== 'RESET') {
                store.toast('Factory reset cancelled');
                return;
            }
            await api.post('/api/v1/system/factory-reset', { confirm: 'RESET', mode: 'settings+users' });
            store.showReconnect();
        },
    }, 'Factory reset');

    const powerCard = Card({
        title: 'Power controls',
        subtitle: 'Each action is isolated below. Read the description before proceeding.',
    }, ActionList(
        ActionRow('Reboot', 'Restart the operating system without changing configuration.', reboot),
        ActionRow('Shutdown', 'Power off the device. A person must start it again.', shutdown),
        ActionRow('System reset', 'Return configuration to default mode while preserving user accounts.', systemReset),
        ActionRow('Factory reset', 'Erase configuration and user accounts. This cannot be undone.', factoryReset)));

    return el('div', { class: 'power-page' },
        el('div', { class: 'page-header' },
            el('h1', {}, 'Power'),
            el('p', {}, 'Safely reboot, shut down, or reset the device.')),
        powerCard);
}