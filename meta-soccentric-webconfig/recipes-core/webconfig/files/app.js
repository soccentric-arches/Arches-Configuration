// app.js — application shell: router, API client (CSRF + session), auth flow,
// SSE client, reconnect polling, commit/confirm countdown, mobile nav.
// Vanilla JS, no framework. Loaded after schema-form.js and pages.js.

(function () {
  'use strict';

  // ---------- API client: JSON + CSRF double-submit (spec §4.8) ----------
  // The CSRF token is the SHA-256 of the session id, returned by GET /auth/whoami
  // as `csrf`. We capture it there and echo it back on every mutation via the
  // X-CSRF-Token header (the csrf_guard middleware compares them).
  let csrfToken = null;
  function setCsrf(t) { csrfToken = t; }

  const Api = {
    async request(method, url, body, isForm) {
      const headers = {};
      if (body != null && !isForm) headers['Content-Type'] = 'application/json';
      // CSRF double-submit: send the token we got from whoami on mutations.
      if (csrfToken && method !== 'GET' && method !== 'HEAD' && method !== 'OPTIONS') {
        headers['X-CSRF-Token'] = csrfToken;
      }
      const opts = { method, headers, credentials: 'same-origin' };
      if (body != null) opts.body = isForm ? body : JSON.stringify(body);
      const res = await fetch(url, opts);
      if (res.status === 401 && !url.endsWith('/auth/whoami')) {
        // Session expired -> back to login.
        Store.onUnauthorized();
        throw new Error('unauthorized');
      }
      const text = await res.text();
      let data = null;
      if (text) { try { data = JSON.parse(text); } catch (_) { data = text; } }
      if (!res.ok) {
        const msg = (data && data.error) || `HTTP ${res.status}`;
        throw new Error(msg);
      }
      return data;
    },
    get(url) { return this.request('GET', url, null); },
    post(url, body) { return this.request('POST', url, body); },
    put(url, body) { return this.request('PUT', url, body); },
    del(url) { return this.request('DELETE', url, null); },
    async upload(url, file) {
      const form = new FormData();
      form.append('bundle', file);
      return this.request('POST', url, form, true);
    },
    eventSource(url) { return new EventSource(url); },
  };

  // ---------- UI store: timers, cleanup, toast, confirm banner ----------
  const timers = new Set();
  const cleanups = [];

  const Store = {
    role: null,
    username: null,
    pendingConfirm: null,

    registerTimer(handle) { timers.add(handle); return handle; },
    registerCleanup(fn) { cleanups.push(fn); },
    clearTimers() { for (const t of timers) clearInterval(t); timers.clear(); },
    runCleanups() { for (const fn of cleanups) try { fn(); } catch (_) {} cleanups.length = 0; },

    toast(msg) {
      const t = document.createElement('div');
      t.textContent = msg;
      t.style.cssText = 'position:fixed;bottom:16px;left:50%;transform:translateX(-50%);' +
        'background:var(--text);color:#fff;padding:10px 16px;border-radius:8px;z-index:60';
      document.body.appendChild(t);
      setTimeout(() => t.remove(), 2500);
    },

    showConfirm(secs) {
      this.pendingConfirm = secs;
      const banner = document.getElementById('confirm-banner');
      const span = document.getElementById('confirm-seconds');
      banner.hidden = false;
      span.textContent = String(secs);
      if (this._confirmTimer) clearInterval(this._confirmTimer);
      this._confirmTimer = setInterval(() => {
        this.pendingConfirm -= 1;
        span.textContent = String(Math.max(0, this.pendingConfirm));
        if (this.pendingConfirm <= 0) this.hideConfirm();
      }, 1000);
    },
    hideConfirm() {
      document.getElementById('confirm-banner').hidden = true;
      if (this._confirmTimer) { clearInterval(this._confirmTimer); this._confirmTimer = null; }
      this.pendingConfirm = null;
    },
    async confirmNow() {
      try { await Api.post('/api/v1/network/confirm', {}); } catch (e) { this.toast(e.message); }
      this.hideConfirm();
    },
    async rollbackNow() {
      try { await Api.post('/api/v1/network/rollback', {}); } catch (e) { this.toast(e.message); }
      this.hideConfirm();
    },

    onUnauthorized() { this.setAuth(null, null); Router.go('login'); },

    setAuth(username, role) {
      this.username = username; this.role = role;
      const ind = document.getElementById('session-indicator');
      const out = document.getElementById('logout-btn');
      if (username) {
        ind.hidden = false; ind.textContent = `${username} · ${role || ''}`;
        out.hidden = false;
      } else {
        ind.hidden = true; out.hidden = true;
      }
    },

    reload() { Router.renderCurrent(); },
  };

  // ---------- Router: hash-based, role-gated ----------
  const NAV = [
    ['system', 'System'],
    ['network', 'Networking'],
    ['services', 'Services'],
    ['updates', 'Updates'],
    ['monitoring', 'Monitoring'],
    ['logs', 'Logs'],
    ['alerts', 'Alerts'],
    ['security', 'Security'],
    ['io', 'I/O'],
  ];

  const Router = {
    current: null,
    buildNav() {
      const nav = document.getElementById('nav');
      nav.innerHTML = '';
      for (const [id, label] of NAV) {
        const a = document.createElement('a');
        a.href = `#/${id}`; a.textContent = label; a.dataset.route = id;
        a.addEventListener('click', () => {
          nav.classList.remove('open'); // close mobile drawer on navigate
        });
        nav.appendChild(a);
      }
    },
    highlight() {
      document.querySelectorAll('#nav a').forEach(a => {
        a.classList.toggle('active', a.dataset.route === this.current);
      });
    },
    go(route) { if (route !== this.current) location.hash = `/${route}`; else this.renderCurrent(); },
    parse() {
      const h = location.hash.replace(/^#\/?/, '').split('/')[0] || 'system';
      return NAV.some(([id]) => id === h) ? h : 'system';
    },
    async renderCurrent() {
      const route = this.parse();
      this.current = route;
      this.highlight();
      Store.clearTimers(); Store.runCleanups();

      const view = document.getElementById('view');
      view.hidden = false;
      view.innerHTML = '<div class="spinner" aria-hidden="true"></div>';

      // Security page is admin-only; others require a session.
      if (route === 'security' && Store.role !== 'admin') {
        view.innerHTML = '<p class="error">Admin role required.</p>';
        return;
      }
      const render = global.Pages.PAGES[route];
      try {
        const node = await render(Api, Store);
        view.innerHTML = '';
        view.appendChild(node);
      } catch (e) {
        view.innerHTML = `<p class="error">${e.message}</p>`;
      }
    },
  };

  // ---------- Auth flow: first-boot, login, whoami ----------
  async function bootstrap() {
    Router.buildNav();
    try {
      const me = await Api.get('/api/v1/auth/whoami');
      if (me && me.csrf) setCsrf(me.csrf);
      if (me && me.first_boot) return showFirstBoot();
      if (me && me.username) { Store.setAuth(me.username, me.role); return enterApp(); }
      return showLogin();
    } catch (_) { return showLogin(); }
  }

  function showFirstBoot() {
    document.getElementById('first-boot').hidden = false;
    document.getElementById('login').hidden = true;
    document.getElementById('view').hidden = true;
  }
  function showLogin() {
    document.getElementById('first-boot').hidden = true;
    document.getElementById('login').hidden = false;
    document.getElementById('view').hidden = true;
    Store.setAuth(null, null);
  }
  async function enterApp() {
    document.getElementById('first-boot').hidden = true;
    document.getElementById('login').hidden = true;
    // Platform badge (post-auth; /manifest is protected).
    try {
      const m = await Api.get('/api/v1/manifest');
      const badge = document.getElementById('platform-badge');
      if (m && m.platform) { badge.textContent = m.platform; badge.hidden = false; }
    } catch (_) {}
    Router.renderCurrent();
  }

  // First-boot form
  document.getElementById('first-boot-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const f = e.target;
    const err = document.getElementById('first-boot-error');
    err.hidden = true;
    try {
      await Api.post('/api/v1/auth/first-boot', {
        username: f.username.value, password: f.password.value,
      });
      await bootstrap(); // re-fetch whoami -> csrf -> enterApp
    } catch (ex) { err.textContent = ex.message; err.hidden = false; }
  });

  // Login form
  document.getElementById('login-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const f = e.target;
    const err = document.getElementById('login-error');
    err.hidden = true;
    try {
      await Api.post('/api/v1/auth/login', {
        username: f.username.value, password: f.password.value,
      });
      await bootstrap(); // whoami carries the CSRF token post-login
    } catch (ex) { err.textContent = ex.message; err.hidden = false; }
  });

  // Logout
  document.getElementById('logout-btn').addEventListener('click', async () => {
    try { await Api.post('/api/v1/auth/logout', {}); } catch (_) {}
    Store.setAuth(null, null);
    showLogin();
  });

  // ---------- Confirm banner buttons ----------
  document.getElementById('confirm-btn').addEventListener('click', () => Store.confirmNow());
  document.getElementById('confirm-rollback-btn').addEventListener('click', () => Store.rollbackNow());

  // ---------- Mobile nav toggle ----------
  document.getElementById('nav-toggle').addEventListener('click', () => {
    document.getElementById('nav').classList.toggle('open');
  });

  // ---------- Reconnect overlay: poll whoami until the daemon answers ----------
  let reconnecting = false;
  function showReconnect() {
    if (reconnecting) return;
    reconnecting = true;
    document.getElementById('reconnect').hidden = false;
    const h = setInterval(async () => {
      try {
        await Api.get('/api/v1/auth/whoami');
        document.getElementById('reconnect').hidden = true;
        clearInterval(h);
        reconnecting = false;
        Router.renderCurrent();
      } catch (_) { /* still down */ }
    }, 2000);
    Store.registerTimer(h);
  }

  // ---------- Hash routing ----------
  window.addEventListener('hashchange', () => Router.renderCurrent());

  bootstrap();
})();