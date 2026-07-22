// Central page registry — the single place that knows every route id, its
// sidebar label/icon, and how to lazily load its module. Adding a tenth page
// means adding one line here plus the page module itself; nothing else in
// the app needs to change.

// `key` is the second letter of the `g <key>` keyboard-shortcut chord
// (core/shortcuts.js) — picked explicitly so pages sharing a first letter
// (System/Services/Security) don't collide.
export const NAV = [
  { id: 'system', label: 'System', icon: 'system', key: 's', loader: () => import('./system.js') },
  { id: 'network', label: 'Networking', icon: 'network', key: 'n', loader: () => import('./network.js') },
  { id: 'services', label: 'Services', icon: 'services', key: 'v', loader: () => import('./services.js') },
  { id: 'updates', label: 'Updates', icon: 'updates', key: 'u', loader: () => import('./updates.js') },
  { id: 'monitoring', label: 'Monitoring', icon: 'monitoring', key: 'm', loader: () => import('./monitoring.js') },
  { id: 'logs', label: 'Logs', icon: 'logs', key: 'l', loader: () => import('./logs.js') },
  { id: 'alerts', label: 'Alerts', icon: 'alerts', key: 'a', loader: () => import('./alerts.js') },
  { id: 'security', label: 'Security', icon: 'security', key: 'e', loader: () => import('./security.js'), adminOnly: true },
  { id: 'io', label: 'I/O', icon: 'io', key: 'i', loader: () => import('./io.js') },
];

export const ROUTE_IDS = NAV.map(n => n.id);
export const routeMeta = (id) => NAV.find(n => n.id === id);
