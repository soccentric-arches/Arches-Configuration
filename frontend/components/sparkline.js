// Canvas sparkline — a tiny rolling-history line chart with no charting
// library (spec §2: vanilla JS only). Feed it new values over time via
// `.push(value)`; it keeps the last `maxPoints` and redraws.

export function Sparkline({ width = 240, height = 48, maxPoints = 60, color = 'var(--primary)', fill = true } = {}) {
  const canvas = document.createElement('canvas');
  const dpr = window.devicePixelRatio || 1;
  canvas.width = width * dpr;
  canvas.height = height * dpr;
  canvas.style.width = '100%';
  canvas.style.height = `${height}px`;
  canvas.className = 'spark';
  const ctx = canvas.getContext('2d');
  ctx.scale(dpr, dpr);

  const values = [];
  const resolvedColor = color.startsWith('var(')
    ? getComputedStyle(document.documentElement).getPropertyValue(color.slice(4, -1)).trim() || '#4f46e5'
    : color;

  function draw() {
    ctx.clearRect(0, 0, width, height);
    if (values.length < 2) return;
    const min = Math.min(...values);
    const max = Math.max(...values);
    const range = max - min || 1;
    const step = width / (maxPoints - 1);
    const startX = width - (values.length - 1) * step;

    ctx.beginPath();
    values.forEach((v, i) => {
      const x = startX + i * step;
      const y = height - 4 - ((v - min) / range) * (height - 8);
      if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    });
    ctx.strokeStyle = resolvedColor;
    ctx.lineWidth = 1.75;
    ctx.lineJoin = 'round';
    ctx.stroke();

    if (fill) {
      ctx.lineTo(startX + (values.length - 1) * step, height);
      ctx.lineTo(startX, height);
      ctx.closePath();
      ctx.fillStyle = resolvedColor.startsWith('#')
        ? `${resolvedColor}1a`
        : 'rgba(79,70,229,.1)';
      ctx.fill();
    }
  }

  function push(v) {
    if (typeof v !== 'number' || Number.isNaN(v)) return;
    values.push(v);
    if (values.length > maxPoints) values.shift();
    draw();
  }

  return { canvas, push, get values() { return values.slice(); } };
}
