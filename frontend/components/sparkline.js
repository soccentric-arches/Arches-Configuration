// Small Chart.js wrapper for rolling telemetry history. Feed it new values via
// `.push(value)`; it keeps the last `maxPoints` and updates without animation.

export function Sparkline({ height = 48, maxPoints = 60, color = 'var(--primary)', fill = true, label = 'Telemetry history' } = {}) {
  const canvas = document.createElement('canvas');
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', label);
  canvas.style.width = '100%';
  canvas.style.height = `${height}px`;
  canvas.className = 'spark';
  const values = [];
  const resolvedColor = color.startsWith('var(')
    ? getComputedStyle(document.documentElement).getPropertyValue(color.slice(4, -1)).trim() || '#4f46e5'
    : color;

  const chart = new window.Chart(canvas, {
    type: 'line',
    data: {
      labels: [], datasets: [{
        data: values,
        borderColor: resolvedColor,
        backgroundColor: resolvedColor.startsWith('#') ? `${resolvedColor}1a` : 'rgba(79,70,229,.1)',
        borderWidth: 2,
        pointRadius: 0,
        pointHitRadius: 8,
        tension: 0.35,
        fill,
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      animation: false,
      interaction: { intersect: false, mode: 'index' },
      plugins: {
        legend: { display: false },
        tooltip: { displayColors: false, callbacks: { label: context => `${context.parsed.y}` } },
      },
      scales: {
        x: { display: false },
        y: { display: false, grace: '8%' },
      },
    },
  });

  function push(v) {
    if (typeof v !== 'number' || Number.isNaN(v)) return;
    values.push(v);
    if (values.length > maxPoints) values.shift();
    chart.data.labels = values.map((_, i) => i + 1);
    chart.update('none');
  }

  return { canvas, push, destroy: () => chart.destroy(), get values() { return values.slice(); } };
}
