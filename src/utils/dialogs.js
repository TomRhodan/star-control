/**
 * Star Control - Custom Dialog Utilities
 *
 * Provides a beautifully integrated replacement for native browser/Tauri dialogs.
 */

import { escapeHtml } from '../utils.js';

/**
 * Shows a custom confirmation modal.
 *
 * @param {string} message - The message to display.
 * @param {Object} [options] - Configuration options.
 * @param {string} [options.title='Confirm'] - Modal title.
 * @param {string} [options.kind='info'] - 'info', 'warning', or 'danger'.
 * @param {string} [options.okLabel='Confirm'] - Label for the OK button.
 * @param {string} [options.cancelLabel='Cancel'] - Label for the Cancel button.
 * @returns {Promise<boolean>} Resolves to true if confirmed, false otherwise.
 */
export function confirm(message, options = {}) {
  const {
    title = 'Confirm',
    kind = 'info',
    okLabel = 'Confirm',
    cancelLabel = 'Cancel'
  } = options;

  return new Promise((resolve) => {
    // Create modal elements
    const overlay = document.createElement('div');
    overlay.className = 'modal-overlay';
    
    // Determine icon based on kind
    let icon = '';
    if (kind === 'warning' || kind === 'danger') {
      icon = `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="modal-icon-${kind}"><path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>`;
    } else {
      icon = `<svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="modal-icon-info"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>`;
    }

    overlay.innerHTML = `
      <div class="modal-container modal-kind-${kind}">
        <div class="modal-header">
          <div class="modal-title-wrap">
            ${icon}
            <h3>${escapeHtml(title)}</h3>
          </div>
        </div>
        <div class="modal-body">
          <p>${escapeHtml(message).replace(/\n/g, '<br>')}</p>
        </div>
        <div class="modal-footer">
          <button class="btn btn-secondary" id="modal-cancel">${escapeHtml(cancelLabel)}</button>
          <button class="btn ${kind === 'danger' ? 'btn-danger' : 'btn-primary'}" id="modal-ok">${escapeHtml(okLabel)}</button>
        </div>
      </div>
    `;

    document.body.appendChild(overlay);

    // Fade in
    requestAnimationFrame(() => {
      overlay.classList.add('show');
    });

    const cleanup = (result) => {
      overlay.classList.remove('show');
      setTimeout(() => {
        overlay.remove();
        resolve(result);
      }, 200);
    };

    overlay.querySelector('#modal-cancel').addEventListener('click', () => cleanup(false));
    overlay.querySelector('#modal-ok').addEventListener('click', () => cleanup(true));
    
    // Close on overlay click (optional, but good for UX usually)
    overlay.addEventListener('click', (e) => {
      if (e.target === overlay) cleanup(false);
    });

    // Handle Escape key
    const handleEscape = (e) => {
      if (e.key === 'Escape') {
        window.removeEventListener('keydown', handleEscape);
        cleanup(false);
      }
    };
    window.addEventListener('keydown', handleEscape);
  });
}
