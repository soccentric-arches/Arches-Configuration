// Shared helpers for manifest-driven hardware pages.

export function peripheralsOf(data, kind) {
    return (data.manifest || []).filter(peripheral => peripheral.kind === kind);
}

export async function saveIo(api, data, updates) {
    // If-Match against the fingerprint this page loaded (io_version from
    // GET /io) — the server rejects the write with 409 if another editor's
    // change landed first, instead of silently clobbering it.
    const extraHeaders = data.io_version ? { 'If-Match': data.io_version } : undefined;
    await api.put('/api/v1/io', {
        serial: data.config && data.config.serial || [],
        serial_enabled: data.config && data.config.serial_enabled !== false,
        can: data.config && data.config.can || [],
        can_enabled: data.config && data.config.can_enabled !== false,
        gpio: data.config && data.config.gpio || [],
        gpio_enabled: data.config && data.config.gpio_enabled !== false,
        cameras: data.config && data.config.cameras || [],
        cameras_enabled: data.config && data.config.cameras_enabled !== false,
        mcu: data.config && data.config.mcu || null,
        board_leds: data.config && data.config.board_leds || {},
        ...updates,
    }, extraHeaders);
}

// setControlsEnabled(controls, on) — enable/disable the wrapped inputs of
// Toggle/Field widgets (both expose `.input`). Used by the hardware pages to
// lock configuration until the owning subsystem/peripheral is enabled: the
// server rejects configuration writes for anything the request leaves
// disabled, so the UI mirrors that contract.
export function setControlsEnabled(controls, on) {
    for (const control of controls) {
        if (control && control.input) control.input.disabled = !on;
    }
}

// gateRows(rows, subsystemOn) — lock each row's controls: the row's own
// enable toggle is usable only when the subsystem is on; the row's remaining
// controls only when the subsystem AND the entry are enabled. `controls(row)`
// returns the per-row controls to lock beyond the enable toggle.
export function wireEntryGate({ subsystem, rows, onChange, controls }) {
    const sync = () => {
        const sub = subsystem.get();
        for (const row of rows) {
            if (row.enabled && row.enabled.input) row.enabled.input.disabled = !sub;
            setControlsEnabled(controls(row), sub && row.enabled.get());
        }
    };
    subsystem.input.addEventListener('change', sync);
    for (const row of rows) {
        if (row.enabled && row.enabled.input) row.enabled.input.addEventListener('change', sync);
    }
    sync();
    return sync;
}]
