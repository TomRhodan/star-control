/**
 * Star Control - Setup Wizard Page
 *
 * This module implements the initial setup wizard:
 * - Step 1: Welcome/disclaimer page with community credits
 * - Step 2: Path selection for the installation directory
 * - Detection of existing installations with mode selection (Quick/Full)
 * - Saving the final configuration
 *
 * @module pages/setup
 */

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import madeByCommunityUrl from '../assets/logos/MadeByTheCommunity_White.png';
import { escapeHtml } from '../utils.js';

/** @type {string} Default installation path (suggested by the backend) */
let defaultPath = '';
/** @type {string} Currently entered/selected path by the user */
let currentPath = '';
/** @type {Object|null} Path validation result */
let validationResult = null;
/** @type {boolean} Locks the "Continue" button during processing */
let isCreating = false;
/** @type {number} Current wizard step (1 = Disclaimer, 2 = Path selection) */
let currentStep = 1;
/** @type {string} Installation mode: 'full' (complete) or 'quick' (DXVK/Wine only) */
let installMode = 'full';
/** @type {Object|null} Result of the existing installation check */
let detectedInstallation = null;

/**
 * Entry point for the setup wizard.
 * Resets all state variables and starts with the disclaimer step.
 *
 * @param {HTMLElement} container - The container element to render into
 * @param {Object} options - Options
 * @param {string} options.defaultPath - Default installation path suggested by the backend
 * @param {Function} options.onComplete - Callback invoked after successful setup
 */
export function renderSetup(container, { defaultPath: defPath, onComplete }) {
  defaultPath = defPath;
  currentPath = defPath;
  validationResult = null;
  isCreating = false;
  currentStep = 1;

  renderDisclaimerStep(container, { onComplete });
}

/**
 * Renders the welcome/disclaimer step (Step 1).
 * Shows the community logo, a description of the app, and links to the
 * projects that make Star Control possible (LUG Wiki, LUG Helper, SC Launcher Configurator).
 *
 * @param {HTMLElement} container - The container element
 * @param {Object} options - Options with onComplete callback
 */
function renderDisclaimerStep(container, { onComplete }) {
  container.innerHTML = `
    <div class="setup-wizard">
      <div class="setup-card">
        <div class="setup-header">
          <img src="${madeByCommunityUrl}" alt="Star Control" class="setup-logo" />
          <h1 class="setup-title">Welcome to Star Control</h1>
          <p class="setup-subtitle">Linux launcher for Star Citizen</p>
        </div>

        <div class="setup-body">
          <p class="setup-description">
            Star Control is a community interface for the Star Citizen Linux ecosystem. This app was
            developed with AI assistance and would not be possible without the following projects:
          </p>

          <!-- Links to the community projects that serve as the foundation -->
          <div class="project-links">
            <a href="https://wiki.starcitizen-lug.org/" target="_blank" rel="noopener noreferrer" class="project-link">
              <span class="project-name">LUG Wiki</span>
              <span class="project-desc">The central resource for Star Citizen on Linux</span>
            </a>
            <a href="https://github.com/starcitizen-lug/lug-helper" target="_blank" rel="noopener noreferrer" class="project-link">
              <span class="project-name">LUG Helper</span>
              <span class="project-desc">Installation scripts and automation</span>
            </a>
            <a href="https://luftwerft.com" target="_blank" rel="noopener noreferrer" class="project-link">
              <span class="project-name">SC Launcher Configurator</span>
              <span class="project-desc">Wine and gaming configuration</span>
            </a>
          </div>

          <p class="setup-description" style="margin-top: 1.5rem;">
            Star Control provides only a graphical interface for these tools. The real magic happens in the projects listed above.
          </p>
        </div>

        <div class="setup-footer">
          <button class="btn btn-primary" id="setup-btn-continue">Continue</button>
        </div>
      </div>
    </div>
  `;

  // Continue to the next step (directory selection)
  document.getElementById('setup-btn-continue').addEventListener('click', () => {
    currentStep = 2;
    renderDirectoryStep(container, { onComplete });
  });
}

/**
 * Renders the directory selection step (Step 2).
 * The user can enter the installation path or select it via a
 * file dialog. The path is validated (existence, write permissions,
 * at least 100 GB free disk space).
 *
 * @param {HTMLElement} container - The container element
 * @param {Object} options - Options with onComplete callback
 */
function renderDirectoryStep(container, { onComplete }) {
  container.innerHTML = `
    <div class="setup-wizard">
      <div class="setup-card">
        <div class="setup-header">
          <img src="${madeByCommunityUrl}" alt="Star Control" class="setup-logo" />
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

  // Update path on input (validation happens on blur/Enter)
  pathInput.addEventListener('input', () => {
    currentPath = pathInput.value;
  });

  // Validate path when the input field loses focus or Enter is pressed
  pathInput.addEventListener('blur', () => validateSetupPath());
  pathInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') validateSetupPath();
  });

  // Open directory browser dialog via the Tauri dialog plugin
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

  // "Continue" button: Checks for existing installation and creates the directory
  continueBtn.addEventListener('click', async () => {
    // Double-click protection: Prevents multiple executions
    if (isCreating) return;
    isCreating = true;
    continueBtn.disabled = true;
    continueBtn.textContent = 'Checking...';

    try {
      // Step 1: Check if runners already exist at the selected path
      let existingRunnerName = null;
      try {
        const scanResult = await invoke('scan_runners', { basePath: currentPath });
        if (scanResult.runners && scanResult.runners.length > 0) {
          existingRunnerName = scanResult.runners[0].name;
        }
      } catch (e) {
        // Ignore scan errors - no problem if no runners are found
      }

      // Step 2: Check if an existing installation is present
      const existingConfig = {
        install_path: currentPath,
        selected_runner: existingRunnerName,
      };

      try {
        detectedInstallation = await invoke('check_installation', { config: existingConfig });
      } catch (e) {
        // check_installation can fail - no problem
        detectedInstallation = null;
      }

      // Step 3: If RSI Launcher exists, show the installation mode dialog
      // (Quick Install vs. full reinstall)
      if (detectedInstallation && detectedInstallation.launcher_exe_exists) {
        continueBtn.textContent = 'Continue';
        continueBtn.disabled = false;
        isCreating = false;
        showInstallModeModal(container, { onComplete, continueBtn });
        return;
      }

      // Step 4: No existing launcher - normal flow (create directory + save config)
      continueBtn.textContent = 'Creating...';

      await invoke('create_install_directory', { path: currentPath });

      // Save default configuration with sensible presets
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
          install_mode: 'full',
        },
      });

      // Setup complete - invoke callback to switch to the main view
      onComplete();
    } catch (err) {
      // Show error and re-enable the button
      const msgEl = document.getElementById('setup-path-validation');
      msgEl.className = 'path-validation-msg validation-fail';
      msgEl.textContent = String(err);
      continueBtn.disabled = false;
      continueBtn.textContent = 'Continue';
      isCreating = false;
    }
  });

  // Perform initial validation of the pre-filled path
  validateSetupPath();
}

/**
 * Validates the currently entered installation path via the Rust backend.
 * Checks existence, write permissions, and available disk space.
 * Enables/disables the "Continue" button based on the result.
 */
async function validateSetupPath() {
  const msgEl = document.getElementById('setup-path-validation');
  const continueBtn = document.getElementById('setup-btn-continue');

  // Empty path is invalid
  if (!currentPath.trim()) {
    msgEl.className = 'path-validation-msg validation-fail';
    msgEl.textContent = 'Please enter an install path';
    continueBtn.disabled = true;
    return;
  }

  msgEl.className = 'path-validation-msg';
  msgEl.textContent = 'Validating...';

  try {
    // Backend validation: Checks path existence, permissions, and free disk space
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



/**
 * Shows a modal dialog for selecting the installation mode.
 * Displayed when an existing RSI Launcher installation is detected.
 * The user can choose between Quick Install (only update DXVK/Wine)
 * and a complete reinstallation.
 *
 * @param {HTMLElement} container - The container element for the modal
 * @param {Object} options - Options
 * @param {Function} options.onComplete - Callback after successful setup
 * @param {HTMLElement} options.continueBtn - Reference to the Continue button (re-enabled on error)
 */
function showInstallModeModal(container, { onComplete, continueBtn }) {
  // Remove existing modal if one exists (double-call protection)
  const existing = container.querySelector('.modal-overlay');
  if (existing) existing.remove();

  const modal = document.createElement('div');
  modal.className = 'modal-overlay';
  modal.innerHTML = `
    <div class="modal-content">
      <h2 class="modal-title">Existing Installation Detected</h2>
      <p class="modal-description">
        An RSI Launcher installation was found at the selected path. You can choose to:
      </p>

      <!-- Show detected installation details (path and runner if available) -->
      <div class="detected-info">
        <div class="detected-row">
          <span class="detected-label">Path:</span>
          <span class="detected-value">${escapeHtml(detectedInstallation.install_path)}</span>
        </div>
        ${detectedInstallation.runner_name ? `
        <div class="detected-row">
          <span class="detected-label">Runner:</span>
          <span class="detected-value">${escapeHtml(detectedInstallation.runner_name)}</span>
        </div>
        ` : ''}
      </div>

      <!-- Choice between Quick Install and full reinstallation -->
      <div class="install-mode-options">
        <div class="mode-option" data-mode="quick">
          <div class="mode-option-header">
            <span class="mode-option-icon">⚡</span>
            <span class="mode-option-title">Quick Install</span>
          </div>
          <p class="mode-option-desc">
            Only install/update DXVK and Wine components. Skip downloading
            the RSI Launcher. (Requires existing Runner)
          </p>
        </div>

        <div class="mode-option" data-mode="full">
          <div class="mode-option-header">
            <span class="mode-option-icon">📦</span>
            <span class="mode-option-title">Reinstall All</span>
          </div>
          <p class="mode-option-desc">
            Complete fresh installation including downloading
            and installing the RSI Launcher.
          </p>
        </div>
      </div>

      <div class="modal-actions">
        <button class="btn btn-secondary" id="modal-btn-cancel">Cancel</button>
        <button class="btn btn-primary" id="modal-btn-continue">Continue</button>
      </div>
    </div>
  `;

  container.appendChild(modal);

  // Trigger CSS transition: show modal overlay with fade-in
  requestAnimationFrame(() => modal.classList.add('show'));

  // Click handler for mode options: Highlights the selected option
  const options = modal.querySelectorAll('.mode-option');
  options.forEach(opt => {
    opt.addEventListener('click', () => {
      options.forEach(o => o.classList.remove('selected'));
      opt.classList.add('selected');
      installMode = opt.dataset.mode;
    });
  });

  // Pre-select "Quick Install" by default
  options[0].classList.add('selected');
  installMode = 'quick';

  // Cancel button closes the modal
  modal.querySelector('#modal-btn-cancel').addEventListener('click', () => {
    modal.remove();
  });

  // "Continue" button: Create directory, save config, and complete setup
  modal.querySelector('#modal-btn-continue').addEventListener('click', async () => {
    const btn = modal.querySelector('#modal-btn-continue');
    btn.disabled = true;
    btn.textContent = 'Saving...';

    try {
      await invoke('create_install_directory', { path: currentPath });

      // Save config with detected runner and selected installation mode
      await invoke('save_config', {
        config: {
          install_path: currentPath,
          selected_runner: detectedInstallation.runner_name,
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
          install_mode: installMode,
        },
      });

      modal.remove();
      onComplete();
    } catch (err) {
      btn.disabled = false;
      btn.textContent = 'Continue';
      console.error('Failed to save config:', err);
    }
  });
}
