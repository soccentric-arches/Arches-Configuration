// Shared helpers for manifest-driven hardware pages.

export function peripheralsOf(data, kind) {
    return (data.manifest || []).filter(peripheral => peripheral.kind === kind);
}

export async function saveIo(api, data, updates) {
    await api.put('/api/v1/io', {
        serial: data.config && data.config.serial || [],
        can: data.config && data.config.can || [],
        gpio: data.config && data.config.gpio || [],
        mcu: data.config && data.config.mcu || null,
        board_leds: data.config && data.config.board_leds || {},
        ...updates,
    });
}
