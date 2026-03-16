/*
 * Star Control - Star Citizen Linux Manager
 * Copyright (C) 2024-2026 TomRhodan <tomrhodan@gmail.com>
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

/**
 * Star Control - Settings Page
 *
 * This module manages the application settings:
 * - Base directory path for the Star Citizen installation
 * - Log level selection (Debug, Info, Warning, Error)
 * - GitHub token management (to increase API rate limits)
 * - Application reset (delete all data and restart)
 *
 * @module pages/settings
 */

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { confirm } from '../utils/dialogs.js';
import { escapeHtml } from '../utils.js';

/** @type {Object|null} Current application configuration, loaded during rendering */
let config = null;

/**
 * Renders the settings page into the provided container.
 * First loads the current configuration from the Rust backend,
 * then renders the HTML and attaches event listeners.
 *
 * @param {HTMLElement} container - The container element to render into
 */
export async function renderSettings(container) {
  try {
    config = await invoke('load_config');
  } catch (e) {
    // Fallback configuration if loading fails
    config = { install_path: '', log_level: 'info' };
  }

  container.innerHTML = `
    <div class="page-header">
      <h1>Settings</h1>
      <p class="page-subtitle">Configure application paths and preferences</p>
    </div>
    ${renderAppSettings()}
  `;

  attachSettingsEventListeners();
}

/**
 * Obfuscates a GitHub token for secure display.
 * Shows only the first 4 and last 4 characters, replacing the rest with asterisks.
 *
 * @param {string} token - The token to obfuscate
 * @returns {string} The obfuscated token (e.g. "ghp_****xxxx")
 */
function obfuscateToken(token) {
  if (!token) return '';
  if (token.length <= 8) return '*'.repeat(token.length);
  return token.slice(0, 4) + '*'.repeat(token.length - 8) + token.slice(-4);
}

/**
 * Generates the HTML for all settings sections:
 * - Paths (base directory + automatically derived Star Citizen path)
 * - Application (log level, GitHub token)
 * - Danger zone (reset button to reset the entire application)
 *
 * @returns {string} The generated HTML
 */
function renderAppSettings() {
  const basePath = config?.install_path || '';
  const logLevel = config?.log_level || 'info';
  const hasToken = !!config?.github_token;
  const uiScale = config?.ui_scale ?? 1.0;

  return `
    <!-- Path settings: Base directory and derived SC path -->
    <div class="card">
      <h3>Paths</h3>
      <div class="settings-group">
        <div class="setting-row">
          <label class="setting-label" data-tooltip="Root directory for Star Citizen Wine prefix and runners" data-tooltip-pos="right">Base Directory</label>
          <div class="setting-input path-input-row">
            <input type="text" class="input" id="setting-install-path" value="${escapeHtml(basePath)}" placeholder="~/Games/star-citizen" aria-label="Base directory path" />
            <button class="btn btn-secondary" id="btn-browse-path">Browse</button>
          </div>
        </div>
        <!-- Automatically calculated path to the Star Citizen directory within the Wine prefix -->
        <div class="setting-row">
          <label class="setting-label">Star Citizen</label>
          <div class="setting-input">
            <input type="text" class="input" id="setting-wine-prefix" value="${escapeHtml(basePath)}/drive_c/Program Files/Roberts Space Industries/StarCitizen" readonly aria-label="Star Citizen install path" />
          </div>
        </div>
      </div>
    </div>
    <!-- Application settings: Log level and GitHub token -->
    <div class="card">
      <h3>Application</h3>
      <div class="settings-group">
        <!-- Log level selection controls the verbosity of log output -->
        <div class="setting-row">
          <label class="setting-label" data-tooltip="Controls verbosity of application log output" data-tooltip-pos="right">Log Level</label>
          <div class="setting-input">
            <select class="input" id="setting-log-level" aria-label="Log level">
              <option value="debug" ${logLevel === 'debug' ? 'selected' : ''}>Debug</option>
              <option value="info" ${logLevel === 'info' ? 'selected' : ''}>Info</option>
              <option value="warn" ${logLevel === 'warn' ? 'selected' : ''}>Warning</option>
              <option value="error" ${logLevel === 'error' ? 'selected' : ''}>Error</option>
            </select>
          </div>
        </div>
        <!-- UI Scale slider: Controls the overall UI scaling factor -->
        <div class="setting-row">
          <label class="setting-label" data-tooltip="Scale the user interface (50% - 200%)" data-tooltip-pos="right">UI Scale</label>
          <div class="setting-input">
            <div class="slider-row">
              <input type="range" class="slider" id="setting-ui-scale" min="0.5" max="2.0" step="0.1" value="${uiScale}" aria-label="UI scale" />
              <span class="slider-value" id="ui-scale-value">${Math.round(uiScale * 100)}%</span>
            </div>
          </div>
        </div>
        <!-- GitHub token: Optional, only needed for rate limit issues -->
        <div class="setting-row">
          <label class="setting-label" data-tooltip="Star Control downloads Wine runners and DXVK from GitHub. Without a token, GitHub allows 60 requests/hour per IP. This is enough for normal use - you only need a token if you hit rate limits, e.g. during development or when many users share one IP." data-tooltip-pos="right">GitHub Token</label>
          <div class="setting-input">
            <div class="token-field" id="token-field">
              ${hasToken ? `
                <!-- Token exists: Obfuscated display with edit/delete actions -->
                <div class="token-display">
                  <code class="token-value">${obfuscateToken(config.github_token)}</code>
                  <div class="token-actions">
                    <button class="btn btn-secondary btn-sm" id="btn-token-edit" title="Replace token">
                      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
                    </button>
                    <button class="btn btn-secondary btn-sm" id="btn-token-delete" title="Remove token">
                      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/></svg>
                    </button>
                  </div>
                </div>
              ` : `
                <!-- No token present: Input field for saving a new token -->
                <div class="token-input-row">
                  <input type="password" class="input" id="token-input" placeholder="ghp_xxxxxxxxxxxx" autocomplete="off" aria-label="GitHub token" />
                  <button class="btn btn-primary btn-sm" id="btn-token-save">Save</button>
                </div>
                <p class="setting-hint">Optional. Only needed if you exceed GitHub's 60 requests/hour limit.</p>
              `}
            </div>
          </div>
        </div>
      </div>
    </div>
    <!-- Save button for path and log level changes -->
    <div class="settings-actions">
      <button class="btn btn-primary" id="btn-save-app-settings">Save Settings</button>
    </div>

    <!-- Danger zone: Complete reset of the application and all data -->
    <div class="card card-danger">
      <h3>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align: -2px; margin-right: 6px;">
          <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
          <line x1="12" y1="9" x2="12" y2="13"/>
          <line x1="12" y1="17" x2="12.01" y2="17"/>
        </svg>
        Reset Application
      </h3>
      <p class="danger-description">
        This will permanently delete the entire Star Citizen installation, including the Wine prefix,
        runners, and all game files. The app configuration and cache will be removed.
        Star Control will restart and return to the initial setup.
      </p>
      <!-- Checklist of what happens during reset -->
      <ul class="danger-checklist">
        <li>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/></svg>
          Delete installation directory <span class="text-muted">(${escapeHtml(basePath)})</span>
        </li>
        <li>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="9" y1="15" x2="15" y2="15"/></svg>
          Remove app config and cache
        </li>
        <li>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/></svg>
          Restart Star Control (setup wizard)
        </li>
        <!-- Positive point: Token will be preserved -->
        <li class="danger-keep-item">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
          GitHub token will be preserved
        </li>
      </ul>
      <div id="reset-confirm-area">
        <button class="btn btn-danger" id="btn-reset-app">Reset &amp; Restart</button>
      </div>
    </div>
  `;
}

/**
 * Saves the current settings (path + log level).
 * Validates the installation path via the backend before saving.
 */
async function saveAppSettings() {
  const installPath = document.getElementById('setting-install-path')?.value || '';
  const logLevel = document.getElementById('setting-log-level')?.value || 'info';
  const uiScale = parseFloat(document.getElementById('setting-ui-scale')?.value) || 1.0;

  try {
    // Validate path via the Rust backend (existence, write permissions, disk space)
    const validation = await invoke('validate_install_path', { path: installPath });
    if (!validation.valid) {
      showNotification(validation.message, 'error');
      return;
    }

    // Build configuration with updated values and save
    const newConfig = { ...config, install_path: installPath, log_level: logLevel, ui_scale: uiScale };
    await invoke('save_config', { config: newConfig });
    config = newConfig;

    // Apply UI scale immediately (with XWayland compensation if available)
    applyUiScale(uiScale, window.__xwaylandCompensation || 1.0);

    showNotification('Settings saved', 'success');
  } catch (e) {
    showNotification('Failed to save settings', 'error');
  }
}

/**
 * Attaches all event listeners for the settings page:
 * - Browse button for directory selection
 * - Path input for automatic update of the derived path
 * - Save button
 * - Token management
 * - Reset button with confirmation dialog
 */
function attachSettingsEventListeners() {
  // Open directory selection dialog via the Tauri dialog plugin
  document.getElementById('btn-browse-path')?.addEventListener('click', async () => {
    try {
      const selected = await open({ directory: true, title: 'Select Star Citizen Base Directory' });
      if (selected) {
        document.getElementById('setting-install-path').value = selected;
        updateDerivedPaths(selected);
      }
    } catch (e) { console.error(e); }
  });

  // Automatically update the derived SC path when the base path changes
  document.getElementById('setting-install-path')?.addEventListener('input', (e) => updateDerivedPaths(e.target.value));
  document.getElementById('btn-save-app-settings')?.addEventListener('click', saveAppSettings);

  // UI Scale slider: Update displayed percentage value in real-time
  document.getElementById('setting-ui-scale')?.addEventListener('input', (e) => {
    const value = Math.round(e.target.value * 100);
    document.getElementById('ui-scale-value').textContent = value + '%';
  });

  // Attach token event listeners separately
  attachTokenListeners();

  // Reset button: Shows confirmation dialog, then deletes everything and reloads the page
  document.getElementById('btn-reset-app')?.addEventListener('click', async () => {
    const confirmed = await confirm(
      'Are you sure? This will delete the entire installation, Wine prefix, and game files. This cannot be undone.',
      { title: 'Reset Application', kind: 'danger', okLabel: 'Yes, delete everything' }
    );

    if (!confirmed) return;

    const btn = document.getElementById('btn-reset-app');
    btn.disabled = true;
    btn.textContent = 'Resetting...';

    try {
      // Rust backend performs the reset (delete directories, remove config)
      await invoke('reset_app');
      // Fully reload the page to start the setup wizard
      window.location.reload();
    } catch (e) {
      showNotification('Reset failed: ' + e, 'error');
      btn.disabled = false;
      btn.textContent = 'Reset & Restart';
    }
  });
}

/**
 * Attaches event listeners for token management:
 * - Save new token (button click or Enter key)
 * - Edit existing token (switches to input mode)
 * - Delete token (sends empty string as signal to remove)
 */
function attachTokenListeners() {
  const field = document.getElementById('token-field');
  if (!field) return;

  // Save token via button or Enter key
  document.getElementById('btn-token-save')?.addEventListener('click', () => saveToken());
  document.getElementById('token-input')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') saveToken();
  });

  // Edit: Switches the display from the obfuscated token to the input field
  document.getElementById('btn-token-edit')?.addEventListener('click', () => {
    field.innerHTML = `
      <div class="token-input-row">
        <input type="password" class="input" id="token-input" placeholder="ghp_xxxxxxxxxxxx" autocomplete="off" aria-label="GitHub token" />
        <button class="btn btn-primary btn-sm" id="btn-token-save">Save</button>
        <button class="btn btn-secondary btn-sm" id="btn-token-cancel">Cancel</button>
      </div>
    `;
    document.getElementById('token-input').focus();
    document.getElementById('btn-token-save').addEventListener('click', () => saveToken());
    document.getElementById('token-input').addEventListener('keydown', (e) => {
      if (e.key === 'Enter') saveToken();
      if (e.key === 'Escape') refreshTokenField();
    });
    document.getElementById('btn-token-cancel').addEventListener('click', () => refreshTokenField());
  });

  // Delete: Empty string signals the backend to remove the token
  document.getElementById('btn-token-delete')?.addEventListener('click', async () => {
    try {
      await invoke('save_config', { config: { ...config, github_token: '' } });
      config = { ...config, github_token: null };
      showNotification('GitHub token removed', 'success');
      refreshTokenField();
    } catch (e) {
      showNotification('Failed to remove token', 'error');
    }
  });
}

/**
 * Saves a new GitHub token.
 * Validates that a token was entered, saves it to the configuration,
 * and updates the display.
 */
async function saveToken() {
  const input = document.getElementById('token-input');
  const token = input?.value.trim();
  if (!token) {
    showNotification('Please enter a token', 'error');
    return;
  }

  try {
    config = { ...config, github_token: token };
    await invoke('save_config', { config });
    showNotification('GitHub token saved', 'success');
    refreshTokenField();
  } catch (e) {
    showNotification('Failed to save token', 'error');
  }
}

/**
 * Updates the token display based on the current configuration state.
 * Shows either the obfuscated token with action buttons or the input field.
 * Re-attaches token event listeners afterwards.
 */
function refreshTokenField() {
  const field = document.getElementById('token-field');
  if (!field) return;
  const hasToken = !!config?.github_token;

  field.innerHTML = hasToken ? `
    <div class="token-display">
      <code class="token-value">${obfuscateToken(config.github_token)}</code>
      <div class="token-actions">
        <button class="btn btn-secondary btn-sm" id="btn-token-edit" title="Replace token">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>
        </button>
        <button class="btn btn-secondary btn-sm" id="btn-token-delete" title="Remove token">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/></svg>
        </button>
      </div>
    </div>
  ` : `
    <div class="token-input-row">
      <input type="password" class="input" id="token-input" placeholder="ghp_xxxxxxxxxxxx" autocomplete="off" aria-label="GitHub token" />
      <button class="btn btn-primary btn-sm" id="btn-token-save">Save</button>
    </div>
    <p class="setting-hint">Optional. Only needed if you exceed GitHub's 60 requests/hour limit.</p>
  `;

  // Event listeners must be re-attached after every innerHTML replacement
  attachTokenListeners();
}

/**
 * Updates the derived Star Citizen path based on the base path.
 * The SC path is automatically calculated as a subdirectory within the Wine prefix.
 *
 * @param {string} basePath - The base path of the installation
 */
function updateDerivedPaths(basePath) {
  document.getElementById('setting-wine-prefix').value = basePath + '/drive_c/Program Files/Roberts Space Industries/StarCitizen';
}


/**
 * Shows a temporary notification at the bottom of the screen.
 * The notification is automatically hidden and removed after 3 seconds.
 *
 * @param {string} message - The message to display
 * @param {string} type - The notification type ('info', 'success', 'error')
 */
function showNotification(message, type = 'info') {
  // Remove existing notification to avoid overlap
  const existing = document.querySelector('.settings-notification');
  if (existing) existing.remove();
  const notification = document.createElement('div');
  notification.className = `settings-notification notification-${type}`;
  notification.textContent = message;
  document.body.appendChild(notification);
  // Short delay for CSS transition (fade-in animation)
  setTimeout(() => notification.classList.add('show'), 10);
  // Fade out after 3 seconds and remove DOM element
  setTimeout(() => { notification.classList.remove('show'); setTimeout(() => notification.remove(), 300); }, 3000);
}

/**
 * Applies UI scale to the application.
 * Scales the html element font-size; all rem-based sizes follow.
 *
 * @param {number} scale - The scale factor (e.g. 1.0 = 100%, 1.5 = 150%)
 */
/**
 * Applies UI scale to the application.
 * Scales the html element font-size; all rem-based sizes follow.
 *
 * @param {number} scale - The scale factor (e.g. 1.0 = 100%, 1.5 = 150%)
 * @param {number} [compensation=1.0] - XWayland zoom compensation factor.
 *   When set_zoom(comp) is active, text appears larger than native because
 *   zoom scales everything uniformly. This divides the base to compensate.
 */
export function applyUiScale(scale, compensation = 1.0) {
  // 14px is the design base from main.css (html { font-size: 14px })
  // Direct pixel calculation avoids the browser-default-16px bug with percentages
  //
  // Under XWayland, set_zoom(comp) makes text appear larger than native rendering.
  // Empirically, comp=1.5 needs ~80% font correction to match native appearance.
  // Formula: lerp between 1.0 (no correction) and 1/comp (full correction) at 60%
  // -> correction = 1 - (1 - 1/comp) * 0.6
  // -> comp=1.5: 1 - (1 - 0.667) * 0.6 = 1 - 0.2 = 0.8
  const correction = compensation > 1.0
    ? 1 - (1 - 1 / compensation) * 0.6
    : 1.0;
  document.documentElement.style.fontSize = `${14 * scale * correction}px`;
}
