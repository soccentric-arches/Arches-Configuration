// Entry point: builds the sidebar/topbar shell, wires the auth flow
// (first-boot / login / logout), and hands off to the router. Loaded as a
// native ES module (spec §2: no bundler, no node at build time — the browser
// resolves every `import` directly against these static files).

import { api, setCsrf, onUnauthorized } from './api.js';
import { Store } from './store.js';
import { initRouter, go, renderCurrent, NAV } from './router.js';
import { installBeforeUnloadGuard } from './dirty-guard.js';
import { initShortcuts, registerShortcut } from './shortcuts.js';
import { Sidebar } from '../components/sidebar.js';
import { Topbar } from '../components/topbar.js';
import { openCommandPalette, registerCommands } from '../components/command-palette.js';
import { showShortcutsHelp } from '../components/shortcuts-help.js';
import { confirmDialog } from '../components/modal.js';

const appShell = document.getElementById('app-shell');
const authScreen = document.getElementById('auth-screen');

// ---------- Shell construction ----------
const sidebar = Sidebar({
  navItems: NAV,
  onNavigate: (id) => { closeMobileSidebar(); go(id); },
  onLogout: async () => {
    try { await api.post('/api/v1/auth/logout', {}); } catch (_) { /* best-effort */ }
    Store.setAuth(null, null);
    showLogin();
  },
});
document.getElementById('sidebar-mount').replaceWith(sidebar.element);

const topbar = Topbar({ onToggleSidebar: toggleMobileSidebar, onOpenPalette: openCommandPalette });
document.getElementById('topbar-mount').replaceWith(topbar.element);

function toggleMobileSidebar() { appShell.classList.toggle('sidebar-open'); }
function closeMobileSidebar() { appShell.classList.remove('sidebar-open'); }
document.getElementById('sidebar-scrim').addEventListener('click', closeMobileSidebar);

Store.onSessionChange((username, role) => topbar.setSession(username, role));
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
    run: async () => { try { await api.post('/api/v1/auth/logout', {}); } catch (_) { /* best-effort */ } Store.setAuth(null, null); showLogin(); },
  },
  {
    id: 'shortcuts', label: 'Keyboard shortcuts', hint: 'Show the cheat sheet', iconName: 'inbox', group: 'Help',
    run: () => showShortcutsHelp(),
  },
]);
initShortcuts({ openPalette: openCommandPalette, openHelp: showShortcutsHelp });
for (const item of NAV) registerShortcut(['g', item.key], () => go(item.id), `Go to ${item.label}`, 'Navigation');

initRouter({ sidebar, topbar });
onUnauthorized(() => { Store.setAuth(null, null); showLogin(); });

// ---------- Auth flow: first-boot, login, whoami ----------
async function bootstrap() {
  try {
    const me = await api.get('/api/v1/auth/whoami');
    if (me && me.csrf) setCsrf(me.csrf);
    if (me && me.first_boot) return showFirstBoot();
    if (me && me.username) { Store.setAuth(me.username, me.role); return enterApp(); }
    return showLogin();
  } catch (_) { return showLogin(); }
}

function showFirstBoot() {
  authScreen.hidden = false;
  appShell.hidden = true;
  document.getElementById('first-boot').hidden = false;
  document.getElementById('login').hidden = true;
}
function showLogin() {
  authScreen.hidden = false;
  appShell.hidden = true;
  document.getElementById('first-boot').hidden = true;
  document.getElementById('login').hidden = false;
}
async function enterApp() {
  authScreen.hidden = true;
  appShell.hidden = false;
  try {
    const m = await api.get('/api/v1/manifest');
    if (m && m.platform) topbar.setBadge(m.platform);
  } catch (_) { /* badge is cosmetic */ }
  renderCurrent();
}

// First-boot form.
document.getElementById('first-boot-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = e.target;
  const err = document.getElementById('first-boot-error');
  err.hidden = true;
  try {
    await api.post('/api/v1/auth/first-boot', { username: f.username.value, password: f.password.value });
    await bootstrap(); // re-fetch whoami -> csrf -> enterApp
  } catch (ex) { err.textContent = ex.message; err.hidden = false; }
});

// Login form.
document.getElementById('login-form').addEventListener('submit', async (e) => {
  e.preventDefault();
  const f = e.target;
  const err = document.getElementById('login-error');
  err.hidden = true;
  try {
    await api.post('/api/v1/auth/login', { username: f.username.value, password: f.password.value });
    await bootstrap(); // whoami carries the CSRF token post-login
  } catch (ex) { err.textContent = ex.message; err.hidden = false; }
});

// Commit/confirm banner buttons (spec §4.2).
document.getElementById('confirm-btn').addEventListener('click', () => Store.confirmNow());
document.getElementById('confirm-rollback-btn').addEventListener('click', () => Store.rollbackNow());

bootstrap();
