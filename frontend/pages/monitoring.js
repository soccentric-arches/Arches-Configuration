// Monitoring page (spec §4.5): live vitals via the daemon's SSE endpoint
// (spec §3 — one SSE endpoint for dashboard push), with an initial REST
// fetch for a fast first paint and one shadcn-style radial gauge per metric
// (components/radial-chart.js — the number is displayed as text in the ring).

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Button } from '../components/button.js';
import { ActionList, ActionRow } from '../components/action-row.js';
import { RadialGauge } from '../components/radial-chart.js';

function fmtBytes(b) {
  if (!b) return '0 B';
  const u = ['B', 'KB', 'MB', 'GB']; let i = 0; let n = b;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(1)} ${u[i]}`;
}

function fmtRate(bytesPerSec) {
  if (bytesPerSec == null) return '—';
  if (bytesPerSec >= 1048576) return `${(bytesPerSec / 1048576).toFixed(1)} MB/s`;
  if (bytesPerSec >= 1024) return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
  return `${bytesPerSec.toFixed(0)} B/s`;
}

function fmtUptimeCompact(seconds) {
  if (!seconds) return '0m';
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days) return `${days}d ${hours}h`;
  if (hours) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

// A metric tile that owns its own radial gauge (ring on top, label under it
// — the number lives in the ring) and updates in place rather than being
// torn down and rebuilt on every SSE tick (keeps the gauge alive and avoids
// visible flicker).
function metricTile(label, gaugeOpts = {}) {
  const gauge = RadialGauge({ size: 124, label: `${label} gauge`, ...gaugeOpts });
  const node = el('div', { class: 'card gauge-tile' },
    el('div', { class: 'card-body gauge-tile-body' },
      gauge.element,
      el('h3', { class: 'gauge-title' }, label),
      el('div', { class: 'gauge-sub' }, 'Live telemetry')));
  return {
    node,
    gauge,
    update(_text, numericValue) {
      if (numericValue != null) gauge.push(numericValue);
    },
  };
}

// Network tiles show THROUGHPUT (bytes/s over the SSE tick interval), not
// the raw rising counter — charted as a gauge normalized to the strongest
// sample seen, with the rate as the center text.
function netMetricTile(label) {
  const tile = metricTile(label, { yFmt: (v) => fmtRate(v), sub: 'bytes/s' });
  let prev = null; // { bytes, t }
  tile.pushCounter = (bytes) => {
    const now = performance.now();
    let rate = null;
    if (prev && bytes >= prev.bytes) {
      const dt = (now - prev.t) / 1000;
      if (dt > 0.2) rate = (bytes - prev.bytes) / dt;
    }
    prev = { bytes, t: now };
    if (rate != null) tile.gauge.push(rate);
  };
  return tile;
}

export async function render(api, store) {
  const cpu = metricTile('CPU load (1m)', { yFmt: (v) => v.toFixed(2), sub: '1m load avg' });
  const mem = metricTile('Memory', { max: 100, yFmt: (v) => `${v.toFixed(0)}%`, sub: 'of RAM' });
  const disk = metricTile('Physical storage', { max: 100, yFmt: (v) => `${v.toFixed(0)}%`, sub: 'of storage' });
  const swap = metricTile('Swap', { max: 100, yFmt: (v) => `${v.toFixed(0)}%`, sub: 'of swap' });
  const temp = metricTile('Hottest', { max: 100, yFmt: (v) => `${v.toFixed(0)}°C`, sub: 'sensor peak' });
  const uptimeEl = el('div', { class: 'tile-value', hidden: true }, '—');
  const bootCountEl = el('div', { class: 'tile-sub', style: 'margin-top:6px' }, 'Boots: —');
  const bootReasonEl = el('div', { class: 'tile-sub' }, 'Last boot: —');
  // Uptime is unbounded, so its gauge normalizes against a rolling max (like
  // CPU load) — the ring fills as the device keeps running, and the center
  // number counts up on every vitals tick.
  const uptimeGauge = RadialGauge({ size: 124, label: 'Uptime gauge', yFmt: fmtUptimeCompact, sub: 'since last boot' });
  const uptimeTile = el('div', { class: 'card gauge-tile' }, el('div', { class: 'card-body gauge-tile-body' },
    uptimeGauge.element,
    el('h3', { class: 'gauge-title' }, 'Uptime'),
    el('div', { class: 'gauge-sub' }, 'Time since last boot'),
    bootCountEl, bootReasonEl));
  const netTiles = new Map();
  const netHost = el('div', { class: 'card-grid grid-2' });

  const liveDot = el('span', { class: 'live-dot' });
  const liveLabel = el('span', { class: 'muted text-sm' }, 'Connecting…');

  function ensureNetTiles(iface) {
    if (!netTiles.has(iface)) {
      // Spec §4.5: per-interface traffic counters from /sys/class/net/*/statistics
      // — show both directions, charted as real throughput (bytes/s deltas).
      const rx = netMetricTile(`${iface} rx`);
      const tx = netMetricTile(`${iface} tx`);
      netTiles.set(iface, { rx, tx });
      netHost.appendChild(rx.node);
      netHost.appendChild(tx.node);
    }
    return netTiles.get(iface);
  }

  function renderVitals(v) {
    cpu.update(v.load && v.load[0] != null ? v.load[0].toFixed(2) : '—', v.load && v.load[0]);

    const memPct = v.mem_total_kb ? ((1 - v.mem_available_kb / v.mem_total_kb) * 100) : null;
    mem.update(memPct != null ? `${memPct.toFixed(0)}%` : '—', memPct);

    const diskPct = (v.mounts || []).filter(m => m.physical).map(m => m.total_bytes ? ((1 - m.avail_bytes / m.total_bytes) * 100) : 0)
      .reduce((a, b) => Math.max(a, b), 0);
    disk.update(diskPct ? `${diskPct.toFixed(0)}%` : '—', diskPct);

    const swapPct = v.swap_total_bytes ? ((1 - v.swap_free_bytes / v.swap_total_bytes) * 100) : null;
    swap.update(swapPct != null ? `${swapPct.toFixed(0)}%` : '—', swapPct);

    const maxTemp = (v.temperatures || []).map(t => t.temp_millideg_c / 1000).reduce((a, b) => Math.max(a, b), -Infinity);
    temp.update(Number.isFinite(maxTemp) ? `${maxTemp.toFixed(0)}°C` : '—', Number.isFinite(maxTemp) ? maxTemp : null);

    uptimeGauge.push(v.uptime_secs);
    bootCountEl.textContent = `Boots: ${v.boot_count != null ? v.boot_count : '—'}`;
    bootReasonEl.textContent = `Last boot: ${v.last_boot_reason || '—'}`;

    for (const n of v.net || []) {
      const t = ensureNetTiles(n.iface);
      t.rx.pushCounter(n.rx_bytes);
      t.tx.pushCounter(n.tx_bytes);
    }
  }

  try {
    renderVitals(await api.get('/api/v1/monitoring/vitals'));
  } catch (_) { /* SSE below will populate once it connects */ }

  const source = api.eventSource('/api/v1/events');
  source.addEventListener('vitals', (e) => {
    liveDot.classList.add('live'); liveLabel.textContent = 'Live';
    try { renderVitals(JSON.parse(e.data)); } catch (_) { /* skip malformed tick */ }
  });
  source.addEventListener('error', () => { liveDot.classList.remove('live'); liveLabel.textContent = 'Reconnecting…'; });
  store.registerCleanup(() => {
    source.close();
    // Gauges keep a themechange listener — remove it with the page.
    for (const tile of [cpu, mem, disk, swap, temp]) tile.gauge.destroy();
    uptimeGauge.destroy();
    for (const t of netTiles.values()) { t.rx.gauge.destroy(); t.tx.gauge.destroy(); }
  });

  // ---------- Service health strip (spec §4.5) ----------
  // One glanceable badge per manifest-allowlisted service: active=green,
  // failed=red, inactive=neutral, anything else=warning. Snapshot on load.
  const healthHost = el('div', { class: 'badge-row' });
  function healthBadgeClass(state) {
    if (state === 'active') return 'badge badge-success';
    if (state === 'failed') return 'badge badge-destructive';
    if (state === 'inactive') return 'badge badge-neutral';
    return 'badge badge-warning';
  }
  async function loadServiceHealth() {
    try {
      const rows = await api.get('/api/v1/services/health');
      if (!rows.length) {
        healthHost.replaceChildren(el('p', { class: 'muted' }, 'No services declared for this platform.'));
        return;
      }
      healthHost.replaceChildren(...rows.map(r =>
        el('span', { class: healthBadgeClass(r.state), title: r.state },
          `${r.unit} · ${r.state}`)));
    } catch (_) { healthHost.replaceChildren(el('p', { class: 'error-text' }, 'Service health unavailable.')); }
  }
  loadServiceHealth();
  const healthCard = Card({ title: 'Service health', subtitle: 'Allowlisted services and their current systemd state (spec §4.5).' }, healthHost);

  // ---------- eMMC wear ----------
  const dev = Field({ label: 'Block device', value: 'mmcblk0', placeholder: 'mmcblk0' });
  const wearResult = el('div', { class: 'tile-sub', style: 'margin-top:8px' });
  const emmcCard = Card({
    title: 'eMMC wear', subtitle: 'Reads the device life-time estimate (spec §4.5).',
    footer: ActionList(ActionRow('Check wear', 'Read the eMMC lifetime estimate from the selected block device.', Button({
      variant: 'outline',
      onClick: async () => {
        const r = await api.get(`/api/v1/monitoring/emmc/${encodeURIComponent(dev.get())}`);
        wearResult.textContent = r.life_time_estimate ? `Life-time estimate: ${r.life_time_estimate}` : 'No wear data reported for this device.';
      },
    }, 'Check wear'))),
  }, dev.wrap, wearResult);

  return el('div', {},
    el('div', { class: 'page-header row', style: 'justify-content:space-between' },
      el('div', {}, el('h1', {}, 'Monitoring'), el('p', {}, 'Live vitals, pushed over SSE.')),
      el('div', { class: 'row' }, liveDot, liveLabel)),
    Card({ title: 'Vitals', subtitle: 'Live CPU, memory, physical storage, swap, temperature, and uptime readings.' }, el('div', { class: 'card-grid grid-2' }, cpu.node, mem.node, disk.node, swap.node, uptimeTile, temp.node)),
    Card({ title: 'Network throughput', subtitle: 'Per-interface receive throughput from the live telemetry stream.' }, netHost),
    el('div', { class: 'card-grid grid-2' }, healthCard, emmcCard));
}
