// Read-only device overview: the first page shows health and identity while
// editable configuration remains under System settings and Networking.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Badge } from '../components/badge.js';

function fmtBytes(bytes) {
    if (!bytes) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let value = bytes;
    let index = 0;
    while (value >= 1024 && index < units.length - 1) { value /= 1024; index++; }
    return `${value.toFixed(index ? 1 : 0)} ${units[index]}`;
}

function fmtUptime(seconds) {
    if (!seconds) return '—';
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    return [days && `${days}d`, hours && `${hours}h`, `${minutes}m`].filter(Boolean).join(' ');
}

function metric(label, explanation) {
    const value = el('div', { class: 'tile-value' }, '—');
    return {
        node: el('div', { class: 'card overview-metric' }, el('div', { class: 'card-body' }, el('div', { class: 'overview-metric-label' }, label), el('div', { class: 'tile-sub' }, explanation), value)),
        update(text) { value.textContent = text; },
    };
}

function usagePercent(total, available) {
    return total ? Math.max(0, Math.min(100, (1 - available / total) * 100)) : null;
}

function statusBadge(operstate) {
    const active = operstate === 'up';
    return Badge(operstate || 'unknown', active ? 'success' : 'neutral');
}

function renderStorage(vitals) {
    if (!vitals.mounts || !vitals.mounts.length) {
        return el('div', { class: 'muted' }, 'No storage status reported.');
    }
    return el('div', { class: 'stack' }, ...vitals.mounts.map((mount) => {
        const used = usagePercent(mount.total_bytes, mount.avail_bytes);
        return el('div', { class: 'field-row' },
            el('div', {}, el('div', { class: 'field-label' }, mount.target), el('div', { class: 'field-help' }, 'Mounted filesystem and source type.'), el('div', {}, mount.source || mount.fstype)),
            el('div', {}, el('div', { class: 'field-label' }, 'Used'), el('div', { class: 'field-help' }, 'Share of this filesystem currently occupied.'), el('div', {}, used == null ? '—' : `${used.toFixed(0)}%`)),
            el('div', {}, el('div', { class: 'field-label' }, 'Available'), el('div', { class: 'field-help' }, 'Space available for files and services.'), el('div', {}, fmtBytes(mount.avail_bytes))),
        );
    }));
}

function renderInterfaces(network) {
    const configured = (network.config && network.config.interfaces) || [];
    const live = network.live || [];
    if (!live.length) return el('div', { class: 'muted' }, 'No network interfaces reported.');

    return el('div', { class: 'stack' }, ...live.map((iface) => {
        const config = configured.find(item => item.name === iface.name);
        const addresses = (iface.addresses && iface.addresses.length)
            ? iface.addresses
            : (config && config.address ? [`${config.address}${config.prefix ? `/${config.prefix}` : ''}`] : []);
        return el('div', { class: 'field-row' },
            el('div', {}, el('div', { class: 'field-label' }, iface.name), el('div', { class: 'field-help' }, 'Interface name and current link state.'), statusBadge(iface.operstate)),
            el('div', {}, el('div', { class: 'field-label' }, 'IP addresses'), el('div', { class: 'field-help' }, 'Addresses assigned for network communication.'), el('div', {}, addresses.length ? addresses.join(', ') : '—')),
            el('div', {}, el('div', { class: 'field-label' }, 'MAC'), el('div', { class: 'field-help' }, 'Hardware address used on the local network.'), el('div', {}, iface.mac || '—')),
        );
    }));
}

export async function render(api, store) {
    const [config, info, vitals, network] = await Promise.all([
        api.get('/api/v1/system'),
        api.get('/api/v1/system/info').catch(() => null),
        api.get('/api/v1/monitoring/vitals').catch(() => ({})),
        api.get('/api/v1/network').catch(() => ({ live: [], config: {} })),
    ]);

    const cpu = metric('CPU load (1m)', 'Average runnable work over the last minute; higher means more CPU demand.');
    const memory = metric('Memory used', 'Percentage of RAM currently unavailable for new work.');
    const temperature = metric('Highest temperature', 'Highest sensor reading currently reported by the board.');
    const uptime = metric('Uptime', 'Time elapsed since the operating system last booted.');
    const connectionBadge = Badge('online', 'success');

    function setConnectionState(online) {
        connectionBadge.textContent = online ? 'online' : 'offline';
        connectionBadge.className = `badge badge-${online ? 'success' : 'destructive'}`;
    }

    function updateVitals(snapshot) {
        const load = snapshot.load && snapshot.load[0];
        cpu.update(load == null ? '—' : load.toFixed(2));
        const memoryUsed = usagePercent(snapshot.mem_total_kb, snapshot.mem_available_kb);
        memory.update(memoryUsed == null ? '—' : `${memoryUsed.toFixed(0)}%`);
        const temperatures = (snapshot.temperatures || []).map(item => item.temp_millideg_c / 1000);
        const hottest = temperatures.length ? Math.max(...temperatures) : null;
        temperature.update(hottest == null ? '—' : `${hottest.toFixed(0)} C`);
        uptime.update(fmtUptime(snapshot.uptime_secs));
    }
    updateVitals(vitals);

    const source = api.eventSource('/api/v1/events');
    source.addEventListener('vitals', (event) => {
        try { updateVitals(JSON.parse(event.data)); setConnectionState(true); } catch (_) { /* skip malformed tick */ }
    });
    source.addEventListener('error', () => setConnectionState(false));
    store.registerCleanup(() => source.close());

    const refresh = async () => {
        try {
            updateVitals(await api.get('/api/v1/monitoring/vitals'));
            setConnectionState(true);
        } catch (_) {
            setConnectionState(false);
        }
    };
    store.registerTimer(setInterval(refresh, 5000));

    const identity = Card({ title: 'Device identity', subtitle: 'Hardware and board identifiers reported by the device.' },
        el('div', { class: 'overview-detail-grid' },
            el('div', {}, el('div', { class: 'field-label' }, 'Hostname'), el('div', { class: 'field-help' }, 'Name used to identify this device on the network.'), el('div', {}, config.hostname || '—')),
            el('div', {}, el('div', { class: 'field-label' }, 'Platform'), el('div', { class: 'field-help' }, 'Board family and manifest selected for this device.'), info ? Badge(info.platform || '—', 'info') : '—'),
            el('div', {}, el('div', { class: 'field-label' }, 'Model'), el('div', { class: 'field-help' }, 'Hardware model reported by the platform.'), el('div', {}, info && info.model || '—')),
            el('div', {}, el('div', { class: 'field-label' }, 'Board'), el('div', { class: 'field-help' }, 'Specific board name used for hardware capabilities.'), el('div', {}, info && info.board_name || '—')),
            el('div', {}, el('div', { class: 'field-label' }, 'Hardware revision'), el('div', { class: 'field-help' }, 'Physical board revision identifier.'), el('div', {}, info && info.hardware_revision || '—')),
            el('div', {}, el('div', { class: 'field-label' }, 'Serial number'), el('div', { class: 'field-help' }, 'Inventory identifier assigned to this device.'), el('div', {}, info && info.serial_number || '—')),
        ));

    const software = Card({ title: 'System information', subtitle: 'Operating system, kernel, bootloader, and WebConfig build metadata.' },
        el('div', { class: 'overview-detail-grid' },
            el('div', {}, el('div', { class: 'field-label' }, 'Operating system'), el('div', { class: 'field-help' }, 'Base operating system running on the device.'), el('div', {}, info && info.os_name || '—')),
            el('div', {}, el('div', { class: 'field-label' }, 'OS version'), el('div', { class: 'field-help' }, 'Release version of the installed operating system.'), el('div', {}, info && info.os_version || '—')),
            el('div', {}, el('div', { class: 'field-label' }, 'OS revision'), el('div', { class: 'field-help' }, 'Build or source revision of the operating system.'), el('div', {}, info && info.os_revision || '—')),
            el('div', {}, el('div', { class: 'field-label' }, 'Kernel'), el('div', { class: 'field-help' }, 'Linux kernel version currently running.'), el('div', {}, info && info.kernel_version || '—')),
            el('div', {}, el('div', { class: 'field-label' }, 'Bootloader'), el('div', { class: 'field-help' }, 'Software that starts and selects the operating system.'), el('div', {}, info && info.bootloader_version || '—')),
            el('div', {}, el('div', { class: 'field-label' }, 'WebConfig version'), el('div', { class: 'field-help' }, 'Installed version of this management interface.'), el('div', {}, info && info.software_version || '—')),
            el('div', {}, el('div', { class: 'field-label' }, 'WebConfig revision'), el('div', { class: 'field-help' }, 'Source or build revision of this management interface.'), el('div', {}, info && info.software_revision || '—')),
        ));

    return el('div', {},
        el('div', { class: 'page-header' }, el('h1', {}, 'Overview'), el('p', {}, 'Live system information and device health.')),
        Card({ class: 'overview-hero', title: 'Device health', subtitle: 'A quick read of the device identity and current operating state.' },
            el('div', { class: 'overview-hero-grid' },
                el('div', {}, el('div', { class: 'overview-eyebrow' }, 'Device'), el('div', { class: 'overview-hero-title' }, config.hostname || 'Unnamed device'), el('div', { class: 'muted' }, info && info.board_name || 'Board information unavailable')),
                el('div', {}, el('div', { class: 'overview-eyebrow' }, 'Platform'), info ? Badge(info.platform || 'unknown', 'info') : Badge('unknown', 'neutral'), el('div', { class: 'overview-hero-meta' }, info && info.model || 'Model unavailable')),
                el('div', {}, el('div', { class: 'overview-eyebrow' }, 'Current state'), connectionBadge, el('div', { class: 'overview-hero-meta' }, 'Live telemetry connected')))),
        el('div', { class: 'overview-metrics' }, cpu.node, memory.node, temperature.node, uptime.node),
        el('div', { class: 'card-grid overview-info-grid' }, identity, software),
        Card({ title: 'Storage status', subtitle: 'Mounted filesystems, capacity, and current usage.' }, renderStorage(vitals)),
        Card({ title: 'Network interfaces', subtitle: 'Live link state, addresses, and hardware identifiers.' }, renderInterfaces(network)),
    );
}