// Shadcn/ui-inspired radial chart (ui.shadcn.com/charts/radial) — vanilla SVG,
// no chart library (spec §2: no build step, < 500 KB, no node deps).
//
// One ring per metric: a muted track circle with a colored arc for the value,
// and the NUMBER displayed as text in the center (shadcn radial style), with
// a small muted caption under it. Bounded metrics pass a fixed `max` (e.g.
// 100 for percentages); unbounded ones (CPU load, throughput) normalize
// against a rolling max of the samples seen so far.
//
// Themed through design tokens and re-read on the `themechange` event, so
// light/dark both render correctly.

import { el } from '../core/dom.js';

const NS = 'http://www.w3.org/2000/svg';

function resolveColor(ref, fallback) {
    if (typeof ref !== 'string') return fallback;
    if (ref.startsWith('var(')) {
        const name = ref.slice(4, -1).trim();
        return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
    }
    return ref;
}

function svgEl(tag, attrs = {}) {
    const node = document.createElementNS(NS, tag);
    for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
    return node;
}

export function RadialGauge({
    size = 124,
    strokeWidth = 10,
    color = 'var(--primary)',
    max = null,           // fixed domain ceiling (e.g. 100 for %); null = rolling max
    valueFmt = (v) => v.toFixed(0),
    sub = '',             // small caption under the number
    label = 'Metric gauge',
} = {}) {
    const r = (size - strokeWidth) / 2 - 1;
    const cxy = size / 2;
    const circ = 2 * Math.PI * r;
    let stroke = resolveColor(color, '#0ea5e9');
    let gaugeMax = max != null ? max : 0;
    let destroyed = false;

    const wrap = el('div', { class: 'radial-wrap', role: 'img', 'aria-label': label });
    wrap.style.width = `${size}px`;
    wrap.style.height = `${size}px`;

    const svg = svgEl('svg', { width: size, height: size, viewBox: `0 0 ${size} ${size}` });
    const track = svgEl('circle', {
        cx: cxy, cy: cxy, r, fill: 'none',
        stroke: 'var(--muted)', 'stroke-width': strokeWidth,
    });
    const arc = svgEl('circle', {
        class: 'radial-arc', cx: cxy, cy: cxy, r, fill: 'none',
        stroke, 'stroke-width': strokeWidth, 'stroke-linecap': 'round',
        'stroke-dasharray': `0 ${circ.toFixed(2)}`,
        transform: `rotate(-90 ${cxy} ${cxy})`,
    });
    const valueText = svgEl('text', {
        x: cxy, y: cxy + 1, 'text-anchor': 'middle',
        'dominant-baseline': 'middle', class: 'radial-value',
    });
    valueText.textContent = '—';
    const subText = svgEl('text', {
        x: cxy, y: cxy + 22, 'text-anchor': 'middle', class: 'radial-sub',
    });
    subText.textContent = sub;
    svg.appendChild(track);
    svg.appendChild(arc);
    svg.appendChild(valueText);
    svg.appendChild(subText);
    wrap.appendChild(svg);

    let shown = null; // the value the center text is currently displaying
    let raf = 0;

    // Count-up tween: the center number glides from its previous value to the
    // new one (ease-out cubic) while the arc sweeps — the whole gauge moves.
    function tweenTo(target) {
        if (raf) cancelAnimationFrame(raf);
        const from = shown == null ? 0 : shown;
        const t0 = performance.now();
        const dur = 500;
        const step = (now) => {
            if (destroyed) return;
            const p = Math.min(1, (now - t0) / dur);
            const e = 1 - Math.pow(1 - p, 3);
            shown = from + (target - from) * e;
            valueText.textContent = valueFmt(shown);
            if (p < 1) raf = requestAnimationFrame(step);
        };
        raf = requestAnimationFrame(step);
    }

    function push(v) {
        if (destroyed || v == null || !Number.isFinite(v)) return;
        if (max == null) {
            // Unbounded metric: normalize against the strongest sample seen,
            // with headroom, so the arc stays readable as load grows.
            gaugeMax = Math.max(gaugeMax, v * 1.15, 1);
        }
        const frac = Math.max(0, Math.min(1, v / gaugeMax));
        arc.setAttribute('stroke-dasharray', `${(frac * circ).toFixed(2)} ${circ.toFixed(2)}`);
        tweenTo(v);
    }

    // Retune colors on theme change.
    const onThemeChange = () => {
        stroke = resolveColor(color, '#0ea5e9');
        arc.setAttribute('stroke', stroke);
    };
    document.addEventListener('themechange', onThemeChange);

    return {
        element: wrap,
        push,
        destroy() {
            destroyed = true;
            if (raf) cancelAnimationFrame(raf);
            document.removeEventListener('themechange', onThemeChange);
        },
    };
}