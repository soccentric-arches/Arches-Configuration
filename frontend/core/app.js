// Entry point: builds the sidebar/topbar shell, wires the auth flow
// (first-boot / login / logout), and hands off to the router. Loaded as a
// native ES module (spec §2: no bundler, no node at build time — the browser
// resolves every `import` directly against these static files).

import { api, onUnauthorized } from './api.js';
import { Store } from './store.js';
import { initRouter, go, renderCurrent, NAV, setWindowTitleSuffix } from './router.js';
import { createAuthController } from './auth.js';
import { installBeforeUnloadGuard } from './dirty-guard.js';
import { initShortcuts, registerShortcut } from './shortcuts.js';
import { Sidebar } from '../components/sidebar.js';
import { Topbar } from '../components/topbar.js';
import { openCommandPalette, registerCommands } from '../components/command-palette.js';
import { showShortcutsHelp } from '../components/shortcuts-help.js';

const appShell = document.getElementById('app-shell');
const authScreen = document.getElementById('auth-screen');
let authController;

// ---------- Shell construction ----------
const sidebar = Sidebar({
  navItems: NAV,
  onNavigate: (id) => { closeMobileSidebar(); go(id); },
});
document.getElementById('sidebar-mount').replaceWith(sidebar.element);

const topbar = Topbar({
  onToggleSidebar: toggleSidebar,
  onOpenPalette: openCommandPalette,
  onLogout: () => authController.logout(),
});
document.getElementById('topbar-mount').replaceWith(topbar.element);

function toggleSidebar() {
  if (window.matchMedia('(max-width: 880px)').matches) {
    appShell.classList.toggle('sidebar-open');
  } else {
    appShell.classList.toggle('sidebar-collapsed');
  }
}
function closeMobileSidebar() { appShell.classList.remove('sidebar-open'); }
document.getElementById('sidebar-scrim').addEventListener('click', closeMobileSidebar);

Store.onSessionChange((username, role) => topbar.setSession(username, role));

// Board identity for the title bar (like a real board's header): fetch the
// read-only device info once per session and surface it in the topbar chip
// and the browser window title. Non-fatal on failure — the bar simply keeps
// the app name only.
Store.onSessionChange((username) => {
  if (!username) { topbar.setBoard(null); setWindowTitleSuffix(null); return; }
  api.get('/api/v1/system/info').then((info) => {
    topbar.setBoard(info);
    const model = info.model || info.board_name || info.platform || '';
    setWindowTitleSuffix(['WebConfig', model].filter(Boolean).join(' · '));
  }).catch(() => { /* leave the chip hidden and the default title */ });
});
Store.onReconnected(() => renderCurrent());
Store.onReload(() => renderCurrent());
installBeforeUnloadGuard();

// ---------- Command palette + keyboard shortcuts ----------
registerCommands(NAV.map(item => ({
  id: `nav:${item.id}`, label: item.label, hint: 'Go to page', iconName: item.icon, group: 'Navigate',
  run: () => go(item.id),
})));
registerCommands([
  {
    id: 'logout', label: 'Log out', iconName: 'logout', group: 'Session',
    run: () => authController.logout(),
  },
  {
    id: 'shortcuts', label: 'Keyboard shortcuts', hint: 'Show the cheat sheet', iconName: 'inbox', group: 'Help',
    run: () => showShortcutsHelp(),
  },
]);
initShortcuts({ openPalette: openCommandPalette, openHelp: showShortcutsHelp });
for (const item of NAV) registerShortcut(['g', item.key], () => go(item.id), `Go to ${item.label}`, 'Navigation');

initRouter({ sidebar, topbar });
authController = createAuthController({
  authScreen,
  appShell,
  topbar,
  onRender: renderCurrent,
});
onUnauthorized(() => { Store.setAuth(null, null); authController.showLogin(); });

// Commit/confirm banner buttons (spec §4.2).
document.getElementById('confirm-btn').addEventListener('click', () => Store.confirmNow());
document.getElementById('confirm-rollback-btn').addEventListener('click', () => Store.rollbackNow());

authController.bootstrap();
