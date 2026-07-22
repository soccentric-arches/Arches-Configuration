// I/O & Peripherals page (spec §4.9): everything is rendered from the board
// manifest — zero hardcoded I/O. Serial/CAN/GPIO/MCU config is edited here
// and saved as one PUT (the engine stores it as a single IoConfig section);
// CAN apply, GPIO toggle, and I2C scan are immediate, separate actions.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { Badge } from '../components/badge.js';
import { EmptyState } from '../components/empty-state.js';
import { renderSchemaForm } from '../components/schema-form.js';

function byKind(peripherals, kind) { return (peripherals || []).filter(p => p.kind === kind); }

export async function render(api, store) {
  const data = await api.get('/api/v1/io');
  const peripherals = data.manifest || [];
  const cfg = data.config || {};

  // ---------- Overview grid ----------
  const overview = peripherals.length
    ? el('div', { class: 'card-grid' }, ...peripherals.map(p => el('div', { class: 'card peripheral-card' },
      el('div', { class: 'card-body' },
        Badge(p.kind, 'neutral'),
        el('h4', { style: 'margin-top:8px' }, p.human_name || p.id),
        el('div', { class: 'muted', style: 'font-size:13px' }, p.device)))))
    : EmptyState({ title: 'No peripherals in the board manifest' });

  // ---------- Serial ----------
  const serialPeripherals = byKind(peripherals, 'serial');
  const serialRows = {};
  const serialSection = serialPeripherals.length ? el('div', { class: 'card-grid' }, ...serialPeripherals.map((p) => {
    const existing = (cfg.serial || []).find(s => s.device === p.device) || {};
    const baud = Field({ label: 'Baud', type: 'number', value: existing.baud || 115200 });
    const mode = Field({ label: 'Mode', type: 'select', value: existing.mode || (p.caps && p.caps.modes && p.caps.modes[0]) || 'rs232', options: (p.caps && p.caps.modes || ['rs232']).map(m => ({ value: m, label: m.toUpperCase() })) });
    const owner = Field({ label: 'Owner', type: 'select', value: existing.owner || 'unassigned', options: ['unassigned', 'console', 'customer', 'modbus'].map(v => ({ value: v, label: v })) });
    serialRows[p.device] = { baud, mode, owner };
    return el('div', { class: 'card' }, el('div', { class: 'card-header' }, el('h4', {}, p.human_name || p.device)),
      el('div', { class: 'card-body' }, baud.wrap, mode.wrap, owner.wrap));
  })) : EmptyState({ title: 'No serial ports on this board' });

  // ---------- CAN ----------
  const canPeripherals = byKind(peripherals, 'can');
  const canRows = {};
  const canSection = canPeripherals.length ? el('div', { class: 'card-grid' }, ...canPeripherals.map((p) => {
    const existing = (cfg.can || []).find(c => c.name === p.device) || {};
    const bitrate = Field({ label: 'Bitrate', type: 'number', value: existing.bitrate || 500000 });
    const fd = Toggle({ label: 'CAN FD', checked: existing.fd });
    const dataBitrate = Field({ label: 'FD data bitrate', type: 'number', value: existing.data_bitrate || 2000000 });
    const listenOnly = Toggle({ label: 'Listen only', checked: existing.listen_only });
    const upOnBoot = Toggle({ label: 'Up on boot', checked: existing.up_on_boot !== false });
    canRows[p.device] = { bitrate, fd, dataBitrate, listenOnly, upOnBoot };
    return el('div', { class: 'card' }, el('div', { class: 'card-header' }, el('h4', {}, p.human_name || p.device)),
      el('div', { class: 'card-body' }, bitrate.wrap, fd.wrap, dataBitrate.wrap, listenOnly.wrap, upOnBoot.wrap));
  })) : EmptyState({ title: 'No CAN interfaces on this board' });
  const canFooter = canPeripherals.length ? Button({
    variant: 'outline',
    onClick: async () => { await api.post('/api/v1/io/can/apply', {}); store.toast('CAN applied', 'success'); },
  }, 'Apply CAN') : null;

  // ---------- GPIO ----------
  const gpioPeripherals = byKind(peripherals, 'gpio');
  const gpioRows = {};
  const gpioSection = gpioPeripherals.length ? el('div', { class: 'card-grid' }, ...gpioPeripherals.map((p) => {
    const existing = (cfg.gpio || []).find(g => g.name === p.id) || {};
    const direction = (p.caps && p.caps.direction) || existing.direction || 'out';
    const initial = Toggle({ label: 'Initial state (on boot)', checked: existing.initial_state === true });
    gpioRows[p.id] = { direction, activeLow: (p.caps && p.caps.active_low) || false, initial };
    return el('div', { class: 'card' },
      el('div', { class: 'card-header' }, el('h4', {}, p.human_name || p.id), Badge(direction, 'neutral')),
      el('div', { class: 'card-body' },
        direction === 'out' ? initial.wrap : el('p', { class: 'muted' }, 'Input pin'),
        direction === 'out' ? Button({
          variant: 'outline', size: 'sm',
          onClick: async () => { const r = await api.post(`/api/v1/io/gpio/${encodeURIComponent(p.id)}/toggle`, {}); store.toast(`${p.id} → ${r.value}`, 'success'); },
        }, 'Toggle now') : null));
  })) : EmptyState({ title: 'No GPIO pins on this board' });

  // ---------- I2C ----------
  const i2cPeripherals = byKind(peripherals, 'i2c');
  const i2cSection = i2cPeripherals.length ? el('div', { class: 'card-grid' }, ...i2cPeripherals.map((p) => {
    const result = el('div', { class: 'muted', style: 'font-size:13px;margin-top:8px' });
    return el('div', { class: 'card' }, el('div', { class: 'card-header' }, el('h4', {}, p.human_name || p.id)),
      el('div', { class: 'card-body' },
        Button({
          variant: 'outline', size: 'sm',
          onClick: async () => {
            const r = await api.post(`/api/v1/io/i2c/scan?bus=${encodeURIComponent(p.id)}`, {});
            result.textContent = r.addresses && r.addresses.length ? `Found: ${r.addresses.map(a => '0x' + a.toString(16)).join(', ')}` : 'No devices found';
          },
        }, 'Scan bus'),
        result));
  })) : EmptyState({ title: 'No I2C buses on this board' });

  // ---------- MCU ----------
  const mcuPeripheral = byKind(peripherals, 'mcu')[0];
  let mcuSection = null;
  let mcuSchemaForm = null;
  if (mcuPeripheral) {
    const mcuData = await api.get('/api/v1/io/mcu').catch(() => ({ config: cfg.mcu }));
    const mcuCfg = mcuData.config || cfg.mcu || {};
    const version = Field({ label: 'Firmware version', value: '' });
    const signed = Toggle({ label: 'Package is signed', checked: true });

    let settingsNode = el('p', { class: 'muted' }, 'No settings schema declared for this MCU.');
    const schemaId = mcuPeripheral.caps && mcuPeripheral.caps.settings_schema;
    if (schemaId) {
      try {
        const schemas = await api.get('/api/v1/schemas');
        const descriptor = (schemas || []).find(s => s.id === schemaId);
        if (descriptor) {
          mcuSchemaForm = renderSchemaForm(descriptor.descriptor, mcuCfg.settings || {});
          settingsNode = mcuSchemaForm.element;
        }
      } catch (_) { /* schema optional */ }
    }

    mcuSection = Card({
      title: mcuPeripheral.human_name || 'Companion MCU',
      subtitle: `Current firmware: ${mcuCfg.firmware_version || 'unknown'}`,
      footer: Button({
        onClick: async () => {
          if (!version.get()) { store.toast('Enter a firmware version', 'error'); return; }
          await api.post('/api/v1/io/mcu/firmware', { version: version.get(), signed: signed.get() });
          store.toast('Firmware staged', 'success');
        },
      }, 'Flash firmware'),
    }, settingsNode, el('div', { class: 'field-row' }, version.wrap, signed.wrap));
  }

  // ---------- Platform-specific panel ----------
  const panel = await api.get('/api/v1/io/platform-panel').catch(() => ({}));
  const panelEntries = Object.entries(panel).filter(([, v]) => v != null && (!Array.isArray(v) || v.length));
  const panelSection = panelEntries.length ? Card({ title: 'Platform panel' },
    ...panelEntries.map(([k, v]) => el('div', { class: 'row' },
      el('strong', {}, k + ':'),
      el('span', { class: 'muted' }, Array.isArray(v) ? (v.map(x => (typeof x === 'object' ? (x.human_name || x.id || JSON.stringify(x)) : x)).join(', ') || 'none') : String(v))))) : null;

  async function save() {
    const serial = serialPeripherals.map(p => ({ device: p.device, baud: Number(serialRows[p.device].baud.get()) || 115200, mode: serialRows[p.device].mode.get(), owner: serialRows[p.device].owner.get() }));
    const can = canPeripherals.map(p => ({
      name: p.device, bitrate: Number(canRows[p.device].bitrate.get()) || 500000, fd: canRows[p.device].fd.get(),
      data_bitrate: Number(canRows[p.device].dataBitrate.get()) || 0, listen_only: canRows[p.device].listenOnly.get(),
      restart_ms: (cfg.can || []).find(c => c.name === p.device)?.restart_ms || 100, up_on_boot: canRows[p.device].upOnBoot.get(),
    }));
    const gpio = gpioPeripherals.map(p => ({
      name: p.id, direction: gpioRows[p.id].direction, active_low: gpioRows[p.id].activeLow,
      initial_state: gpioRows[p.id].direction === 'out' ? gpioRows[p.id].initial.get() : null,
    }));
    const mcu = mcuPeripheral ? {
      kind: mcuPeripheral.caps && mcuPeripheral.caps.protocol === 'soccentric-mcu' ? 'stm32' : (cfg.mcu && cfg.mcu.kind) || 'stm32',
      firmware_version: cfg.mcu && cfg.mcu.firmware_version,
      settings: mcuSchemaForm ? mcuSchemaForm.getValue() : (cfg.mcu && cfg.mcu.settings) || {},
    } : null;
    await api.put('/api/v1/io', { serial, can, gpio, mcu });
    store.toast('I/O configuration saved', 'success');
  }

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'I/O'), el('p', {}, 'All I/O is rendered from the board manifest — zero hardcoded I/O.')),
    Card({ title: 'Board peripherals' }, overview),
    Card({ title: 'Serial' }, serialSection),
    Card({ title: 'CAN', footer: canFooter }, canSection),
    Card({ title: 'GPIO' }, gpioSection),
    Card({ title: 'I2C' }, i2cSection),
    mcuSection,
    panelSection,
    el('div', { class: 'form-actions' }, Button({ onClick: save }, 'Save I/O configuration')));
}
