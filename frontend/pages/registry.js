// Central page registry — the single place that knows every route id, its
// sidebar label/icon, and how to lazily load its module. Adding a tenth page
// means adding one line here plus the page module itself; nothing else in
// the app needs to change.

// `key` is the second letter of the `g <key>` keyboard-shortcut chord
// (core/shortcuts.js) — picked explicitly so pages sharing a first letter
// (System/Services/Security) don't collide.
export const NAV = [
  { id: 'overview', label: 'Overview', icon: 'monitoring', category: 'General', key: 'o', loader: () => import('./overview.js') },
  { id: 'system', label: 'System settings', icon: 'system', category: 'General', key: 's', loader: () => import('./system.js') },
  { id: 'time', label: 'Time & NTP', icon: 'refresh', category: 'General', key: 't', loader: () => import('./time.js') },
  { id: 'power', label: 'Power', icon: 'power', category: 'General', key: 'p', loader: () => import('./power.js') },
  { id: 'network', label: 'Networking', icon: 'network', category: 'Connectivity', key: 'n', loader: () => import('./network.js') },
  { id: 'services', label: 'Services', icon: 'services', category: 'Connectivity', key: 'v', loader: () => import('./services.js') },
  { id: 'cloud', label: 'Cloud', icon: 'cloud', category: 'Connectivity', key: 'c', loader: () => import('./cloud.js') },
  { id: 'updates', label: 'Updates', icon: 'updates', category: 'Operations', key: 'u', loader: () => import('./updates.js') },
  { id: 'monitoring', label: 'Monitoring', icon: 'monitoring', category: 'Operations', key: 'm', loader: () => import('./monitoring.js') },
  { id: 'logs', label: 'Logs', icon: 'logs', category: 'Operations', key: 'l', loader: () => import('./logs.js') },
  { id: 'alerts', label: 'Alerts', icon: 'alerts', category: 'Operations', key: 'a', loader: () => import('./alerts.js') },
  { id: 'security', label: 'Security', icon: 'security', category: 'Security', key: 'e', loader: () => import('./security.js'), adminOnly: true },
  { id: 'users', label: 'Users & Groups', icon: 'users', category: 'Security', key: 'g', loader: () => import('./users.js'), adminOnly: true },
  { id: 'io', label: 'Hardware', icon: 'io', category: 'Hardware', key: 'h', loader: () => import('./io.js') },
  { id: 'mcu', label: 'MCU', icon: 'cpu', category: 'Hardware', key: 'q', loader: () => import('./mcu.js') },
  { id: 'camera', label: 'Camera', icon: 'camera', category: 'Hardware', key: 'k', loader: () => import('./camera.js') },
];

export const ROUTE_IDS = NAV.map(n => n.id);
export const routeMeta = (id) => NAV.find(n => n.id === id);
