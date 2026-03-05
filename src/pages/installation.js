/**
 * Star Control - Installation Page
 *
 * This module handles the installation wizard which includes:
 * - Step 1: System compatibility checks (memory, AVX, mapcount, etc.)
 * - Step 2: Configuration (install path, runner selection, performance options)
 * - Step 3: Actual installation (Wine prefix, DXVK, RSI Launcher)
 *
 * @module pages/installation
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open, confirm } from '@tauri-apps/plugin-dialog';
import { router } from '../router.js';
import { escapeHtml } from '../utils.js';

/** @constant {Array} System check items */
const CHECK_ITEMS = [
  { id: 'memory', name: 'Memory', icon: '', tooltip: 'Star Citizen requires at least 16 GB RAM' },
  { id: 'avx', name: 'AVX Support', icon: '', tooltip: 'Advanced Vector Extensions — required by CryEngine' },
  { id: 'mapcount', name: 'vm.max_map_count', icon: '', tooltip: 'Kernel parameter for memory-mapped files — Wine needs a high value' },
  { id: 'filelimit', name: 'File Descriptor Limit', icon: '', tooltip: 'Maximum number of open file descriptors per process' },
  { id: 'vulkan', name: 'Vulkan Support', icon: '', tooltip: 'Vulkan graphics API — required for DXVK translation' },
  { id: 'diskspace', name: 'Disk Space', icon: '', tooltip: 'At least 100 GB free space recommended' },
];

// --- Wizard State ---

let currentStep = 1;
let systemCheckPassed = false;
let hasRun = false;
let lastCheckResult = null;

let configState = {
  installPath: '',
  selectedRunner: null,
  runners: [],
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
  pathValidation: null,
  installMode: 'full',
};

let detectedMonitors = [];
let fractionalScaling = false;

// --- Runner Download State ---

let availableRunners = [];
let fetchErrors = [];
let isInstallingRunner = false;
let selectedSource = 'LUG';
let availableSources = ['LUG']; // Will be populated from config
let isLoadingRunners = true;
let unlistenProgress = null;

// Loading spinner
const spinnerHtml = '<div class="runners-loading-state"><div class="runners-loading-spinner"></div><span>Loading...</span></div>';

// --- Installation State ---

let installPhase = null;   // null | "running" | "complete" | "error"
let installLog = [];
let unlistenInstall = null;
let installError = null;
let pendingLogLines = [];
let logRafPending = false;

const INSTALL_PHASES = [
  { id: 'prepare', label: 'Prepare environment' },
  { id: 'winetricks', label: 'Install Wine components' },
  { id: 'dxvk', label: 'Install DXVK' },
  { id: 'registry', label: 'Configure registry' },
  { id: 'download', label: 'Download RSI Launcher' },
  { id: 'install', label: 'Install RSI Launcher' },
  { id: 'launch', label: 'Launch RSI Launcher' },
];

// --- Main Render ---

export async function renderInstallation(container) {
  // First, ensure we have the latest runner sources from LUG-Helper
  try {
    await invoke('import_lug_helper_sources');
  } catch (e) {
    // Ignore errors - we'll use cached/default sources
  }

  // Load saved config on first render (must complete before rendering)
  try {
    const config = await invoke('load_config');
    if (config) {
      configState.installPath = config.install_path;
      configState.selectedRunner = config.selected_runner;
      configState.performance = config.performance;
      configState.installMode = config.install_mode || 'full';

      // Populate available sources from config
      if (config.runner_sources && config.runner_sources.length > 0) {
        availableSources = config.runner_sources
          .filter(s => s.enabled)
          .map(s => s.name);
        if (!availableSources.includes(selectedSource)) {
          selectedSource = availableSources[0];
        }
      }
    }
  } catch (e) {
    // Ignore config load errors
  }

  renderCurrentStep(container);

  // Load default path if none set
  if (!configState.installPath) {
    invoke('get_default_install_path').then(path => {
      configState.installPath = path;
    }).catch(e => console.error('Failed to load default install path:', e));
  }
}

function renderCurrentStep(container) {
  container.innerHTML = `
    <div class="page-header">
      <h1>Installation</h1>
      <p class="page-subtitle">Set up Star Citizen on your system</p>
    </div>
    <div class="wizard">
      ${renderWizardSteps()}
      <div class="wizard-content card" id="wizard-body"></div>
    </div>
  `;

  const body = document.getElementById('wizard-body');

  if (currentStep === 1) {
    renderStep1(body);
  } else if (currentStep === 2) {
    renderStep2(body);
  } else if (currentStep === 3) {
    renderStep3(body);
  }
}

function renderWizardSteps() {
  const steps = [
    { num: 1, label: 'System Check', tooltip: 'Verify hardware and kernel requirements' },
    { num: 2, label: 'Configuration', tooltip: 'Set install path, runner, and performance options' },
    { num: 3, label: 'Installation', tooltip: 'Install Wine prefix and RSI Launcher' },
  ];

  return `
    <div class="wizard-steps">
      ${steps.map((s, i) => {
        let cls = '';
        if (s.num === currentStep) cls = 'active';
        else if (s.num < currentStep || (s.num === 1 && systemCheckPassed)) cls = 'completed';

        const icon = cls === 'completed'
          ? '<span class="step-number completed">\u2713</span>'
          : `<span class="step-number">${s.num}</span>`;

        return `
          <div class="wizard-step ${cls}">
            ${icon}
            <span class="step-label" ${s.tooltip ? `data-tooltip="${s.tooltip}" data-tooltip-pos="bottom"` : ''}>${s.label}</span>
          </div>
          ${i < steps.length - 1 ? '<div class="wizard-divider"></div>' : ''}
        `;
      }).join('')}
    </div>
  `;
}

// --- Step 1: System Check ---

function renderStep1(body) {
  body.innerHTML = `
    <h3>System Compatibility Check</h3>
    <div class="check-list" id="check-list">
      ${CHECK_ITEMS.map(item => renderCheckItem(item.id, item.name, item.tooltip)).join('')}
    </div>
    <div id="summary-bar"></div>
    <div class="wizard-actions">
      <button class="btn btn-primary" id="btn-run-check">${hasRun ? 'Re-run Check' : 'Run System Check'}</button>
      <button class="btn btn-secondary" id="btn-next-step" ${!systemCheckPassed ? 'disabled' : ''}>Next Step</button>
    </div>
  `;

  document.getElementById('btn-run-check').addEventListener('click', () => runChecks(body));
  document.getElementById('btn-next-step').addEventListener('click', () => {
    if (systemCheckPassed) {
      currentStep = 2;
      renderCurrentStep(body.closest('#content') || body.parentElement.parentElement);
    }
  });

  // Restore previous results if going back
  if (hasRun && lastCheckResult) {
    lastCheckResult.checks.forEach(check => updateCheckItem(check));
    showSummary(lastCheckResult);
  }
}

function renderCheckItem(id, name, tooltip) {
  const status = hasRun && lastCheckResult
    ? lastCheckResult.checks.find(c => c.id === id)?.status || 'pending'
    : 'pending';

  return `
    <div class="check-item ${hasRun ? 'revealed' : ''}" id="check-${id}" data-status="${status}">
      <div class="check-icon-wrap">
        <span class="check-icon pending">\u25CB</span>
      </div>
      <div class="check-info">
        <span class="check-name" ${tooltip ? `data-tooltip="${tooltip}" data-tooltip-pos="right"` : ''}>${name}</span>
        <span class="check-detail">Waiting...</span>
      </div>
    </div>
  `;
}

async function runChecks(body) {
  const btn = document.getElementById('btn-run-check');
  const nextBtn = document.getElementById('btn-next-step');
  btn.disabled = true;
  btn.textContent = 'Checking...';
  nextBtn.disabled = true;

  CHECK_ITEMS.forEach(item => {
    const el = document.getElementById(`check-${item.id}`);
    if (el) {
      el.dataset.status = 'running';
      el.classList.remove('revealed');
      const icon = el.querySelector('.check-icon');
      icon.className = 'check-icon running';
      icon.innerHTML = '';
      el.querySelector('.check-detail').textContent = 'Checking...';
      const existingFix = el.querySelector('.btn-fix');
      if (existingFix) existingFix.remove();
    }
  });

  document.getElementById('summary-bar').innerHTML = '';

  try {
    const result = await invoke('run_system_check', { installPath: configState.installPath });

    for (let i = 0; i < result.checks.length; i++) {
      await delay(150);
      updateCheckItem(result.checks[i]);
    }

    await delay(200);
    showSummary(result);
    lastCheckResult = result;

    if (result.all_passed) {
      systemCheckPassed = true;
      nextBtn.disabled = false;
    }
  } catch (err) {
    showSummary({ all_passed: false, has_warnings: false, checks: [] });
    console.error('System check failed:', err);
  }

  hasRun = true;
  btn.disabled = false;
  btn.textContent = 'Re-run Check';
}

function updateCheckItem(check) {
  const el = document.getElementById(`check-${check.id}`);
  if (!el) return;

  el.dataset.status = check.status;

  const icon = el.querySelector('.check-icon');
  icon.className = `check-icon ${check.status}`;

  if (check.status === 'pass') {
    icon.innerHTML = '\u2713';
  } else if (check.status === 'warn') {
    icon.innerHTML = '\u26A0';
  } else {
    icon.innerHTML = '\u2717';
  }

  el.querySelector('.check-detail').textContent = check.detail;

  if (check.fixable && check.status === 'fail') {
    const existing = el.querySelector('.btn-fix');
    if (!existing) {
      const fixBtn = document.createElement('button');
      fixBtn.className = 'btn btn-fix';
      fixBtn.textContent = 'Fix';
      fixBtn.addEventListener('click', () => applyFix(check.id, fixBtn));
      el.appendChild(fixBtn);
    }
  }

  el.classList.add('revealed');
}

async function applyFix(checkId, btn) {
  const commandMap = {
    mapcount: 'fix_mapcount',
    filelimit: 'fix_filelimit',
  };

  const command = commandMap[checkId];
  if (!command) return;

  const descriptions = {
    mapcount: 'This will set vm.max_map_count=16777216 system-wide (requires root via pkexec).',
    filelimit: 'This will increase the system file descriptor limit (requires root via pkexec).',
  };

  const confirmed = await confirm(
    descriptions[checkId] || 'This will modify system settings (requires root).',
    { title: 'Apply System Fix?', kind: 'warning' }
  );
  if (!confirmed) return;

  btn.disabled = true;
  btn.textContent = 'Fixing...';

  try {
    const result = await invoke(command);
    if (result.success) {
      btn.textContent = 'Fixed!';
      btn.classList.add('fixed');
      await delay(500);
      const body = document.getElementById('wizard-body');
      if (body) runChecks(body);
    } else {
      btn.textContent = 'Failed';
      btn.title = result.message;
      const el = document.getElementById(`check-${checkId}`);
      if (el) {
        el.querySelector('.check-detail').textContent = result.message;
      }
      setTimeout(() => {
        btn.disabled = false;
        btn.textContent = 'Retry Fix';
      }, 2000);
    }
  } catch (err) {
    btn.textContent = 'Error';
    btn.disabled = false;
    setTimeout(() => { btn.textContent = 'Retry Fix'; }, 2000);
  }
}

function showSummary(result) {
  const bar = document.getElementById('summary-bar');
  if (!bar) return;

  let cls, text;
  if (result.all_passed && !result.has_warnings) {
    cls = 'summary-pass';
    text = 'All checks passed \u2014 your system is ready!';
  } else if (result.all_passed && result.has_warnings) {
    cls = 'summary-warn';
    text = 'Checks passed with warnings \u2014 Star Citizen should work, but performance may be affected.';
  } else {
    const failCount = result.checks.filter(c => c.status === 'fail').length;
    cls = 'summary-fail';
    text = `${failCount} check${failCount !== 1 ? 's' : ''} failed \u2014 please resolve before continuing.`;
  }

  bar.innerHTML = `<div class="summary-bar ${cls}">${text}</div>`;
}

// --- Step 2: Configuration ---

function renderStep2(body) {
  body.innerHTML = `
    <h3>Configuration</h3>

    <div class="config-section">
      <h4 class="config-section-title">Install Directory</h4>
      <div class="path-input-row">
        <input type="text" class="input" id="install-path-input"
               value="${escapeHtml(configState.installPath)}"
               placeholder="~/Games/star-citizen"
               aria-label="Install directory path" />
        <button class="btn btn-secondary" id="btn-browse">Browse</button>
      </div>
      <div id="path-validation" class="path-validation-msg"></div>
    </div>

    <div class="config-section">
      <h4 class="config-section-title">Wine Runner</h4>
      <div id="runner-section">
        <div class="runner-loading">Scanning for runners...</div>
      </div>
    </div>

    <div class="config-section config-section-last">
      <h4 class="config-section-title">Performance Options</h4>
      <div class="perf-options">
        ${renderPerfGroup('Performance', [
          renderToggle('esync', 'ESync', 'Eventfd-based synchronization — reduces CPU overhead in Wine', configState.performance.esync),
          renderToggle('fsync', 'FSync', 'Futex-based synchronization — faster than ESync on supported kernels', configState.performance.fsync),
          renderToggle('dxvk_async', 'DXVK Async', 'Asynchronous shader compilation — reduces stutter', configState.performance.dxvk_async),
        ])}
        ${renderPerfGroup('Display', [
          fractionalScaling
            ? renderBlockedToggle('wayland', 'Wayland', 'Fractional scaling detected — Wayland mode is not compatible. Set all monitors to 100% scale to use this option.')
            : renderToggle('wayland', 'Wayland', 'Enable Wayland protocol support in Wine', configState.performance.wayland),
          renderToggle('hdr', 'HDR', 'High Dynamic Range rendering (PROTON_ENABLE_HDR + DXVK_HDR)', configState.performance.hdr),
          renderToggle('fsr', 'FSR', 'AMD FidelityFX Super Resolution 4 upscaling', configState.performance.fsr),
          fractionalScaling ? '' : renderMonitorDropdown(),
        ])}
        ${renderPerfGroup('Overlays', [
          renderToggle('mangohud', 'MangoHUD', 'On-screen performance overlay (FPS, CPU, GPU, RAM)', configState.performance.mangohud),
          renderToggle('dxvk_hud', 'DXVK HUD', 'DXVK-specific overlay showing draw calls and compiler activity', configState.performance.dxvk_hud),
        ])}
      </div>
    </div>

    <div class="wizard-actions">
      <button class="btn btn-secondary" id="btn-back">Back</button>
      <button class="btn btn-primary" id="btn-next-step2" disabled>Next Step</button>
    </div>
  `;

  // Event listeners
  const pathInput = document.getElementById('install-path-input');
  pathInput.addEventListener('blur', () => validatePath(pathInput.value));
  pathInput.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') validatePath(pathInput.value);
  });

  document.getElementById('btn-browse').addEventListener('click', async () => {
    try {
      const selected = await open({ directory: true, title: 'Select Install Directory' });
      if (selected) {
        pathInput.value = selected;
        configState.installPath = selected;
        validatePath(selected);
      }
    } catch (err) {
      console.error('Browse dialog failed:', err);
    }
  });

  document.getElementById('btn-back').addEventListener('click', () => {
    currentStep = 1;
    renderCurrentStep(body.closest('#content') || body.parentElement.parentElement);
  });

  document.getElementById('btn-next-step2').addEventListener('click', async () => {
    await saveCurrentConfig();
    currentStep = 3;
    renderCurrentStep(body.closest('#content') || body.parentElement.parentElement);
  });

  // Toggle listeners
  document.querySelectorAll('.perf-toggle input[type="checkbox"]').forEach(cb => {
    if (!cb.dataset.key) return;
    cb.addEventListener('change', () => {
      configState.performance[cb.dataset.key] = cb.checked;
      // Show/hide monitor dropdown when Wayland toggle changes
      if (cb.dataset.key === 'wayland') {
        const monitorRow = document.getElementById('monitor-select-row');
        if (monitorRow) monitorRow.style.display = cb.checked ? '' : 'none';
      }
    });
  });

  // Monitor enabled checkbox
  const monitorEnabledCb = document.getElementById('monitor-enabled');
  if (monitorEnabledCb) {
    monitorEnabledCb.addEventListener('change', () => {
      const container = document.getElementById('monitor-select-container');
      if (monitorEnabledCb.checked) {
        // Enable: pick first detected monitor or leave for manual input
        const select = document.getElementById('monitor-select');
        const input = document.getElementById('monitor-input');
        if (container) container.classList.remove('disabled');
        if (select) { select.disabled = false; configState.performance.primary_monitor = select.value || (detectedMonitors[0]?.name ?? null); select.value = configState.performance.primary_monitor || ''; }
        if (input) { input.disabled = false; configState.performance.primary_monitor = input.value.trim() || null; }
      } else {
        // Disable: clear value
        configState.performance.primary_monitor = null;
        if (container) container.classList.add('disabled');
        const select = document.getElementById('monitor-select');
        const input = document.getElementById('monitor-input');
        if (select) select.disabled = true;
        if (input) input.disabled = true;
      }
    });
  }

  // Detect monitors and check for fractional scaling
  invoke('detect_monitors').then(monitors => {
    detectedMonitors = monitors;
    const wasFractional = fractionalScaling;
    fractionalScaling = monitors.some(m => m.scale != null && Math.abs(m.scale - 1.0) > 0.01);

    if (fractionalScaling && configState.performance.wayland) {
      configState.performance.wayland = false;
    }

    if (fractionalScaling !== wasFractional) {
      // Re-render step 2 to reflect the blocked Wayland toggle
      renderCurrentStep(body.closest('#content') || body.parentElement.parentElement);
    } else {
      updateMonitorDropdown();
    }
  }).catch(e => {
    console.error('Failed to detect monitors:', e);
    detectedMonitors = [];
    updateMonitorDropdown();
  });

  // Initial validation + runner scan
  if (configState.installPath) {
    validatePath(configState.installPath);
  } else {
    renderRunnerSection();
  }
}

function renderToggle(key, label, hint, checked) {
  return `
    <label class="perf-toggle">
      <input type="checkbox" data-key="${key}" ${checked ? 'checked' : ''} />
      <span class="toggle-label">${label}</span>
      <span class="toggle-hint">${hint}</span>
    </label>
  `;
}

function renderBlockedToggle(key, label, hint) {
  return `
    <label class="perf-toggle toggle-blocked" data-tooltip="${hint}" data-tooltip-pos="right">
      <input type="checkbox" data-key="${key}" disabled />
      <span class="toggle-label">${label}</span>
      <span class="toggle-hint">${hint}</span>
    </label>
  `;
}

function renderPerfGroup(title, toggles) {
  return `
    <div class="perf-group">
      <div class="perf-group-title">${title}</div>
      ${toggles.join('')}
    </div>
  `;
}

function renderMonitorDropdown() {
  const hidden = !configState.performance.wayland ? 'style="display:none"' : '';
  const enabled = !!configState.performance.primary_monitor;

  return `
    <label class="perf-toggle" id="monitor-select-row" ${hidden}>
      <input type="checkbox" id="monitor-enabled" ${enabled ? 'checked' : ''} />
      <span class="toggle-label">Wayland Monitor</span>
      <span class="toggle-hint monitor-hint-row">
        <span class="monitor-env-name">WAYLANDDRV_PRIMARY_MONITOR</span>
        <span class="monitor-select-container ${!enabled ? 'disabled' : ''}" id="monitor-select-container">
          <select class="input monitor-select-input" id="monitor-select" ${!enabled ? 'disabled' : ''} aria-label="Wayland monitor">
            <option value="">Detecting...</option>
          </select>
        </span>
      </span>
    </label>
  `;
}

function updateMonitorDropdown() {
  const container = document.getElementById('monitor-select-container');
  if (!container) return;

  const enabled = !!configState.performance.primary_monitor;
  const currentValue = configState.performance.primary_monitor || '';

  if (detectedMonitors.length > 0) {
    const options = detectedMonitors.map(m => {
      const label = `${m.name}${m.resolution ? ' (' + m.resolution : ''}${m.primary ? ', primary)' : m.resolution ? ')' : ''}`;
      const selected = currentValue === m.name ? 'selected' : '';
      return `<option value="${escapeHtml(m.name)}" ${selected}>${escapeHtml(label)}</option>`;
    }).join('');

    container.innerHTML = `
      <select class="input monitor-select-input" id="monitor-select" ${!enabled ? 'disabled' : ''} aria-label="Wayland monitor">
        ${options}
      </select>
    `;
  } else {
    container.innerHTML = `
      <input type="text" class="input monitor-select-input" id="monitor-input"
             value="${escapeHtml(currentValue)}"
             placeholder="e.g. DP-1 (xrandr n/a)"
             ${!enabled ? 'disabled' : ''}
             aria-label="Wayland monitor name" />
    `;
  }

  if (!enabled) container.classList.add('disabled');
  else container.classList.remove('disabled');

  bindMonitorInputListeners();
}

function bindMonitorInputListeners() {
  const select = document.getElementById('monitor-select');
  if (select) {
    select.addEventListener('change', (e) => {
      configState.performance.primary_monitor = e.target.value || null;
    });
  }
  const input = document.getElementById('monitor-input');
  if (input) {
    input.addEventListener('input', (e) => {
      configState.performance.primary_monitor = e.target.value.trim() || null;
    });
  }
}

function updateNextButton() {
  const nextBtn = document.getElementById('btn-next-step2');
  if (!nextBtn) return;
  const pathValid = configState.pathValidation && configState.pathValidation.valid;
  const hasRunner = !!configState.selectedRunner;
  nextBtn.disabled = !(pathValid && hasRunner);
}

async function validatePath(path) {
  configState.installPath = path;
  const msgEl = document.getElementById('path-validation');

  if (!path.trim()) {
    msgEl.className = 'path-validation-msg validation-fail';
    msgEl.textContent = 'Please enter an install path';
    configState.pathValidation = null;
    updateNextButton();
    return;
  }

  msgEl.className = 'path-validation-msg';
  msgEl.textContent = 'Validating...';

  try {
    const result = await invoke('validate_install_path', { path });
    configState.pathValidation = result;

    if (result.valid) {
      msgEl.className = 'path-validation-msg validation-pass';
      msgEl.textContent = result.message;
    } else {
      msgEl.className = 'path-validation-msg validation-fail';
      msgEl.textContent = result.message;
    }

    updateNextButton();

    // Re-scan runners when path changes and auto-save if valid
    renderRunnerSection();
    if (result.valid) saveCurrentConfig();
  } catch (err) {
    msgEl.className = 'path-validation-msg validation-fail';
    msgEl.textContent = 'Validation failed';
    configState.pathValidation = null;
    updateNextButton();
  }
}

async function renderRunnerSection() {
  const section = document.getElementById('runner-section');
  if (!section) return;

  if (!configState.installPath.trim()) {
    section.innerHTML = '<div class="runner-empty-notice">Set an install path first to scan for runners.</div>';
    return;
  }

  section.innerHTML = '<div class="runner-loading">Scanning for runners...</div>';

  // Scan local runners
  let localRunners = [];
  try {
    const result = await invoke('scan_runners', { basePath: configState.installPath });
    configState.runners = result.runners;
    localRunners = result.runners;
  } catch (err) {
    // ignore scan errors
  }

  // Build section HTML
  let html = '';

  // Local runner dropdown (if any exist)
  if (localRunners.length > 0) {
    const options = localRunners.map(r => {
      const selected = configState.selectedRunner === r.name ? 'selected' : '';
      return `<option value="${escapeHtml(r.name)}" ${selected}>${escapeHtml(r.name)}</option>`;
    }).join('');

    html += `
      <select class="input" id="runner-select" aria-label="Wine runner">
        <option value="">-- Select a runner --</option>
        ${options}
      </select>
    `;
  }

  // Download panel
  html += `
    <div class="runner-download-panel ${localRunners.length > 0 ? 'has-local' : ''}">
      ${localRunners.length > 0 ? '<div class="runner-download-toggle" id="toggle-download-panel">Download more runners...</div>' : ''}
      <div class="runner-download-content" id="runner-download-content" ${localRunners.length > 0 ? 'style="display:none"' : ''}>
        <div class="runner-source-tabs" id="source-tabs">
          ${availableSources.map(s => `<button class="source-tab ${selectedSource === s ? 'active' : ''}" data-source="${s}">${s}</button>`).join('')}
        </div>
        <div id="fetch-errors"></div>
        <div class="runner-available-list" id="runner-available-list">
          <div class="runner-loading">Fetching available runners...</div>
        </div>
      </div>
    </div>

    <div class="runner-install-overlay" id="runner-install-overlay" style="display:none">
      <div class="runner-install-name" id="install-runner-name"></div>
      <div class="progress-bar-track">
        <div class="progress-bar-fill" id="install-progress-fill"></div>
      </div>
      <div class="runner-install-status" id="install-status">Preparing...</div>
      <button class="btn btn-sm btn-secondary" id="btn-cancel-install">Cancel</button>
    </div>
  `;

  section.innerHTML = html;

  // Event: local runner select
  const selectEl = document.getElementById('runner-select');
  if (selectEl) {
    selectEl.addEventListener('change', (e) => {
      configState.selectedRunner = e.target.value || null;
      updateNextButton();
    });
  }

  // Event: toggle download panel
  const toggleEl = document.getElementById('toggle-download-panel');
  if (toggleEl) {
    toggleEl.addEventListener('click', () => {
      const content = document.getElementById('runner-download-content');
      if (content) {
        const visible = content.style.display !== 'none';
        content.style.display = visible ? 'none' : '';
        toggleEl.textContent = visible ? 'Download more runners...' : 'Hide download panel';
      }
    });
  }

  // Event: source tabs
  document.querySelectorAll('#source-tabs .source-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      selectedSource = tab.dataset.source;
      document.querySelectorAll('#source-tabs .source-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      renderAvailableRunnersList();
    });
  });

  // Event: cancel install
  const cancelBtn = document.getElementById('btn-cancel-install');
  if (cancelBtn) {
    cancelBtn.addEventListener('click', async () => {
      try { await invoke('cancel_runner_install'); } catch (e) { /* ignore */ }
    });
  }

  // Fetch online runners
  fetchAvailableRunners();
}

async function fetchAvailableRunners() {
  isLoadingRunners = true;

  // Show loading state in the available list
  const list = document.getElementById('runner-available-list');
  if (list) list.innerHTML = spinnerHtml;

  try {
    const result = await invoke('fetch_available_runners', { basePath: configState.installPath });
    availableRunners = result.runners;
    fetchErrors = result.errors;
  } catch (err) {
    availableRunners = [];
    fetchErrors = [String(err)];
  }

  isLoadingRunners = false;
  renderAvailableRunnersList();
  renderFetchErrors();
}

function renderFetchErrors() {
  const el = document.getElementById('fetch-errors');
  if (!el || fetchErrors.length === 0) return;
  el.innerHTML = fetchErrors.map(e =>
    `<div class="runner-fetch-error">${escapeHtml(e)}</div>`
  ).join('');
}

function renderAvailableRunnersList() {
  const list = document.getElementById('runner-available-list');
  if (!list) return;

  const filtered = availableRunners.filter(r => r.source === selectedSource);

  if (filtered.length === 0) {
    list.innerHTML = '<div class="runner-empty-notice">No runners available from this source.</div>';
    return;
  }

  list.innerHTML = filtered.map(r => `
    <div class="runner-available-item ${r.installed ? 'installed' : ''}">
      <div class="runner-item-info">
        <span class="runner-item-name">${escapeHtml(r.name)}</span>
        <span class="runner-item-meta">
          <span class="runner-source-badge">${escapeHtml(r.source)}</span>
          <span class="runner-item-size">${formatSize(r.size_bytes)}</span>
        </span>
      </div>
      ${r.installed
        ? '<span class="runner-installed-badge">Installed</span>'
        : `<button class="btn btn-sm btn-install" data-url="${escapeHtml(r.download_url)}" data-file="${escapeHtml(r.file_name)}" data-name="${escapeHtml(r.name)}">Install</button>`
      }
    </div>
  `).join('');

  // Install button listeners
  list.querySelectorAll('.btn-install').forEach(btn => {
    btn.addEventListener('click', () => {
      installRunner(btn.dataset.url, btn.dataset.file, btn.dataset.name);
    });
  });
}

async function installRunner(downloadUrl, fileName, displayName) {
  if (isInstallingRunner) return;
  isInstallingRunner = true;

  const overlay = document.getElementById('runner-install-overlay');
  const nameEl = document.getElementById('install-runner-name');
  const fillEl = document.getElementById('install-progress-fill');
  const statusEl = document.getElementById('install-status');

  if (overlay) overlay.style.display = '';
  if (nameEl) nameEl.textContent = displayName;
  if (fillEl) { fillEl.style.width = '0%'; fillEl.classList.remove('extracting'); }
  if (statusEl) statusEl.textContent = 'Starting download...';

  // Listen for progress events
  if (unlistenProgress) { unlistenProgress(); unlistenProgress = null; }

  try {
    unlistenProgress = await listen('runner-download-progress', (event) => {
      const p = event.payload;
      const fill = document.getElementById('install-progress-fill');
      const status = document.getElementById('install-status');
      if (!fill || !status) return;

      if (p.phase === 'downloading') {
        fill.classList.remove('extracting');
        fill.style.width = `${p.percent.toFixed(1)}%`;
        status.textContent = `Downloading... ${formatSize(p.bytes_downloaded)} / ${formatSize(p.total_bytes)}`;
      } else if (p.phase === 'extracting') {
        fill.style.width = '100%';
        fill.classList.add('extracting');
        status.textContent = 'Extracting archive...';
      } else if (p.phase === 'complete') {
        fill.style.width = '100%';
        fill.classList.remove('extracting');
        status.textContent = 'Installation complete!';
      } else if (p.phase === 'error') {
        fill.classList.remove('extracting');
        status.textContent = p.message;
      }
    });
  } catch (e) {
    // listen failed
  }

  try {
    await invoke('install_runner', {
      downloadUrl,
      fileName,
      basePath: configState.installPath,
    });
  } catch (err) {
    if (statusEl) statusEl.textContent = `Error: ${err}`;
  }

  // Cleanup
  if (unlistenProgress) { unlistenProgress(); unlistenProgress = null; }
  isInstallingRunner = false;

  // Auto-hide overlay after short delay, refresh runner list
  await delay(1200);
  if (overlay) overlay.style.display = 'none';

  // Auto-select the newly installed runner if none selected yet
  if (!configState.selectedRunner) {
    try {
      const result = await invoke('scan_runners', { basePath: configState.installPath });
      if (result.runners.length > 0) {
        configState.selectedRunner = result.runners[result.runners.length - 1].name;
      }
    } catch (_) {}
  }

  // Refresh both local and online runner lists
  renderRunnerSection();
  updateNextButton();
}

function formatSize(bytes) {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const val = bytes / Math.pow(1024, i);
  return `${val.toFixed(i > 0 ? 1 : 0)} ${units[i]}`;
}

async function saveCurrentConfig() {
  const config = {
    install_path: configState.installPath,
    selected_runner: configState.selectedRunner,
    performance: configState.performance,
  };

  try {
    await invoke('save_config', { config });
  } catch (err) {
    console.error('Failed to save config:', err);
  }
}

// --- Step 3: Installation ---

let currentPhaseId = null;
let currentPercent = 0;
let completedPhases = new Set();

function renderStep3(body) {
  if (!configState.selectedRunner) {
    body.innerHTML = `
      <h3>Installation</h3>
      <div class="step3-placeholder">
        <p>No runner selected. Please go back and select a Wine runner first.</p>
      </div>
      <div class="wizard-actions">
        <button class="btn btn-secondary" id="btn-back3">Back</button>
      </div>
    `;
    document.getElementById('btn-back3').addEventListener('click', () => {
      currentStep = 2;
      renderCurrentStep(body.closest('#content') || body.parentElement.parentElement);
    });
    return;
  }

  // If installation is already in progress or finished, show the process view
  if (installPhase === 'running' || installPhase === 'complete' || installPhase === 'error') {
    renderStep3Process(body);
    return;
  }

  // Otherwise show the summary
  const perf = configState.performance;
  const enabledOptions = [];
  if (perf.esync) enabledOptions.push('ESync');
  if (perf.fsync) enabledOptions.push('FSync');
  if (perf.dxvk_async) enabledOptions.push('DXVK Async');
  if (perf.wayland) enabledOptions.push('Wayland');
  if (perf.hdr) enabledOptions.push('HDR');
  if (perf.fsr) enabledOptions.push('FSR');
  if (perf.mangohud) enabledOptions.push('MangoHUD');
  if (perf.dxvk_hud) enabledOptions.push('DXVK HUD');

  body.innerHTML = `
    <h3>Installation</h3>

    <div class="install-summary">
      <div class="install-summary-row">
        <span class="install-summary-label">Install Path</span>
        <span class="install-summary-value mono">${escapeHtml(configState.installPath)}</span>
      </div>
      <div class="install-summary-row">
        <span class="install-summary-label">Wine Runner</span>
        <span class="install-summary-value">${escapeHtml(configState.selectedRunner)}</span>
      </div>
      <div class="install-summary-row">
        <span class="install-summary-label">Performance</span>
        <span class="install-summary-value">
          ${enabledOptions.length > 0
            ? enabledOptions.map(o => `<span class="install-summary-badge">${o}</span>`).join('')
            : '<span class="text-muted">None</span>'}
        </span>
      </div>
      ${perf.primary_monitor ? `
      <div class="install-summary-row">
        <span class="install-summary-label">Primary Monitor</span>
        <span class="install-summary-value mono">${escapeHtml(perf.primary_monitor)}</span>
      </div>
      ` : ''}
    </div>

    <div class="install-summary-hint">
      This will create a Wine prefix, install required components (winetricks, DXVK, PowerShell),
      download and install the RSI Launcher.
    </div>

    <div class="wizard-actions">
      <button class="btn btn-secondary" id="btn-back3">Back</button>
      <button class="btn btn-primary" id="btn-start-install">Start Installation</button>
    </div>
  `;

  document.getElementById('btn-back3').addEventListener('click', () => {
    currentStep = 2;
    renderCurrentStep(body.closest('#content') || body.parentElement.parentElement);
  });

  document.getElementById('btn-start-install').addEventListener('click', () => {
    renderStep3Process(body);
    startInstallation();
  });
}

function renderStep3Process(body) {
  body.innerHTML = `
    <h3>Installation</h3>

    <div class="install-phases" id="install-phases">
      ${INSTALL_PHASES.map(p => `
        <div class="install-phase" id="phase-${p.id}" data-phase="${p.id}">
          <span class="install-phase-icon">\u25CB</span>
          <span class="install-phase-label">${p.label}</span>
        </div>
      `).join('')}
    </div>

    <div class="install-log-container" id="install-log-container">
      <div class="install-log" id="install-log"></div>
    </div>

    <div id="install-error-msg" class="install-error-msg" style="display:none"></div>

    <div class="wizard-actions">
      <button class="btn btn-secondary" id="btn-back3" disabled>Back</button>
      <div class="install-progress-wrapper" id="install-progress-wrapper">
        <div class="progress-bar-track">
          <div class="progress-bar-fill" id="install-progress-fill"></div>
        </div>
        <span class="install-percent" id="install-percent">0%</span>
      </div>
      <button class="btn btn-secondary" id="btn-cancel-install">Cancel</button>
      <button class="btn btn-primary" id="btn-retry-install" style="display:none">Retry</button>
    </div>
  `;

  document.getElementById('btn-back3').addEventListener('click', () => {
    if (installPhase === 'running') return;
    currentStep = 2;
    renderCurrentStep(body.closest('#content') || body.parentElement.parentElement);
  });

  document.getElementById('btn-cancel-install').addEventListener('click', async () => {
    try { await invoke('cancel_installation'); } catch (e) { /* ignore */ }
  });

  document.getElementById('btn-retry-install').addEventListener('click', () => {
    installPhase = null;
    installLog = [];
    installError = null;
    currentPhaseId = null;
    currentPercent = 0;
    completedPhases.clear();
    renderStep3(body);
  });

  // Restore state if resuming
  if (installPhase === 'running' || installPhase === 'complete' || installPhase === 'error') {
    restoreInstallUI();
  }
}

async function startInstallation() {
  installPhase = 'running';
  installLog = [];
  pendingLogLines = [];
  logRafPending = false;
  installError = null;
  currentPhaseId = null;
  currentPercent = 0;
  completedPhases.clear();

  // Update UI
  const startBtn = document.getElementById('btn-start-install');
  const cancelBtn = document.getElementById('btn-cancel-install');
  const backBtn = document.getElementById('btn-back3');
  const progressWrapper = document.getElementById('install-progress-wrapper');
  const logContainer = document.getElementById('install-log-container');

  if (startBtn) startBtn.style.display = 'none';
  if (cancelBtn) cancelBtn.style.display = '';
  if (backBtn) backBtn.disabled = true;
  if (progressWrapper) progressWrapper.style.display = '';
  if (logContainer) logContainer.style.display = '';

  // Listen for progress events
  if (unlistenInstall) { unlistenInstall(); unlistenInstall = null; }

  try {
    unlistenInstall = await listen('install-progress', (event) => {
      handleInstallProgress(event.payload);
    });
  } catch (e) {
    // listen failed
  }

  try {
    await invoke('run_installation', {
      config: {
        install_path: configState.installPath,
        selected_runner: configState.selectedRunner,
        performance: configState.performance,
        install_mode: configState.installMode,
      },
    });
    onInstallComplete();
  } catch (err) {
    onInstallError(String(err));
  }

  if (unlistenInstall) { unlistenInstall(); unlistenInstall = null; }
}

function handleInstallProgress(payload) {
  const { phase, step, percent, log_line } = payload;

  // Track phase transitions
  if (phase !== 'error' && phase !== 'complete' && phase !== currentPhaseId) {
    if (currentPhaseId) {
      completedPhases.add(currentPhaseId);
    }
    currentPhaseId = phase;
  }

  // Store launch logs in global state for the Launch page
  if ((phase === 'launch' || phase === 'complete') && log_line) {
    if (!window._starControlLaunchLogs) {
      window._starControlLaunchLogs = [];
    }
    window._starControlLaunchLogs.push(log_line);
  }

  currentPercent = percent;

  // Update phase list UI
  updatePhaseList();

  // Update progress bar
  const fill = document.getElementById('install-progress-fill');
  const percentLabel = document.getElementById('install-percent');
  if (fill) fill.style.width = `${percent.toFixed(1)}%`;
  if (percentLabel) percentLabel.textContent = `${Math.round(percent)}%`;

  // Batch log line DOM updates via requestAnimationFrame to prevent UI flooding
  if (log_line) {
    installLog.push(log_line);
    pendingLogLines.push(log_line);
    if (!logRafPending) {
      logRafPending = true;
      requestAnimationFrame(flushPendingLogLines);
    }
  }
}

function flushPendingLogLines() {
  logRafPending = false;
  const logEl = document.getElementById('install-log');
  if (!logEl || pendingLogLines.length === 0) return;

  const fragment = document.createDocumentFragment();
  for (const text of pendingLogLines) {
    const line = document.createElement('div');
    line.className = 'install-log-line';
    line.textContent = `> ${text}`;
    fragment.appendChild(line);
  }
  pendingLogLines = [];
  logEl.appendChild(fragment);
  logEl.scrollTop = logEl.scrollHeight;
}

function updatePhaseList() {
  INSTALL_PHASES.forEach(p => {
    const el = document.getElementById(`phase-${p.id}`);
    if (!el) return;

    const icon = el.querySelector('.install-phase-icon');
    el.className = 'install-phase';

    if (completedPhases.has(p.id)) {
      el.classList.add('done');
      if (icon) { icon.className = 'install-phase-icon'; icon.textContent = '\u2713'; }
    } else if (p.id === currentPhaseId) {
      el.classList.add('active');
      if (icon) { icon.textContent = ''; icon.className = 'install-phase-icon spinning'; }
    } else {
      if (icon) { icon.textContent = '\u25CB'; icon.className = 'install-phase-icon'; }
    }
  });
}

function onInstallComplete() {
  installPhase = 'complete';
  if (currentPhaseId) completedPhases.add(currentPhaseId);
  INSTALL_PHASES.forEach(p => completedPhases.add(p.id));
  currentPhaseId = null;

  // Installation done — update sidebar to show full navigation and go to Launch
  router.updateSidebar(true);
  router.navigate('launch');
}

function onInstallError(error) {
  installPhase = 'error';
  installError = error;

  const cancelBtn = document.getElementById('btn-cancel-install');
  const retryBtn = document.getElementById('btn-retry-install');
  const backBtn = document.getElementById('btn-back3');
  const startBtn = document.getElementById('btn-start-install');
  const errorMsg = document.getElementById('install-error-msg');

  if (cancelBtn) cancelBtn.style.display = 'none';
  if (retryBtn) retryBtn.style.display = '';
  if (backBtn) backBtn.disabled = false;
  if (startBtn) startBtn.style.display = 'none';
  if (errorMsg) {
    errorMsg.style.display = '';
    errorMsg.textContent = error;
  }

  appendLog(`ERROR: ${error}`);
}

function appendLog(text) {
  installLog.push(text);
  const logEl = document.getElementById('install-log');
  if (logEl) {
    const line = document.createElement('div');
    line.className = 'install-log-line';
    line.textContent = `> ${text}`;
    logEl.appendChild(line);
    logEl.scrollTop = logEl.scrollHeight;
  }
}

function restoreInstallUI() {
  const logContainer = document.getElementById('install-log-container');
  const progressWrapper = document.getElementById('install-progress-wrapper');
  const startBtn = document.getElementById('btn-start-install');

  if (logContainer) logContainer.style.display = '';
  if (progressWrapper) progressWrapper.style.display = '';
  if (startBtn) startBtn.style.display = 'none';

  // Restore log
  const logEl = document.getElementById('install-log');
  if (logEl) {
    logEl.innerHTML = installLog.map(l => `<div class="install-log-line">> ${escapeHtml(l)}</div>`).join('');
    logEl.scrollTop = logEl.scrollHeight;
  }

  // Restore progress
  const fill = document.getElementById('install-progress-fill');
  const percentLabel = document.getElementById('install-percent');
  if (fill) fill.style.width = `${currentPercent.toFixed(1)}%`;
  if (percentLabel) percentLabel.textContent = `${Math.round(currentPercent)}%`;

  updatePhaseList();

  if (installPhase === 'complete') {
    onInstallComplete();
  } else if (installPhase === 'error') {
    const cancelBtn = document.getElementById('btn-cancel-install');
    const retryBtn = document.getElementById('btn-retry-install');
    const backBtn = document.getElementById('btn-back3');
    const errorMsg = document.getElementById('install-error-msg');
    if (cancelBtn) cancelBtn.style.display = 'none';
    if (retryBtn) retryBtn.style.display = '';
    if (backBtn) backBtn.disabled = false;
    if (errorMsg && installError) {
      errorMsg.style.display = '';
      errorMsg.textContent = installError;
    }
  } else if (installPhase === 'running') {
    const cancelBtn = document.getElementById('btn-cancel-install');
    const backBtn = document.getElementById('btn-back3');
    if (cancelBtn) cancelBtn.style.display = '';
    if (backBtn) backBtn.disabled = true;
  }
}

// --- Helpers ---

function delay(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

