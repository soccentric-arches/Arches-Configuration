// Left navigation sidebar — category sections with links, plus the
// collapse/expand control at its foot (the control belongs to the sidebar,
// not the main-page topbar).

import { el } from '../core/dom.js';
import { icon } from '../core/icons.js';

/**
 * Sidebar({ navItems, onNavigate, onToggle }) -> { element, setActive, setCollapsed }
 * navItems: [{ id, label, icon, category }]
 */
export function Sidebar({ navItems, onNavigate, onToggle }) {
    const links = new Map();

    const groupedItems = new Map();
    for (const item of navItems) {
        const category = item.category || 'Configuration';
        if (!groupedItems.has(category)) groupedItems.set(category, []);
        groupedItems.get(category).push(item);
    }

    const nav = el('nav', { class: 'sidebar-nav', 'aria-label': 'Main' },
        ...[...groupedItems].flatMap(([category, items]) => [
            el('div', { class: 'sidebar-section-label' }, category),
            ...items.map((item) => {
                // preventDefault: the real `href` is there for right-click/"open
                // in new tab"/accessibility, but a plain click must go through
                // onNavigate() so the router's unsaved-changes guard runs first.
                const link = el('a', {
                    href: `#/${item.id}`,
                    class: 'sidebar-link',
                    'data-route': item.id,
                    onclick: (e) => {
                        if (e.metaKey || e.ctrlKey || e.shiftKey || e.button === 1) return;
                        e.preventDefault();
                        onNavigate && onNavigate(item.id);
                    },
                }, icon(item.icon), el('span', { class: 'label' }, item.label));
                links.set(item.id, link);
                return link;
            }),
        ]));

    // Collapse/expand control, owned by the sidebar. The chevron flips
    // direction via CSS when the shell gets `sidebar-collapsed` (layout.css).
    const toggleBtn = el('button', {
        type: 'button',
        class: 'sidebar-toggle',
        title: 'Collapse navigation',
        'aria-label': 'Collapse navigation',
        'aria-expanded': 'true',
    }, icon('chevronLeft', { size: 16 }));
    if (onToggle) toggleBtn.addEventListener('click', onToggle);

    const footer = el('div', { class: 'sidebar-footer' }, toggleBtn);
    const element = el('aside', { class: 'sidebar', id: 'sidebar' }, nav, footer);

    function setActive(routeId) {
        for (const [id, link] of links) {
            const isActive = id === routeId;
            link.classList.toggle('active', isActive);
            if (isActive) link.setAttribute('aria-current', 'page');
            else link.removeAttribute('aria-current');
        }
    }

    // setCollapsed(bool) — app.js calls this after toggling the shell class
    // so the button's label flips (Collapse ↔ Expand navigation). Mobile
    // ignores it: the drawer header is hidden and the topbar hamburger opens it.
    function setCollapsed(collapsed) {
        toggleBtn.title = collapsed ? 'Expand navigation' : 'Collapse navigation';
        toggleBtn.setAttribute('aria-label', toggleBtn.title);
        toggleBtn.setAttribute('aria-expanded', String(!collapsed));
    }

    return { element, setActive, setCollapsed };
}