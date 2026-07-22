// Central page registry — the single place that knows every route id, its
// sidebar label/icon, and how to lazily load its module. Adding a tenth page
// means adding one line here plus the page module itself; nothing else in
// the app needs to change.

// `key` is the second letter of the `g <key>` keyboard-shortcut chord
// (core/shortcuts.js) — picked explicitly so pages sharing a first letter
// (System/Services/Security) don't collide.
export const NAV = [
  // Dashboard: understand the current state before changing anything.
  { id: 'overview', label: 'Overview', icon: 'monitoring', category: 'Dashboard', key: 'o', loader: () => import('./overview.js') },
  { id: 'monitoring', label: 'Monitoring', icon: 'monitoring', category: 'Dashboard', key: 'm', loader: () => import('./monitoring.js') },
  { id: 'faults', label: 'Faults', icon: 'alerts', category: 'Dashboard', key: '2', loader: () => import('./faults.js') },
  { id: 'alerts', label: 'Alarms', icon: 'alerts', category: 'Dashboard', key: 'a', loader: () => import('./alerts.js') },
  { id: 'logs', label: 'Logs', icon: 'logs', category: 'Dashboard', key: 'l', loader: () => import('./logs.js') },

  // System: local identity, time, and lifecycle controls.
  { id: 'system', label: 'System settings', icon: 'system', category: 'System', key: 's', loader: () => import('./system.js') },
  { id: 'time', label: 'Time & NTP', icon: 'refresh', category: 'System', key: 't', loader: () => import('./time.js') },
  { id: 'power', label: 'Power', icon: 'power', category: 'System', key: 'p', loader: () => import('./power.js') },

  // Connectivity: network links and connected services.
  { id: 'network', label: 'Networking', icon: 'network', category: 'Connectivity', key: 'n', loader: () => import('./network.js') },
  { id: 'services', label: 'Services', icon: 'services', category: 'Connectivity', key: 'v', loader: () => import('./services.js') },
  { id: 'cloud', label: 'Cloud', icon: 'cloud', category: 'Connectivity', key: 'c', loader: () => import('./cloud.js') },

  // Maintenance: updates and configuration recovery.
  { id: 'updates', label: 'Updates', icon: 'updates', category: 'Maintenance', key: 'u', loader: () => import('./updates.js') },
  { id: 'backup', label: 'Backup & Restore', icon: 'updates', category: 'Maintenance', key: 'b', loader: () => import('./backup.js'), adminOnly: true },

  // Security: access, exposure, trust, and user administration.
  { id: 'security', label: 'Security', icon: 'security', category: 'Security', key: 'e', loader: () => import('./security.js'), adminOnly: true },
  { id: 'ssh', label: 'SSH & Console', icon: 'security', category: 'Security', key: 'j', loader: () => import('./ssh.js'), adminOnly: true },
  { id: 'firewall', label: 'Firewall', icon: 'security', category: 'Security', key: 'f', loader: () => import('./firewall.js'), adminOnly: true },
  { id: 'secure-boot', label: 'Secure Boot', icon: 'security', category: 'Security', key: 'r', loader: () => import('./secure-boot.js'), adminOnly: true },
  { id: 'users', label: 'Users & Groups', icon: 'users', category: 'Security', key: 'g', loader: () => import('./users.js'), adminOnly: true },

  // Hardware: individual peripheral controls.
  { id: 'display', label: 'Display', icon: 'camera', category: 'Hardware', key: 'i', loader: () => import('./display.js') },
  { id: 'leds', label: 'Board LEDs', icon: 'io', category: 'Hardware', key: '1', loader: () => import('./leds.js'), adminOnly: true },
  { id: 'gpio', label: 'GPIO', icon: 'io', category: 'Hardware', key: 'd', loader: () => import('./gpio.js'), adminOnly: true },
  { id: 'serial', label: 'Serial Peripherals', icon: 'io', category: 'Hardware', key: 'w', loader: () => import('./serial.js'), adminOnly: true },
  { id: 'can', label: 'CAN', icon: 'network', category: 'Hardware', key: 'z', loader: () => import('./can.js'), adminOnly: true },
  { id: 'usb', label: 'USB', icon: 'io', category: 'Hardware', key: 'x', loader: () => import('./usb.js') },
  { id: 'pci', label: 'PCI', icon: 'io', category: 'Hardware', key: 'y', loader: () => import('./pci.js') },
  { id: 'mcu', label: 'MCU', icon: 'cpu', category: 'Hardware', key: 'q', loader: () => import('./mcu.js') },
  { id: 'camera', label: 'Camera', icon: 'camera', category: 'Hardware', key: 'k', loader: () => import('./camera.js') },
];

export const ROUTE_IDS = NAV.map(n => n.id);
export const routeMeta = (id) => NAV.find(n => n.id === id);
