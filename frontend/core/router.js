// Hash-based router. Each route lazily imports its page module (native
// dynamic import — no bundler needed) and mounts the returned node into
// #view. Errors are caught and rendered inline rather than left as a stuck
// loading state.

import { el, mount } from './dom.js';
import { api } from './api.js';
import { Store } from './store.js';
import { isDirty, watchForEdits } from './dirty-guard.js';
import { startProgress, finishProgress } from '../components/progress-bar.js';
import { confirmDialog } from '../components/modal.js';
import { PageSkeleton } from '../components/skeleton.js';
import { EmptyState } from '../components/empty-state.js';
import { NAV, ROUTE_IDS, routeMeta } from '../pages/registry.js';

let sidebarApi = null;
let topbarApi = null;
let current = null;

export function initRouter({ sidebar, topbar }) {
  sidebarApi = sidebar;
  topbarApi = topbar;
  window.addEventListener('hashchange', renderCurrent);
}

function parse() {
  const h = location.hash.replace(/^#\/?/, '').split('/')[0] || 'system';
  return ROUTE_IDS.includes(h) ? h : 'system';
}

/** Programmatic navigation (sidebar, command palette). Guards unsaved edits. */
export async function go(route) {
  if (route === current) { renderCurrent(); return; }
  if (isDirty() && !await confirmDialog({
    title: 'Discard unsaved changes?',
    description: 'You have edits on this page that haven’t been saved.',
    confirmLabel: 'Discard', variant: 'destructive',
  })) return;
  location.hash = `/${route}`;
}

export async function renderCurrent() {
  const route = parse();
  current = route;
  if (sidebarApi) sidebarApi.setActive(route);

  const meta = routeMeta(route);
  if (topbarApi) topbarApi.setTitle(meta ? meta.label : route);
  document.title = meta ? `${meta.label} — SoCcentric WebConfig` : 'SoCcentric WebConfig';

  Store.clearTimers();
  Store.runCleanups();

  const view = document.getElementById('view');
  view.hidden = false;
  mount(view, el('div', { class: 'page-loading' }, PageSkeleton()));
  startProgress();

  if (meta && meta.adminOnly && Store.role !== 'admin') {
    mount(view, EmptyState({ iconName: 'shield', title: 'Admin role required', description: 'Sign in as an administrator to view this page.' }));
    finishProgress();
    return;
  }

  try {
    const mod = await meta.loader();
    const node = await mod.render(api, Store);
    mount(view, node);
    view.classList.remove('page-enter');
    void view.offsetWidth; // restart the animation on every navigation
    view.classList.add('page-enter');
    watchForEdits(view);
    // Move focus to the new page for screen-reader/keyboard users — a bare
    // hash change otherwise leaves focus stranded on the sidebar link.
    view.setAttribute('tabindex', '-1');
    view.focus({ preventScroll: true });
  } catch (e) {
    mount(view, EmptyState({ iconName: 'alertTriangle', title: 'Couldn’t load this page', description: e.message }));
  } finally {
    finishProgress();
  }
}

export { NAV };
