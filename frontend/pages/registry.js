// Central page registry — the single place that knows every route id, its
// sidebar label/icon, and how to lazily load its module. Adding a tenth page
// means adding one line here plus the page module itself; nothing else in
// the app needs to change.

// `key` is the second letter of the `g <key>` keyboard-shortcut chord
// (core/shortcuts.js) — picked explicitly so pages sharing a first letter
// (System/Services/Security) don't collide.
export const NAV = [
  { id: 'overview', label: 'Overview', icon: 'monitoring', key: 'o', loader: () => import('./overview.js') },
  { id: 'system', label: 'System settings', icon: 'system', key: 's', loader: () => import('./system.js') },
  { id: 'power', label: 'Power', icon: 'power', key: 'p', loader: () => import('./power.js') },
  { id: 'network', label: 'Networking', icon: 'network', key: 'n', loader: () => import('./network.js') },
  { id: 'services', label: 'Services', icon: 'services', key: 'v', loader: () => import('./services.js') },
  { id: 'cloud', label: 'Cloud', icon: 'cloud', key: 'c', loader: () => import('./cloud.js') },
  { id: 'updates', label: 'Updates', icon: 'updates', key: 'u', loader: () => import('./updates.js') },
  { id: 'monitoring', label: 'Monitoring', icon: 'monitoring', key: 'm', loader: () => import('./monitoring.js') },
  { id: 'logs', label: 'Logs', icon: 'logs', key: 'l', loader: () => import('./logs.js') },
  { id: 'alerts', label: 'Alerts', icon: 'alerts', key: 'a', loader: () => import('./alerts.js') },
  { id: 'security', label: 'Security', icon: 'security', key: 'e', loader: () => import('./security.js'), adminOnly: true },
  { id: 'users', label: 'Users & Groups', icon: 'users', key: 'g', loader: () => import('./users.js'), adminOnly: true },
  { id: 'io', label: 'Hardware', icon: 'io', key: 'h', loader: () => import('./io.js') },
  { id: 'mcu', label: 'MCU', icon: 'cpu', key: 'q', loader: () => import('./mcu.js') },
  { id: 'camera', label: 'Camera', icon: 'camera', key: 'k', loader: () => import('./camera.js') },
];

export const ROUTE_IDS = NAV.map(n => n.id);
export const routeMeta = (id) => NAV.find(n => n.id === id);
