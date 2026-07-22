// Minimal inline SVG icon set — stroke-based, 24x24, no external icon font
// or library (spec §2: <500 KB, no build-time deps). Add new icons by
// dropping another path-string entry into ICONS.

const ICONS = {
  system: '<path d="M4 4h16v10H4z"/><path d="M9 20h6M12 14v6"/>',
  network: '<circle cx="12" cy="5" r="2"/><circle cx="5" cy="19" r="2"/><circle cx="19" cy="19" r="2"/><path d="M12 7v4m0 0-5.5 6M12 11l5.5 6"/>',
  services: '<rect x="4" y="4" width="7" height="7" rx="1.5"/><rect x="13" y="4" width="7" height="7" rx="1.5"/><rect x="4" y="13" width="7" height="7" rx="1.5"/><rect x="13" y="13" width="7" height="7" rx="1.5"/>',
  cloud: '<path d="M7 18h10a4 4 0 0 0 .5-8A6 6 0 0 0 6 11a3.5 3.5 0 0 0 1 7z"/>',
  updates: '<path d="M4 12a8 8 0 0 1 14.5-4.7M20 12a8 8 0 0 1-14.5 4.7"/><path d="M18.5 3v4.3H14.2M5.5 21v-4.3h4.3"/>',
  monitoring: '<path d="M3 17l5-6 4 4 5-8 4 5"/><path d="M3 20h18"/>',
  logs: '<path d="M6 4h9l4 4v12H6z"/><path d="M14 4v4h4M9 12h7M9 15.5h7M9 8.5h2"/>',
  alerts: '<path d="M12 3 2 20h20z"/><path d="M12 10v4M12 17h.01"/>',
  security: '<path d="M12 3l7 3v6c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6z"/>',
  io: '<rect x="3" y="7" width="18" height="10" rx="2"/><path d="M7 7v-2M12 7v-2M17 7v-2M7 17v2M12 17v2M17 17v2"/>',
  menu: '<path d="M4 6h16M4 12h16M4 18h16"/>',
  close: '<path d="M5 5l14 14M19 5 5 19"/>',
  logout: '<path d="M9 4H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h3"/><path d="M14 16l4-4-4-4M18 12H8"/>',
  chevronDown: '<path d="M6 9l6 6 6-6"/>',
  chevronRight: '<path d="M9 6l6 6-6 6"/>',
  refresh: '<path d="M4 4v5h5M20 20v-5h-5"/><path d="M4.6 15A8 8 0 0 0 19 9M19.4 9A8 8 0 0 0 5 15"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  trash: '<path d="M4 7h16M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2m2 0-1 13a1 1 0 0 1-1 1H9a1 1 0 0 1-1-1L7 7"/>',
  download: '<path d="M12 3v13m0 0-4.5-4.5M12 16l4.5-4.5"/><path d="M4 18v1a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-1"/>',
  power: '<path d="M12 3v8"/><path d="M6.3 7a8 8 0 1 0 11.4 0"/>',
  check: '<path d="M5 13l4 4L19 7"/>',
  alertTriangle: '<path d="M12 4 3 20h18z"/><path d="M12 10v4M12 17h.01"/>',
  search: '<circle cx="11" cy="11" r="6.5"/><path d="M20 20l-4.3-4.3"/>',
  wifi: '<path d="M2 8.5a15 15 0 0 1 20 0"/><path d="M5.5 12a10 10 0 0 1 13 0"/><path d="M9 15.5a5 5 0 0 1 6 0"/><path d="M12 19h.01"/>',
  play: '<path d="M7 5v14l11-7z"/>',
  stop: '<rect x="6" y="6" width="12" height="12" rx="1.5"/>',
  spinnerArc: '<path d="M12 3a9 9 0 1 0 9 9"/>',
  shield: '<path d="M12 3l7 3v6c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6z"/><path d="M9.5 12l1.8 1.8L14.8 10"/>',
  users: '<circle cx="9" cy="8" r="3"/><path d="M3 20c0-3.3 2.7-5.5 6-5.5s6 2.2 6 5.5"/><circle cx="17.5" cy="9" r="2.3"/><path d="M15.8 14.6c2.5.4 4.2 2.3 4.2 5.4"/>',
  inbox: '<path d="M4 4h16v9l-3 7H7l-3-7z"/><path d="M4 13h5l1.5 3h3L15 13h5"/>',
  cpu: '<rect x="7" y="7" width="10" height="10" rx="1.5"/><path d="M9 7V4M15 7V4M9 20v-3M15 20v-3M4 9h3M4 15h3M20 9h-3M20 15h-3"/>',
  camera: '<path d="M4 8h4l1.5-2h5L16 8h4v11H4z"/><circle cx="12" cy="13.5" r="3.5"/>',
  bell: '<path d="M6 9a6 6 0 0 1 12 0c0 5 2 6 2 6H4s2-1 2-6"/><path d="M10 19a2 2 0 0 0 4 0"/>',
};

export function iconMarkup(name) {
  return ICONS[name] || '';
}

/** Build an <svg> element for `name`. Falls back to an empty group if unknown. */
export function icon(name, { size = 18, strokeWidth = 2 } = {}) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('width', String(size));
  svg.setAttribute('height', String(size));
  svg.setAttribute('fill', 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', String(strokeWidth));
  svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  svg.setAttribute('aria-hidden', 'true');
  svg.innerHTML = iconMarkup(name);
  return svg;
}
