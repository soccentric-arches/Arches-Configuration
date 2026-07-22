// Companion MCU page: manifest-driven settings and signed firmware actions.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { EmptyState } from '../components/empty-state.js';
import { renderSchemaForm } from '../components/schema-form.js';
import { ActionList, ActionRow } from '../components/action-row.js';

export async function render(api, store) {
    const data = await api.get('/api/v1/io');
    const mcu = (data.manifest || []).find(peripheral => peripheral.kind === 'mcu');
    if (!mcu) {
        return el('div', {},
            el('div', { class: 'page-header' }, el('h1', {}, 'MCU'), el('p', {}, 'Companion microcontroller settings and firmware.')),
            EmptyState({ title: 'No companion MCU on this board' }));
    }

    const mcuData = await api.get('/api/v1/io/mcu').catch(() => ({ config: data.config && data.config.mcu }));
    const mcuCfg = mcuData.config || (data.config && data.config.mcu) || {};
    const enabled = Toggle({ label: 'Enable companion MCU', checked: mcuCfg.enabled !== false });
    const version = Field({ label: 'Firmware version' });
    const signed = Toggle({ label: 'Package is signed', checked: true });
    let schemaForm = null;
    let settings = el('p', { class: 'muted' }, 'No settings schema declared for this MCU.');
    const schemaId = mcu.caps && mcu.caps.settings_schema;
    if (schemaId) {
        const schemas = await api.get('/api/v1/schemas').catch(() => []);
        const descriptor = (schemas || []).find(item => item.id === schemaId);
        if (descriptor) {
            schemaForm = renderSchemaForm(descriptor.descriptor, mcuCfg.settings || {});
            settings = schemaForm.element;
        }
    }

    const firmwareCard = Card({
        title: mcu.human_name || 'Companion MCU',
        subtitle: `Current firmware: ${mcuCfg.firmware_version || 'unknown'} · ${mcu.device}`,
        footer: ActionList(ActionRow('Flash firmware', 'Stage signed firmware for the companion MCU after validating its version.', Button({
            onClick: async () => {
                if (!version.get()) { store.toast('Enter a firmware version', 'error'); return; }
                await api.post('/api/v1/io/mcu/firmware', { version: version.get(), signed: signed.get() });
                store.toast('Firmware staged', 'success');
            },
        }, 'Flash firmware'))),
    }, enabled.wrap, settings, el('div', { class: 'field-row' }, version.wrap, signed.wrap));

    const saveSettings = async () => {
        const config = data.config || {};
        await api.put('/api/v1/io', {
            serial: config.serial || [], can: config.can || [], gpio: config.gpio || [],
            mcu: { kind: mcuCfg.kind || 'stm32', enabled: enabled.get(), firmware_version: mcuCfg.firmware_version, settings: schemaForm ? schemaForm.getValue() : mcuCfg.settings || {} },
            board_leds: config.board_leds || {},
        });
        store.toast('MCU settings saved', 'success');
    };

    return el('div', {},
        el('div', { class: 'page-header' }, el('h1', {}, 'MCU'), el('p', {}, 'Configure the companion microcontroller and stage signed firmware.')),
        firmwareCard,
        Card({ title: 'MCU actions', subtitle: 'Persist settings separately from firmware staging.' }, ActionList(ActionRow('Save MCU settings', 'Persist the settings defined by the board manifest schema.', Button({ onClick: saveSettings }, 'Save settings')))));
}
