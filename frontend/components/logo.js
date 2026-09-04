// XConfig brand mark — the X logo as a proper SVG: two rounded ember
// strokes with a gradient (light orange → brand red), a soft offset shadow
// for depth, and four terminal nodes like circuit connection points.
//
// Used at small size in the navbar/topbar brand and large (with glow) on the
// auth screens. The gradient ids are unique per instance so multiple logos
// can live in one document.

const NS = 'http://www.w3.org/2000/svg';

export function XLogo({ size = 28 } = {}) {
    const svg = document.createElementNS(NS, 'svg');
    svg.setAttribute('viewBox', '0 0 48 48');
    svg.setAttribute('width', String(size));
    svg.setAttribute('height', String(size));
    svg.setAttribute('fill', 'none');
    svg.setAttribute('role', 'img');
    svg.setAttribute('aria-label', 'XConfig');

    const gradId = `xlogo-grad-${Math.random().toString(36).slice(2, 9)}`;
    const defs = document.createElementNS(NS, 'defs');
    const grad = document.createElementNS(NS, 'linearGradient');
    grad.setAttribute('id', gradId);
    grad.setAttribute('x1', '0');
    grad.setAttribute('y1', '0');
    grad.setAttribute('x2', '1');
    grad.setAttribute('y2', '1');
    for (const [offset, color] of [['0%', '#fb923c'], ['55%', '#f0562c'], ['100%', '#dc2626']]) {
        const stop = document.createElementNS(NS, 'stop');
        stop.setAttribute('offset', offset);
        stop.setAttribute('stop-color', color);
        grad.appendChild(stop);
    }
    defs.appendChild(grad);
    svg.appendChild(defs);

    // Soft offset shadow — lifts the mark off the page.
    svg.appendChild(mk('path', {
        d: 'M13.5 13.5 L37.5 37.5 M37.5 13.5 L13.5 37.5',
        stroke: 'rgba(15,10,8,0.22)',
        'stroke-width': '8.5',
        'stroke-linecap': 'round',
    }));

    // The X itself.
    svg.appendChild(mk('path', {
        d: 'M11.5 11.5 L36.5 36.5 M36.5 11.5 L11.5 36.5',
        stroke: `url(#${gradId})`,
        'stroke-width': '8.5',
        'stroke-linecap': 'round',
    }));

    // Terminal nodes — the four connection points of the X.
    for (const [cx, cy] of [[11.5, 11.5], [36.5, 11.5], [11.5, 36.5], [36.5, 36.5]]) {
        svg.appendChild(mk('circle', { cx, cy, r: '3', fill: '#fdba74' }));
    }

    return svg;
}

function mk(tag, attrs) {
    const node = document.createElementNS(NS, tag);
    for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
    return node;
}