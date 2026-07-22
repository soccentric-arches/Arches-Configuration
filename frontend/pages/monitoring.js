// Monitoring page (spec §4.5): live vitals via the daemon's SSE endpoint
// (spec §3 — one SSE endpoint for dashboard push), with an initial REST
// fetch for a fast first paint and rolling sparkline history per metric.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Field } from '../components/field.js';
import { Button } from '../components/button.js';
import { Sparkline } from '../components/sparkline.js';

function fmtBytes(b) {
  if (!b) return '0 B';
  const u = ['B', 'KB', 'MB', 'GB']; let i = 0; let n = b;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(1)} ${u[i]}`;
}

// A metric tile that owns its own sparkline and updates in place rather
// than being torn down and rebuilt on every SSE tick (keeps the chart
// history alive and avoids visible flicker on a several-times-a-second feed).
function metricTile(label) {
  const valueEl = el('div', { class: 'tile-value' }, '—');
  const spark = Sparkline({ height: 36 });
  const node = el('div', { class: 'card' },
    el('div', { class: 'card-body' }, el('h3', {}, label), valueEl, spark.canvas));
  return {
    node,
    update(text, numericValue) {
      valueEl.textContent = text;
      if (numericValue != null) spark.push(numericValue);
    },
  };
}

export async function render(api, store) {
  const cpu = metricTile('CPU load (1m)');
  const mem = metricTile('Memory');
  const disk = metricTile('Disk (max)');
  const temp = metricTile('Hottest');
  const uptimeEl = el('div', { class: 'tile-value' }, '—');
  const uptimeTile = el('div', { class: 'card' }, el('div', { class: 'card-body' }, el('h3', {}, 'Uptime'), uptimeEl));
  const netTiles = new Map();
  const netHost = el('div', { class: 'card-grid' });

  const liveDot = el('span', { class: 'live-dot' });
  const liveLabel = el('span', { class: 'muted', style: 'font-size:13px' }, 'Connecting…');

  function renderVitals(v) {
    cpu.update(v.load && v.load[0] != null ? v.load[0].toFixed(2) : '—', v.load && v.load[0]);

    const memPct = v.mem_total_kb ? ((1 - v.mem_available_kb / v.mem_total_kb) * 100) : null;
    mem.update(memPct != null ? `${memPct.toFixed(0)}%` : '—', memPct);

    const diskPct = (v.mounts || []).map(m => m.total_bytes ? ((1 - m.avail_bytes / m.total_bytes) * 100) : 0)
      .reduce((a, b) => Math.max(a, b), 0);
    disk.update(diskPct ? `${diskPct.toFixed(0)}%` : '—', diskPct);

    const maxTemp = (v.temperatures || []).map(t => t.temp_millideg_c / 1000).reduce((a, b) => Math.max(a, b), -Infinity);
    temp.update(Number.isFinite(maxTemp) ? `${maxTemp.toFixed(0)}°C` : '—', Number.isFinite(maxTemp) ? maxTemp : null);

    uptimeEl.textContent = v.uptime_secs ? `${Math.round(v.uptime_secs / 60)} min` : '—';

    for (const n of v.net || []) {
      if (!netTiles.has(n.iface)) {
        const t = metricTile(`${n.iface} rx`);
        netTiles.set(n.iface, t);
        netHost.appendChild(t.node);
      }
      netTiles.get(n.iface).update(fmtBytes(n.rx_bytes), n.rx_bytes / 1024);
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
  store.registerCleanup(() => source.close());

  // ---------- eMMC wear ----------
  const dev = Field({ label: 'Block device', value: 'mmcblk0', placeholder: 'mmcblk0' });
  const wearResult = el('div', { class: 'tile-sub', style: 'margin-top:8px' });
  const emmcCard = Card({
    title: 'eMMC wear', subtitle: 'Reads the device life-time estimate (spec §4.5).',
    footer: Button({
      variant: 'outline',
      onClick: async () => {
        const r = await api.get(`/api/v1/monitoring/emmc/${encodeURIComponent(dev.get())}`);
        wearResult.textContent = r.life_time_estimate ? `Life-time estimate: ${r.life_time_estimate}` : 'No wear data reported for this device.';
      },
    }, 'Check wear'),
  }, dev.wrap, wearResult);

  return el('div', {},
    el('div', { class: 'page-header row', style: 'justify-content:space-between' },
      el('div', {}, el('h1', {}, 'Monitoring'), el('p', {}, 'Live vitals, pushed over SSE.')),
      el('div', { class: 'row' }, liveDot, liveLabel)),
    Card({ title: 'Vitals' }, el('div', { class: 'card-grid' }, cpu.node, mem.node, disk.node, uptimeTile, temp.node)),
    Card({ title: 'Network throughput' }, netHost),
    emmcCard);
}
