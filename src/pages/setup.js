import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

let defaultPath = '';
let currentPath = '';
let validationResult = null;
let isCreating = false;

export function renderSetup(container, { defaultPath: defPath, onComplete }) {
  defaultPath = defPath;
  currentPath = defPath;
  validationResult = null;
  isCreating = false;

  container.innerHTML = `
    <div class="setup-wizard">
      <div class="setup-card">
        <div class="setup-header">
          <img src="/assets/logos/MadeByTheCommunity_White.png" alt="Star Control" class="setup-logo" />
          <h1 class="setup-title">Welcome to Star Control</h1>
          <p class="setup-subtitle">Linux launcher for Star Citizen</p>
        </div>

        <div class="setup-body">
          <p class="setup-description">
            Choose where Star Citizen will be installed. This directory will hold the Wine prefix,
            runners, and game files. You need at least <strong>100 GB</strong> of free space.
          </p>

          <div class="setup-field">
            <label class="setup-label">Installation Directory</label>
            <div class="path-input-row">
              <input type="text" class="input" id="setup-path-input"
                     value="${escapeHtml(defaultPath)}"
                     placeholder="~/Games/star-citizen" />
              <button class="btn btn-secondary" id="setup-btn-browse">Browse</button>
            </div>
            <div id="setup-path-validation" class="path-validation-msg"></div>
          </div>
        </div>

        <div class="setup-footer">
          <button class="btn btn-primary" id="setup-btn-continue" disabled>Continue</button>
        </div>
      </div>
    </div>
  `;

  const pathInput = document.getElementById('setup-path-input');
  const continueBtn = document.getElementById('setup-btn-continue');

  pathInput.addEventListener('input', () => {
    currentPath = pathInput.value;
  });

  pathInput.addEventListener('blur', () => validateSetupPath());
  pathInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') validateSetupPath();
  });

  document.getElementById('setup-btn-browse').addEventListener('click', async () => {
    try {
      const selected = await open({ directory: true, title: 'Select Installation Directory' });
      if (selected) {
        pathInput.value = selected;
        currentPath = selected;
        validateSetupPath();
      }
    } catch (err) {
      console.error('Browse dialog failed:', err);
    }
  });

  continueBtn.addEventListener('click', async () => {
    if (isCreating) return;
    isCreating = true;
    continueBtn.disabled = true;
    continueBtn.textContent = 'Creating...';

    try {
      await invoke('create_install_directory', { path: currentPath });

      await invoke('save_config', {
        config: {
          install_path: currentPath,
          selected_runner: null,
          performance: {
            esync: true,
            fsync: true,
            dxvk_async: true,
            mangohud: false,
            dxvk_hud: false,
            wayland: true,
            hdr: false,
            fsr: false,
            primary_monitor: null,
          },
          github_token: null,
          log_level: 'info',
          auto_backup_on_launch: null,
        },
      });

      onComplete();
    } catch (err) {
      const msgEl = document.getElementById('setup-path-validation');
      msgEl.className = 'path-validation-msg validation-fail';
      msgEl.textContent = String(err);
      continueBtn.disabled = false;
      continueBtn.textContent = 'Continue';
      isCreating = false;
    }
  });

  // Run initial validation
  validateSetupPath();
}

async function validateSetupPath() {
  const msgEl = document.getElementById('setup-path-validation');
  const continueBtn = document.getElementById('setup-btn-continue');

  if (!currentPath.trim()) {
    msgEl.className = 'path-validation-msg validation-fail';
    msgEl.textContent = 'Please enter an install path';
    continueBtn.disabled = true;
    return;
  }

  msgEl.className = 'path-validation-msg';
  msgEl.textContent = 'Validating...';

  try {
    const result = await invoke('validate_install_path', { path: currentPath });
    validationResult = result;

    if (result.valid) {
      msgEl.className = 'path-validation-msg validation-pass';
      msgEl.textContent = result.message;
      continueBtn.disabled = false;
    } else {
      msgEl.className = 'path-validation-msg validation-fail';
      msgEl.textContent = result.message;
      continueBtn.disabled = true;
    }
  } catch (err) {
    msgEl.className = 'path-validation-msg validation-fail';
    msgEl.textContent = 'Validation failed';
    continueBtn.disabled = true;
  }
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
