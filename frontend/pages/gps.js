// GPS/GNSS/GLONASS time source configuration — satellite systems and
// receiver devices. Saving also applies to the device (one action, per the
// enable/apply contract used by the other time pages).

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';

// Every satellite system the page covers; the stored list selects which ones
// the receiver may use. Missing/empty stored list = all of them (receiver
// default), matching the backend's serde default.
const CONSTELLATIONS = [
    { id: 'gps', label: 'GPS' },
    { id: 'glonass', label: 'GLONASS' },
    { id: 'galileo', label: 'Galileo' },
    { id: 'beidou', label: 'BeiDou' },
];

export async function render(api, store) {
    const cfg = await api.get('/api/v1/system');
    const enabled = Toggle({ label: 'GPS/GNSS time enabled', checked: cfg.time && cfg.time.gps_enabled });
    const device = Field({
        label: 'GNSS receiver device',
        value: cfg.time && cfg.time.gps_device,
        placeholder: '/dev/ttyUSB0',
        help: 'Serial device used by the receiver or gpsd.',
    });
    const ppsDevice = Field({
        label: 'GNSS PPS device (optional)',
        value: cfg.time && cfg.time.gps_pps_device,
        placeholder: '/dev/pps0',
        help: 'Kernel PPS device for sub-second synchronization.',
    });

    // Constellation selection: checked when the stored list includes the id
    // (or when the list is empty = all enabled by default).
    const stored = (cfg.time && cfg.time.gps_constellations && cfg.time.gps_constellations.length)
        ? cfg.time.gps_constellations
        : CONSTELLATIONS.map((c) => c.id);
    const constellationToggles = CONSTELLATIONS.map((c) => Toggle({ label: c.label, checked: stored.includes(c.id) }));

    // One action, not two: persisting and applying are the same operation
    // from the operator's point of view — save the section, then apply the
    // clock appliers, then report once.
    const saveAndApply = async () => {
        await api.put('/api/v1/system', {
            ...cfg,
            time: {
                ...cfg.time,
                gps_enabled: enabled.get(),
                gps_device: device.get() || null,
                gps_pps_device: ppsDevice.get() || null,
                gps_constellations: CONSTELLATIONS
                    .filter((c, i) => constellationToggles[i].get())
                    .map((c) => c.id),
            },
        });
        await api.post('/api/v1/system/apply', {});
        store.toast('Satellite time settings saved and applied', 'success');
    };

    return el('div', { class: 'time-page' },
        el('div', { class: 'page-header' },
            el('h1', {}, 'GPS / GNSS / GLONASS'),
            el('p', {}, 'Use a multi-constellation satellite receiver as the device time source.')),
        Card({
            title: 'Satellite time synchronization',
            subtitle: 'Enable the constellations your receiver tracks; enable PPS when the receiver and kernel expose a precision pulse.',
            footer: ActionList(
                ActionRow('Save & apply to device', 'Store the constellation, receiver, and PPS settings and apply clock synchronization now.',
                    Button({ onClick: saveAndApply }, 'Save & apply')),
            ),
        },
            el('div', { class: 'toggle-stack' }, enabled.wrap, ...constellationToggles.map((t) => t.wrap)),
            device.wrap,
            ppsDevice.wrap),
    );
}