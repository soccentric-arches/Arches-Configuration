// Read-only secure boot and platform trust status.

import { el } from '../core/dom.js';
import { Card } from '../components/card.js';
import { Badge } from '../components/badge.js';
import { DataItem } from '../components/data-item.js';

export async function render(api) {
    const trust = await api.get('/api/v1/security/trust');
    // `platform_status_available` distinguishes "the BSP reported these
    // values" from "no platform-status source exists on this board yet" —
    // never render the latter as if it were a completed, merely-empty read.
    const veritySection = trust.platform_status_available
        ? DataItem(`${trust.verity_status ?? 'unreported'} · ${trust.encryption_status ?? 'unreported'}`, 'Verity and encryption', 'Read-only integrity and storage protection states.')
        : DataItem(Badge('not available on this platform', 'neutral'), 'Verity and encryption', 'Read-only integrity and storage protection states.');
    const signingKeySection = trust.platform_status_available
        && DataItem(trust.update_signing_key_id ?? 'unreported', 'Update signing key', 'Key ID that signed the currently installed update.');
    const trustBody = el('div', { class: 'field-row' },
        DataItem(Badge(trust.secure_boot ? 'on' : 'off', trust.secure_boot ? 'success' : 'destructive'), 'Secure boot', 'Firmware verification state reported by the platform.'),
        DataItem(Badge(trust.tpm_present ? 'present' : 'absent', trust.tpm_present ? 'success' : 'neutral'), 'TPM', 'Whether a trusted platform module is available.'),
        veritySection,
        signingKeySection);
    return el('div', {},
        el('div', { class: 'page-header' }, el('h1', {}, 'Secure Boot'), el('p', {}, 'Read-only platform integrity and trusted hardware status.')),
        Card({ title: 'Trust status', subtitle: 'Secure boot, TPM, verity, and encryption posture reported by the device.' }, trustBody));
}
