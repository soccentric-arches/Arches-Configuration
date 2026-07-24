// I/O & Peripherals page (spec §4.9): everything is rendered from the board
// manifest — zero hardcoded I/O. Serial/CAN/GPIO config is edited here
// and saved as one PUT (the engine stores it as a single IoConfig section);
// CAN apply, GPIO toggle, and I2C scan are immediate, separate actions.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Toggle } from '../components/toggle.js';
import { Button } from '../components/button.js';
import { Badge } from '../components/badge.js';
import { EmptyState } from '../components/empty-state.js';
import { ActionList, ActionRow } from '../components/action-row.js';

function byKind(peripherals, kind) { return (peripherals || []).filter(p => p.kind === kind); }

export async function render(api, store) {
  const data = await api.get('/api/v1/io');
  const peripherals = (data.manifest || []).filter(p => p.kind !== 'mcu' && p.kind !== 'camera');
  const cfg = data.config || {};
  const serialEnabled = Toggle({ label: 'Enable serial subsystem', checked: cfg.serial_enabled !== false });
  const canEnabled = Toggle({ label: 'Enable CAN subsystem', checked: cfg.can_enabled !== false });
  const gpioEnabled = Toggle({ label: 'Enable GPIO subsystem', checked: cfg.gpio_enabled !== false });
  const camerasEnabled = Toggle({ label: 'Enable camera subsystem', checked: cfg.cameras_enabled !== false });

  // ---------- Overview grid ----------
  const overview = peripherals.length
    ? el('div', { class: 'card-grid' }, ...peripherals.map(p => el('div', { class: 'card peripheral-card' },
      el('div', { class: 'card-body' },
        Badge(p.kind, 'neutral'),
        el('h4', { style: 'margin-top:8px' }, p.human_name || p.id),
        el('div', { class: 'muted text-sm' }, p.device)))))
    : EmptyState({ title: 'No peripherals in the board manifest' });

  // ---------- Serial ----------
  const serialPeripherals = byKind(peripherals, 'serial');
  const serialRows = {};
  const serialSection = serialPeripherals.length ? el('div', { class: 'card-grid' }, ...serialPeripherals.map((p) => {
    const existing = (cfg.serial || []).find(s => s.device === p.device) || {};
    const enabled = Toggle({ label: 'Enable serial port', checked: existing.enabled !== false });
    const baud = Field({ label: 'Baud', type: 'number', value: existing.baud || 115200 });
    const mode = Field({ label: 'Mode', type: 'select', value: existing.mode || (p.caps && p.caps.modes && p.caps.modes[0]) || 'rs232', options: (p.caps && p.caps.modes || ['rs232']).map(m => ({ value: m, label: m.toUpperCase() })) });
    const owner = Field({ label: 'Owner', type: 'select', value: existing.owner || 'unassigned', options: ['unassigned', 'console', 'customer', 'modbus'].map(v => ({ value: v, label: v })) });
    serialRows[p.device] = { enabled, baud, mode, owner };
    return el('div', { class: 'card' }, el('div', { class: 'card-header' }, el('div', {}, el('h4', {}, p.human_name || p.device), el('div', { class: 'card-header-sub' }, 'Serial port communication settings.'))),
      el('div', { class: 'card-body' }, enabled.wrap, baud.wrap, mode.wrap, owner.wrap));
  })) : EmptyState({ title: 'No serial ports on this board' });

  // ---------- CAN ----------
  const canPeripherals = byKind(peripherals, 'can');
  const canRows = {};
  const canSection = canPeripherals.length ? el('div', { class: 'card-grid' }, ...canPeripherals.map((p) => {
    const existing = (cfg.can || []).find(c => c.name === p.device) || {};
    const enabled = Toggle({ label: 'Enable CAN interface', checked: existing.enabled !== false });
    const bitrate = Field({ label: 'Bitrate', type: 'number', value: existing.bitrate || 500000 });
    const fd = Toggle({ label: 'CAN FD', checked: existing.fd });
    const dataBitrate = Field({ label: 'FD data bitrate', type: 'number', value: existing.data_bitrate || 2000000 });
    const listenOnly = Toggle({ label: 'Listen only', checked: existing.listen_only });
    const upOnBoot = Toggle({ label: 'Up on boot', checked: existing.up_on_boot !== false });
    canRows[p.device] = { enabled, bitrate, fd, dataBitrate, listenOnly, upOnBoot };
    return el('div', { class: 'card' }, el('div', { class: 'card-header' }, el('div', {}, el('h4', {}, p.human_name || p.device), el('div', { class: 'card-header-sub' }, 'CAN bus speed, mode, and startup settings.'))),
      el('div', { class: 'card-body' }, el('div', { class: 'toggle-stack' }, enabled.wrap, fd.wrap, listenOnly.wrap, upOnBoot.wrap), bitrate.wrap, dataBitrate.wrap));
  })) : EmptyState({ title: 'No CAN interfaces on this board' });
  const canFooter = canPeripherals.length ? ActionList(ActionRow('Apply CAN', 'Apply the configured CAN bitrate, mode, and startup settings to the interfaces.', Button({
    variant: 'outline',
    onClick: async () => { await api.post('/api/v1/io/can/apply', {}); store.toast('CAN applied', 'success'); },
  }, 'Apply CAN'))) : null;

  // ---------- GPIO ----------
  const gpioPeripherals = byKind(peripherals, 'gpio');
  const gpioRows = {};
  const gpioSection = gpioPeripherals.length ? el('div', { class: 'card-grid' }, ...gpioPeripherals.map((p) => {
    const existing = (cfg.gpio || []).find(g => g.name === p.id) || {};
    const direction = (p.caps && p.caps.direction) || existing.direction || 'out';
    const enabled = Toggle({ label: 'Enable GPIO pin', checked: existing.enabled !== false });
    const initial = Toggle({ label: 'Initial state (on boot)', checked: existing.initial_state === true });
    gpioRows[p.id] = { enabled, direction, activeLow: (p.caps && p.caps.active_low) || false, initial };
    return el('div', { class: 'card' },
      el('div', { class: 'card-header' }, el('div', {}, el('h4', {}, p.human_name || p.id), el('div', { class: 'card-header-sub' }, 'GPIO direction and startup behavior.')), Badge(direction, 'neutral')),
      el('div', { class: 'card-body' },
        enabled.wrap,
        direction === 'out' ? initial.wrap : el('p', { class: 'muted' }, 'Input pin'),
        direction === 'out' ? Button({
          variant: 'outline', size: 'sm', title: 'Set this output pin high',
          onClick: async () => { const r = await api.post(`/api/v1/io/gpio/${encodeURIComponent(p.id)}/toggle`, {}); store.toast(`${p.id} → ${r.value}`, 'success'); },
        }, 'Toggle now') : null));
  })) : EmptyState({ title: 'No GPIO pins on this board' });

  // ---------- I2C ----------
  const i2cPeripherals = byKind(peripherals, 'i2c');
  const i2cSection = i2cPeripherals.length ? el('div', { class: 'card-grid' }, ...i2cPeripherals.map((p) => {
    const result = el('div', { class: 'muted text-sm', style: 'margin-top:8px' });
    return el('div', { class: 'card' }, el('div', { class: 'card-header' }, el('div', {}, el('h4', {}, p.human_name || p.id), el('div', { class: 'card-header-sub' }, 'I2C bus discovery and address scanning.'))),
      el('div', { class: 'card-body' },
        Button({
          variant: 'outline', size: 'sm', title: 'Scan this I2C bus for connected devices',
          onClick: async () => {
            const r = await api.post(`/api/v1/io/i2c/scan?bus=${encodeURIComponent(p.id)}`, {});
            result.textContent = r.addresses && r.addresses.length ? `Found: ${r.addresses.map(a => '0x' + a.toString(16)).join(', ')}` : 'No devices found';
          },
        }, 'Scan bus'),
        result));
  })) : EmptyState({ title: 'No I2C buses on this board' });

  // ---------- Platform-specific panel ----------
  const panel = await api.get('/api/v1/io/platform-panel').catch(() => ({}));
  const panelEntries = Object.entries(panel).filter(([, v]) => v != null && (!Array.isArray(v) || v.length));
  const panelSection = panelEntries.length ? Card({ title: 'Platform panel', subtitle: 'Additional platform-specific hardware information from the board manifest.' },
    ...panelEntries.map(([k, v]) => el('div', { class: 'row' },
      el('strong', {}, k + ':'),
      el('span', { class: 'muted' }, Array.isArray(v) ? (v.map(x => (typeof x === 'object' ? (x.human_name || x.id || JSON.stringify(x)) : x)).join(', ') || 'none') : String(v))))) : null;

  async function save() {
    const serial = serialPeripherals.map(p => ({ device: p.device, enabled: serialRows[p.device].enabled.get(), baud: Number(serialRows[p.device].baud.get()) || 115200, mode: serialRows[p.device].mode.get(), owner: serialRows[p.device].owner.get() }));
    const can = canPeripherals.map(p => ({
      name: p.device, enabled: canRows[p.device].enabled.get(), bitrate: Number(canRows[p.device].bitrate.get()) || 500000, fd: canRows[p.device].fd.get(),
      data_bitrate: Number(canRows[p.device].dataBitrate.get()) || 0, listen_only: canRows[p.device].listenOnly.get(),
      restart_ms: (cfg.can || []).find(c => c.name === p.device)?.restart_ms || 100, up_on_boot: canRows[p.device].upOnBoot.get(),
    }));
    const gpio = gpioPeripherals.map(p => ({
      name: p.id, enabled: gpioRows[p.id].enabled.get(), direction: gpioRows[p.id].direction, active_low: gpioRows[p.id].activeLow,
      initial_state: gpioRows[p.id].direction === 'out' ? gpioRows[p.id].initial.get() : null,
    }));
    await api.put('/api/v1/io', { serial, serial_enabled: serialEnabled.get(), can, can_enabled: canEnabled.get(), gpio, gpio_enabled: gpioEnabled.get(), cameras: cfg.cameras || [], cameras_enabled: camerasEnabled.get(), mcu: cfg.mcu || null, board_leds: cfg.board_leds || {} });
    store.toast('Hardware configuration saved', 'success');
  }

  return el('div', {},
    el('div', { class: 'page-header' }, el('h1', {}, 'Hardware'), el('p', {}, 'Configure serial, CAN, GPIO, and I2C hardware from the board manifest.')),
    Card({ title: 'Board peripherals', subtitle: 'Manifest inventory for configurable serial, CAN, GPIO, and I2C hardware.' }, overview),
    Card({ title: 'Serial', subtitle: 'Configure port speed, protocol mode, and ownership.' }, serialEnabled.wrap, serialSection),
    Card({ title: 'CAN', subtitle: 'Configure CAN bitrate, FD mode, listen-only behavior, and startup.', footer: canFooter }, canEnabled.wrap, canSection),
    Card({ title: 'GPIO', subtitle: 'Configure output startup state and toggle output pins when supported.' }, gpioEnabled.wrap, gpioSection),
    Card({ title: 'Cameras', subtitle: 'Enable the camera subsystem before configuring individual cameras.' }, camerasEnabled.wrap),
    Card({ title: 'I2C', subtitle: 'Inspect buses and scan for connected device addresses.' }, i2cSection),
    panelSection,
    Card({ title: 'Hardware actions', subtitle: 'Save the edited hardware configuration as one device configuration.', },
      ActionList(ActionRow('Save hardware configuration', 'Persist serial, CAN, GPIO, and MCU settings from the sections above.', Button({ onClick: save }, 'Save configuration')))));
}
