import { el } from '../core/dom.js';
import { Button } from './button.js';

const FOCUSABLE = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';
let modalSeq = 0;

/**
 * Low-level modal primitive. Returns { close, dialog }. Traps focus inside
 * the dialog while open, restores it to whatever was focused before opening,
 * and closes on Escape or a scrim click — the same contract native <dialog>
 * gives you, without relying on it (Safari/older-browser gaps).
 */
function openModal({ title, body, footer }) {
  const titleId = `modal-title-${++modalSeq}`;
  const previouslyFocused = document.activeElement;

  const scrim = el('div', { class: 'modal-scrim' });
  const dialog = el('div', { class: 'modal', role: 'dialog', 'aria-modal': 'true', 'aria-labelledby': title ? titleId : null, tabindex: '-1' },
    title ? el('div', { class: 'modal-header' }, el('h2', { id: titleId }, title)) : null,
    el('div', { class: 'modal-body' }, body),
    footer ? el('div', { class: 'modal-footer' }, ...footer) : null);
  scrim.appendChild(dialog);
  scrim.addEventListener('mousedown', (e) => { if (e.target === scrim) close(); });

  function onKey(e) {
    if (e.key === 'Escape') { close(); return; }
    if (e.key !== 'Tab') return;
    const focusable = [...dialog.querySelectorAll(FOCUSABLE)].filter(el => !el.disabled);
    if (!focusable.length) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  }
  document.addEventListener('keydown', onKey);
  document.body.appendChild(scrim);

  const firstField = dialog.querySelector('input, select, textarea');
  (firstField || dialog).focus();

  function close() {
    document.removeEventListener('keydown', onKey);
    scrim.remove();
    if (previouslyFocused && previouslyFocused.focus) previouslyFocused.focus();
  }
  return { close, dialog };
}

/**
 * confirmDialog({ title, description, confirmLabel, cancelLabel, variant })
 * -> Promise<boolean>. Replaces the native, thread-blocking `confirm()`.
 */
export function confirmDialog({ title = 'Are you sure?', description, confirmLabel = 'Confirm', cancelLabel = 'Cancel', variant = 'primary' } = {}) {
  return new Promise((resolve) => {
    let modal;
    const finish = (v) => { modal.close(); resolve(v); };
    modal = openModal({
      title,
      body: description ? el('p', { class: 'muted' }, description) : null,
      footer: [
        Button({ variant: 'outline', onClick: () => finish(false) }, cancelLabel),
        Button({ variant, onClick: () => finish(true) }, confirmLabel),
      ],
    });
  });
}

/**
 * choiceDialog({ title, description, choices }) -> Promise<string|null>.
 * Each choice is { value, label, variant }. Closing the dialog returns null.
 */
export function choiceDialog({ title, description, choices = [] } = {}) {
  return new Promise((resolve) => {
    let modal;
    const finish = (value) => { modal.close(); resolve(value); };
    modal = openModal({
      title,
      body: description ? el('p', { class: 'muted' }, description) : null,
      footer: choices.map(choice => Button({
        variant: choice.variant || 'primary',
        onClick: () => finish(choice.value),
      }, choice.label)),
    });
  });
}

/**
 * promptDialog({ title, label, placeholder, type, confirmLabel })
 * -> Promise<string|null>. Replaces the native `prompt()`.
 */
export function promptDialog({ title = 'Enter a value', label, placeholder, type = 'text', confirmLabel = 'Continue' } = {}) {
  return new Promise((resolve) => {
    const input = el('input', { class: 'input', type, placeholder, autofocus: true });
    let modal;
    const finish = (v) => { modal.close(); resolve(v); };
    input.addEventListener('keydown', (e) => { if (e.key === 'Enter') finish(input.value); });
    modal = openModal({
      title,
      body: el('label', { class: 'field' }, label ? el('span', { class: 'field-label' }, label) : null, input),
      footer: [
        Button({ variant: 'outline', onClick: () => finish(null) }, 'Cancel'),
        Button({ onClick: () => finish(input.value) }, confirmLabel),
      ],
    });
  });
}

/**
 * formDialog({ title, fields: [{ key, label, type, placeholder }], confirmLabel })
 * -> Promise<Record<string,string>|null>. Multi-field variant of promptDialog,
 * used where a native prompt() chain would otherwise be needed (e.g. add user).
 */
export function formDialog({ title = 'New', fields = [], confirmLabel = 'Create' } = {}) {
  return new Promise((resolve) => {
    const inputs = fields.map(f => ({ key: f.key, input: el('input', { class: 'input', type: f.type || 'text', placeholder: f.placeholder }) }));
    let modal;
    const finish = (v) => { modal.close(); resolve(v); };
    const body = el('div', { class: 'stack' }, ...fields.map((f, i) =>
      el('label', { class: 'field' }, el('span', { class: 'field-label' }, f.label), inputs[i].input)));
    modal = openModal({
      title,
      body,
      footer: [
        Button({ variant: 'outline', onClick: () => finish(null) }, 'Cancel'),
        Button({ onClick: () => finish(Object.fromEntries(inputs.map(i => [i.key, i.input.value]))) }, confirmLabel),
      ],
    });
  });
}

/**
 * infoDialog({ title, body }) -> Promise<void>. Display-only content (e.g.
 * the keyboard shortcuts cheat sheet) with a single dismiss button.
 */
export function infoDialog({ title, body, closeLabel = 'Close' } = {}) {
  return new Promise((resolve) => {
    let modal;
    modal = openModal({ title, body, footer: [Button({ onClick: () => { modal.close(); resolve(); } }, closeLabel)] });
  });
}
