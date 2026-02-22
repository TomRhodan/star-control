/**
 * Star Control - Settings Page
 *
 * This module handles application settings:
 * - Base directory path configuration
 * - Log level selection
 * - GitHub token management (for API rate limits)
 * - Application reset (delete all data)
 *
 * @module pages/settings
 */

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

/** @type {Object|null} Current application configuration */
let config = null;

export async function renderSettings(container) {
  try {
    config = await invoke('load_config');
  } catch (e) {
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

function obfuscateToken(token) {
  if (!token) return '';
  if (token.length <= 8) return '*'.repeat(token.length);
  return token.slice(0, 4) + '*'.repeat(token.length - 8) + token.slice(-4);
}

function renderAppSettings() {
  const basePath = config?.install_path || '';
  const logLevel = config?.log_level || 'info';
  const hasToken = !!config?.github_token;

  return `
    <div class="card">
      <h3>Paths</h3>
      <div class="settings-group">
        <div class="setting-row">
          <label class="setting-label" data-tooltip="Root directory for Star Citizen Wine prefix and runners" data-tooltip-pos="right">Base Directory</label>
          <div class="setting-input path-input-row">
            <input type="text" class="input" id="setting-install-path" value="${escapeHtml(basePath)}" placeholder="~/Games/star-citizen" />
            <button class="btn btn-secondary" id="btn-browse-path">Browse</button>
          </div>
        </div>
        <div class="setting-row">
          <label class="setting-label">Star Citizen</label>
          <div class="setting-input">
            <input type="text" class="input" id="setting-wine-prefix" value="${escapeHtml(basePath)}/drive_c/Program Files/Roberts Space Industries/StarCitizen" readonly />
          </div>
        </div>
      </div>
    </div>
    <div class="card">
      <h3>Application</h3>
      <div class="settings-group">
        <div class="setting-row">
          <label class="setting-label" data-tooltip="Controls verbosity of application log output" data-tooltip-pos="right">Log Level</label>
          <div class="setting-input">
            <select class="input" id="setting-log-level">
              <option value="debug" ${logLevel === 'debug' ? 'selected' : ''}>Debug</option>
              <option value="info" ${logLevel === 'info' ? 'selected' : ''}>Info</option>
              <option value="warn" ${logLevel === 'warn' ? 'selected' : ''}>Warning</option>
              <option value="error" ${logLevel === 'error' ? 'selected' : ''}>Error</option>
            </select>
          </div>
        </div>
        <div class="setting-row">
          <label class="setting-label" data-tooltip="Star Control downloads Wine runners and DXVK from GitHub. Without a token, GitHub allows 60 requests/hour per IP. This is enough for normal use — you only need a token if you hit rate limits, e.g. during development or when many users share one IP." data-tooltip-pos="right">GitHub Token</label>
          <div class="setting-input">
            <div class="token-field" id="token-field">
              ${hasToken ? `
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
                  <input type="password" class="input" id="token-input" placeholder="ghp_xxxxxxxxxxxx" autocomplete="off" />
                  <button class="btn btn-primary btn-sm" id="btn-token-save">Save</button>
                </div>
                <p class="setting-hint">Optional. Only needed if you exceed GitHub's 60 requests/hour limit.</p>
              `}
            </div>
          </div>
        </div>
      </div>
    </div>
    <div class="settings-actions">
      <button class="btn btn-primary" id="btn-save-app-settings">Save Settings</button>
    </div>

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

async function saveAppSettings() {
  const installPath = document.getElementById('setting-install-path')?.value || '';
  const logLevel = document.getElementById('setting-log-level')?.value || 'info';

  try {
    const validation = await invoke('validate_install_path', { path: installPath });
    if (!validation.valid) {
      showNotification(validation.message, 'error');
      return;
    }

    const newConfig = { ...config, install_path: installPath, log_level: logLevel };
    await invoke('save_config', { config: newConfig });
    config = newConfig;
    showNotification('Settings saved', 'success');
  } catch (e) {
    showNotification('Failed to save settings', 'error');
  }
}

function attachSettingsEventListeners() {
  document.getElementById('btn-browse-path')?.addEventListener('click', async () => {
    try {
      const selected = await open({ directory: true, title: 'Select Star Citizen Base Directory' });
      if (selected) {
        document.getElementById('setting-install-path').value = selected;
        updateDerivedPaths(selected);
      }
    } catch (e) { console.error(e); }
  });

  document.getElementById('setting-install-path')?.addEventListener('input', (e) => updateDerivedPaths(e.target.value));
  document.getElementById('btn-save-app-settings')?.addEventListener('click', saveAppSettings);

  attachTokenListeners();

  document.getElementById('btn-reset-app')?.addEventListener('click', () => {
    const area = document.getElementById('reset-confirm-area');
    // Already showing confirmation — ignore
    if (area.querySelector('.reset-confirm-prompt')) return;

    const prompt = document.createElement('div');
    prompt.className = 'reset-confirm-prompt';
    prompt.innerHTML = `
      <p>Are you sure? This cannot be undone.</p>
      <div class="reset-confirm-actions">
        <button class="btn btn-secondary btn-sm" id="btn-reset-cancel">Cancel</button>
        <button class="btn btn-danger btn-sm" id="btn-reset-confirm">Yes, delete everything</button>
      </div>
    `;
    area.appendChild(prompt);

    document.getElementById('btn-reset-app').style.display = 'none';

    document.getElementById('btn-reset-cancel').addEventListener('click', () => {
      prompt.remove();
      document.getElementById('btn-reset-app').style.display = '';
    });

    document.getElementById('btn-reset-confirm').addEventListener('click', async () => {
      const confirmBtn = document.getElementById('btn-reset-confirm');
      confirmBtn.disabled = true;
      confirmBtn.textContent = 'Resetting...';
      try {
        await invoke('reset_app');
        window.location.reload();
      } catch (e) {
        showNotification('Reset failed: ' + e, 'error');
        prompt.remove();
        document.getElementById('btn-reset-app').style.display = '';
      }
    });
  });
}

function attachTokenListeners() {
  const field = document.getElementById('token-field');
  if (!field) return;

  // Save new token
  document.getElementById('btn-token-save')?.addEventListener('click', () => saveToken());
  document.getElementById('token-input')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') saveToken();
  });

  // Replace existing token — swap to input mode
  document.getElementById('btn-token-edit')?.addEventListener('click', () => {
    field.innerHTML = `
      <div class="token-input-row">
        <input type="password" class="input" id="token-input" placeholder="ghp_xxxxxxxxxxxx" autocomplete="off" />
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

  // Delete token — send empty string to signal explicit removal
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
      <input type="password" class="input" id="token-input" placeholder="ghp_xxxxxxxxxxxx" autocomplete="off" />
      <button class="btn btn-primary btn-sm" id="btn-token-save">Save</button>
    </div>
    <p class="setting-hint">Optional. Only needed if you exceed GitHub's 60 requests/hour limit.</p>
  `;

  attachTokenListeners();
}

function updateDerivedPaths(basePath) {
  document.getElementById('setting-wine-prefix').value = basePath + '/drive_c/Program Files/Roberts Space Industries/StarCitizen';
}

function escapeHtml(str) {
  if (!str) return '';
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function showNotification(message, type = 'info') {
  const existing = document.querySelector('.settings-notification');
  if (existing) existing.remove();
  const notification = document.createElement('div');
  notification.className = `settings-notification notification-${type}`;
  notification.textContent = message;
  document.body.appendChild(notification);
  setTimeout(() => notification.classList.add('show'), 10);
  setTimeout(() => { notification.classList.remove('show'); setTimeout(() => notification.remove(), 300); }, 3000);
}
