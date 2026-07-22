// Small Chart.js wrapper for a progressive rolling line. Feed it new values via
// `.push(value)`; it keeps the last `maxPoints` and advances smoothly.

export function Sparkline({ height = 48, maxPoints = 60, color = 'var(--primary)', fill = false, label = 'Telemetry history' } = {}) {
  const container = document.createElement('div');
  container.className = 'spark-wrap';
  container.style.height = `${height}px`;
  const canvas = document.createElement('canvas');
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', label);
  canvas.style.width = '100%';
  canvas.style.height = '100%';
  canvas.className = 'spark';
  container.appendChild(canvas);
  const values = [];
  const resolvedColor = color.startsWith('var(')
    ? getComputedStyle(document.documentElement).getPropertyValue(color.slice(4, -1)).trim() || '#4f46e5'
    : color;

  const chart = new window.Chart(canvas, {
    type: 'line',
    data: {
      datasets: [{
        data: values,
        borderColor: resolvedColor,
        backgroundColor: resolvedColor.startsWith('#') ? `${resolvedColor}1a` : 'rgba(79,70,229,.1)',
        borderWidth: 2.25,
        pointRadius: context => context.dataIndex === context.dataset.data.length - 1 ? 3 : 0,
        pointHoverRadius: 4,
        pointBackgroundColor: resolvedColor,
        pointBorderColor: '#ffffff',
        pointBorderWidth: 2,
        pointHitRadius: 8,
        tension: 0.2,
        fill,
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      animation: {
        duration: 420,
        easing: 'linear',
        from: NaN,
        delay: context => context.type === 'data' && context.mode === 'default'
          ? context.dataIndex * 8
          : 0,
      },
      interaction: { intersect: false, mode: 'index' },
      plugins: {
        legend: { display: false },
        tooltip: { displayColors: false, callbacks: { label: context => `${context.parsed.y}` } },
      },
      scales: {
        x: {
          type: 'linear',
          display: true,
          grid: { display: false },
          ticks: { display: false },
          border: { display: false },
        },
        y: {
          display: true,
          grid: { color: 'rgba(113,113,122,.12)', drawTicks: false },
          ticks: { display: false },
          border: { display: false },
          grace: '8%',
        },
      },
    },
  });

  function push(v) {
    if (typeof v !== 'number' || Number.isNaN(v)) return;
    values.push(v);
    if (values.length > maxPoints) values.shift();
    chart.data.datasets[0].data = values.map((value, i) => ({ x: i, y: value }));
    chart.update();
  }

  return { canvas: container, push, destroy: () => chart.destroy(), get values() { return values.slice(); } };
}
