/**
 * Star Control - Launch Page
 *
 * This module handles launching Star Citizen with configurable options:
 * - Launch/stop the RSI Launcher
 * - Configure performance options (ESync, FSync, DXVK Async)
 * - Configure display options (Wayland, HDR, FSR)
 * - Configure overlays (MangoHUD, DXVK HUD)
 * - Monitor selection for Wayland
 * - Real-time log output
 *
 * @module pages/launch
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** @constant {string} Launch status states */
let launchStatus = 'idle'; // 'idle' | 'checking' | 'ready' | 'launching' | 'running' | 'error' | 'not_installed'
let launchConfig = null;
let launchLog = [];
let installStatus = null;
let detectedMonitors = [];
let unlistenLaunchLog = null;
let unlistenLaunchStarted = null;
let unlistenLaunchExited = null;

const LAUNCH_OPTIONS = [
  { group: 'Performance', options: [
    { key: 'esync', label: 'ESync', tooltip: 'Eventfd-based synchronization — reduces CPU overhead in Wine' },
    { key: 'fsync', label: 'FSync', tooltip: 'Futex-based synchronization — faster than ESync on supported kernels' },
    { key: 'dxvk_async', label: 'DXVK Async', tooltip: 'Asynchronous shader compilation — reduces stutter' },
  ]},
  { group: 'Display', options: [
    { key: 'wayland', label: 'Wayland', tooltip: 'Enable Wayland protocol support in Wine' },
    { key: 'hdr', label: 'HDR', tooltip: 'High Dynamic Range rendering (PROTON_ENABLE_HDR + DXVK_HDR)' },
    { key: 'fsr', label: 'FSR', tooltip: 'AMD FidelityFX Super Resolution upscaling' },
  ]},
  { group: 'Overlays', options: [
    { key: 'mangohud', label: 'MangoHUD', tooltip: 'On-screen performance overlay (FPS, CPU, GPU, RAM)' },
    { key: 'dxvk_hud', label: 'DXVK HUD', tooltip: 'DXVK-specific overlay showing draw calls and compiler activity' },
  ]},
];

// --- Auto-Launch Flag ---

let pendingAutoLaunch = false;

export function requestAutoLaunch() {
  pendingAutoLaunch = true;
}

// --- Main Render ---

export function renderLaunch(container) {
  launchStatus = 'checking';
  // Load logs from installation process if available
  launchLog = window._starControlLaunchLogs ? [...window._starControlLaunchLogs] : [];
  // Clear the stored logs after loading
  window._starControlLaunchLogs = [];
  renderPage(container);
  loadAndCheck(container);
}

async function loadAndCheck(container) {
  // Detect monitors in parallel
  invoke('detect_monitors').then(monitors => {
    detectedMonitors = monitors || [];
    updateMonitorSelect();
    // Auto-disable Wayland if fractional scaling is active
    if (hasFractionalScaling() && launchConfig?.performance?.wayland) {
      launchConfig.performance.wayland = false;
      renderPage(container);
    }
  }).catch(() => { detectedMonitors = []; });

  try {
    const config = await invoke('load_config');
    if (!config) {
      launchStatus = 'not_installed';
      installStatus = { installed: false, message: 'No configuration found' };
      renderPage(container);
      return;
    }

    launchConfig = config;
    const status = await invoke('check_installation', { config });
    installStatus = status;

    if (status.installed) {
      // Check if the game process is already running (e.g. started by installer)
      const running = await invoke('is_game_running');
      if (running) {
        launchStatus = 'running';
        listenForExit(container);
      } else {
        launchStatus = 'ready';
      }
    } else {
      launchStatus = 'not_installed';
    }
  } catch (err) {
    launchStatus = 'error';
    installStatus = { installed: false, message: String(err) };
  }

  renderPage(container);

  // Auto-launch if requested (e.g. from Dashboard Quick Launch)
  if (pendingAutoLaunch && launchStatus === 'ready') {
    pendingAutoLaunch = false;
    onLaunch(container);
  } else {
    pendingAutoLaunch = false;
  }
}

function listenForExit(container) {
  cleanup();
  listen('launch-exited', () => {
    launchStatus = 'ready';
    renderPage(container);
    cleanup();
  }).then(fn => { unlistenLaunchExited = fn; }).catch(() => {});

  listen('launch-log', (event) => {
    launchLog.push(event.payload);
    appendLogLine(event.payload);
  }).then(fn => { unlistenLaunchLog = fn; }).catch(() => {});
}

function renderPage(container) {
  container.innerHTML = `
    <div class="page-header">
      <h1>Launch</h1>
      <p class="page-subtitle">Start Star Citizen</p>
    </div>
    <div class="launch-section">
      <div class="launch-center">
        ${renderLaunchButton()}
        ${renderLaunchStatus()}
        ${renderLaunchInfo()}
      </div>
      <div class="launch-options card">
        <h3>Launch Options</h3>
        ${renderOptionsGrid()}
      </div>
    </div>
    <div class="card log-panel-flex">
      <h3>Log Output</h3>
      <pre class="log-output log-output-flex" id="launch-log-output"><code>${launchLog.length > 0 ? escapeHtml(launchLog.join('\n')) : 'Waiting for launch...'}</code></pre>
    </div>
  `;

  bindEvents(container);
  scrollLog();
}

// --- Launch Button ---

function renderLaunchButton() {
  const spinning = launchStatus === 'launching';
  const running = launchStatus === 'running';

  if (running) {
    return `
      <button class="btn-launch stop" id="btn-stop">
        <svg class="launch-icon" viewBox="0 0 24 24" fill="currentColor"><rect x="4" y="4" width="16" height="16" rx="2"/></svg>
        <span>STOP</span>
      </button>
    `;
  }

  const disabled = launchStatus !== 'ready';

  let label = 'LAUNCH';
  if (spinning) label = 'LAUNCHING...';
  if (launchStatus === 'checking') label = 'CHECKING...';

  const iconSvg = spinning
    ? '<div class="launch-spinner"></div>'
    : '<svg class="launch-icon" viewBox="0 0 24 24" fill="currentColor"><polygon points="5 3 19 12 5 21 5 3"/></svg>';

  return `
    <button class="btn-launch" id="btn-launch" ${disabled ? 'disabled' : ''}>
      ${iconSvg}
      <span>${label}</span>
    </button>
  `;
}

// --- Status ---

function renderLaunchStatus() {
  if (launchStatus === 'not_installed') {
    const msg = installStatus?.message || 'Star Citizen is not installed';
    return `
      <div class="launch-not-installed">
        <p>${escapeHtml(msg)}</p>
        <button class="btn btn-primary btn-sm" id="btn-goto-install">Go to Installation</button>
      </div>
    `;
  }

  if (launchStatus === 'error') {
    const msg = installStatus?.message || 'An error occurred';
    return `
      <div class="launch-status error">
        <p>${escapeHtml(msg)}</p>
        <button class="btn btn-sm" id="btn-retry-check">Retry</button>
      </div>
    `;
  }

  if (launchStatus === 'running') {
    return '<div class="launch-status running">RSI Launcher is running — press Stop to close</div>';
  }

  if (launchStatus === 'checking') {
    return '<div class="launch-status checking">Checking installation...</div>';
  }

  return '';
}

// --- Info ---

function renderLaunchInfo() {
  if (!launchConfig || !installStatus?.installed) return '';

  const runner = launchConfig.selected_runner || 'None';
  const prefix = launchConfig.install_path || '?';

  const runnerIcon = `<svg class="launch-runner-badge-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></svg>`;

  return `
    <div class="launch-info">
      <div class="launch-info-card">
        <div class="launch-info-card-title">Active Configuration</div>
        <div class="launch-info-row">
          <span class="launch-info-label">Runner</span>
          <span class="launch-runner-badge">
            ${runnerIcon}
            ${escapeHtml(runner)}
          </span>
        </div>
        <div class="launch-info-row">
          <span class="launch-info-label">Prefix</span>
          <span class="launch-prefix-value">${escapeHtml(prefix)}</span>
        </div>
      </div>
    </div>
  `;
}

// --- Options Grid ---

function renderOptionsGrid() {
  const disabled = launchStatus === 'launching' || launchStatus === 'running' || launchStatus === 'not_installed' || launchStatus === 'checking';
  const perf = launchConfig?.performance || {};
  const fractional = hasFractionalScaling();

  return `
    <div class="launch-options-grid">
      ${LAUNCH_OPTIONS.map(group => `
        <div class="launch-option-group">
          <div class="launch-option-group-title">${group.group}</div>
          ${group.options.map(opt => {
            const blockedByScaling = opt.key === 'wayland' && fractional;
            const isDisabled = disabled || blockedByScaling;
            const tooltip = blockedByScaling
              ? 'Fractional scaling detected — Wayland mode is not compatible and has been disabled. Set all monitors to 100% scale to use this option.'
              : (opt.tooltip || '');
            return `
              <label class="toggle-option ${blockedByScaling ? 'toggle-blocked' : ''}" ${tooltip ? `data-tooltip="${tooltip}"` : ''}>
                <input type="checkbox" data-key="${opt.key}"
                  ${!blockedByScaling && perf[opt.key] ? 'checked' : ''}
                  ${isDisabled ? 'disabled' : ''} />
                <span>${opt.label}</span>
              </label>
            `;
          }).join('')}
        </div>
      `).join('')}
    </div>
    ${fractional ? '<div class="launch-scaling-warning">Fractional scaling active — Wayland mode disabled</div>' : ''}
    ${renderMonitorSelect(disabled, perf)}
  `;
}

function renderMonitorSelect(disabled, perf) {
  const hasMonitor = !!perf.primary_monitor;

  let selectHtml;
  if (detectedMonitors.length > 0) {
    const options = detectedMonitors.map(m => {
      const label = `${m.name}${m.resolution ? ' (' + m.resolution : ''}${m.primary ? ', primary)' : m.resolution ? ')' : ''}`;
      const selected = perf.primary_monitor === m.name ? 'selected' : '';
      return `<option value="${escapeHtml(m.name)}" ${selected}>${escapeHtml(label)}</option>`;
    }).join('');
    selectHtml = `<select class="input launch-monitor-input" id="launch-monitor-select" ${!hasMonitor || disabled ? 'disabled' : ''}>${options}</select>`;
  } else {
    selectHtml = `<input type="text" class="input launch-monitor-input" id="launch-monitor-input" value="${escapeHtml(perf.primary_monitor || '')}" placeholder="e.g. DP-1" ${!hasMonitor || disabled ? 'disabled' : ''} />`;
  }

  return `
    <div class="launch-monitor-row">
      <label class="toggle-option" data-tooltip="Force game to a specific Wayland output. Requires a GE-Proton or Mactan runner with WAYLANDDRV_PRIMARY_MONITOR support." data-tooltip-pos="bottom">
        <input type="checkbox" id="launch-monitor-enabled" ${hasMonitor ? 'checked' : ''} ${disabled ? 'disabled' : ''} />
        <span>Wayland Monitor</span>
      </label>
      <div class="launch-monitor-select-wrap ${!hasMonitor ? 'disabled' : ''}" id="launch-monitor-wrap">
        ${selectHtml}
      </div>
    </div>
  `;
}

function updateMonitorSelect() {
  const wrap = document.getElementById('launch-monitor-wrap');
  if (!wrap || detectedMonitors.length === 0) return;

  const perf = launchConfig?.performance || {};
  const hasMonitor = !!perf.primary_monitor;
  const disabled = launchStatus === 'launching' || launchStatus === 'running' || launchStatus === 'not_installed' || launchStatus === 'checking';

  const options = detectedMonitors.map(m => {
    const label = `${m.name}${m.resolution ? ' (' + m.resolution : ''}${m.primary ? ', primary)' : m.resolution ? ')' : ''}`;
    const selected = perf.primary_monitor === m.name ? 'selected' : '';
    return `<option value="${escapeHtml(m.name)}" ${selected}>${escapeHtml(label)}</option>`;
  }).join('');

  wrap.innerHTML = `<select class="input launch-monitor-input" id="launch-monitor-select" ${!hasMonitor || disabled ? 'disabled' : ''}>${options}</select>`;
  bindMonitorSelectListener();
}

function bindMonitorSelectListener() {
  const select = document.getElementById('launch-monitor-select');
  if (select) {
    select.addEventListener('change', () => {
      if (launchConfig) launchConfig.performance.primary_monitor = select.value || null;
    });
  }
  const input = document.getElementById('launch-monitor-input');
  if (input) {
    input.addEventListener('input', () => {
      if (launchConfig) launchConfig.performance.primary_monitor = input.value.trim() || null;
    });
  }
}

// --- Events ---

function bindEvents(container) {
  const launchBtn = document.getElementById('btn-launch');
  if (launchBtn) {
    launchBtn.addEventListener('click', () => onLaunch(container));
  }

  const stopBtn = document.getElementById('btn-stop');
  if (stopBtn) {
    stopBtn.addEventListener('click', () => onStop(container));
  }

  const gotoBtn = document.getElementById('btn-goto-install');
  if (gotoBtn) {
    gotoBtn.addEventListener('click', () => {
      const link = document.querySelector('.nav-link[data-page="installation"]');
      if (link) link.click();
    });
  }

  const retryBtn = document.getElementById('btn-retry-check');
  if (retryBtn) {
    retryBtn.addEventListener('click', () => {
      launchStatus = 'checking';
      renderPage(container);
      loadAndCheck(container);
    });
  }

  // Toggle listeners
  container.querySelectorAll('.launch-options-grid input[type="checkbox"]').forEach(cb => {
    if (!cb.dataset.key) return;
    cb.addEventListener('change', () => {
      if (launchConfig) {
        launchConfig.performance[cb.dataset.key] = cb.checked;
      }
    });
  });

  // Monitor enabled checkbox
  const monitorCb = document.getElementById('launch-monitor-enabled');
  if (monitorCb) {
    monitorCb.addEventListener('change', () => {
      const wrap = document.getElementById('launch-monitor-wrap');
      if (monitorCb.checked) {
        if (wrap) wrap.classList.remove('disabled');
        const select = document.getElementById('launch-monitor-select');
        const input = document.getElementById('launch-monitor-input');
        if (select) { select.disabled = false; if (launchConfig) launchConfig.performance.primary_monitor = select.value || (detectedMonitors[0]?.name ?? null); }
        if (input) { input.disabled = false; if (launchConfig) launchConfig.performance.primary_monitor = input.value.trim() || null; }
      } else {
        if (launchConfig) launchConfig.performance.primary_monitor = null;
        if (wrap) wrap.classList.add('disabled');
        const select = document.getElementById('launch-monitor-select');
        const input = document.getElementById('launch-monitor-input');
        if (select) select.disabled = true;
        if (input) input.disabled = true;
      }
    });
  }

  bindMonitorSelectListener();
}

async function onLaunch(container) {
  if (launchStatus !== 'ready' || !launchConfig) return;

  launchStatus = 'launching';
  launchLog = [];
  renderPage(container);

  // Save config in case toggles changed
  try {
    await invoke('save_config', { config: launchConfig });
  } catch (e) {
    // non-fatal
  }

  // Listen for log events
  cleanup();

  try {
    unlistenLaunchLog = await listen('launch-log', (event) => {
      const line = event.payload;
      launchLog.push(line);
      appendLogLine(line);
    });

    unlistenLaunchStarted = await listen('launch-started', () => {
      launchStatus = 'running';
      renderPage(container);
    });

    unlistenLaunchExited = await listen('launch-exited', () => {
      launchStatus = 'ready';
      renderPage(container);
      cleanup();
    });
  } catch (e) {
    // listen failed
  }

  try {
    await invoke('launch_game', { config: launchConfig });
    if (launchStatus === 'launching') {
      launchStatus = 'running';
      renderPage(container);
    }
  } catch (err) {
    launchStatus = 'error';
    installStatus = { installed: true, message: String(err) };
    launchLog.push(`ERROR: ${err}`);
    renderPage(container);
    cleanup();
  }
}

async function onStop(container) {
  if (launchStatus !== 'running') return;

  try {
    await invoke('stop_game');
  } catch (err) {
    launchLog.push(`ERROR stopping: ${err}`);
    appendLogLine(`ERROR stopping: ${err}`);
  }
}

function appendLogLine(text) {
  const logEl = document.getElementById('launch-log-output');
  if (!logEl) return;

  const code = logEl.querySelector('code');
  if (code) {
    if (launchLog.length === 1 && code.textContent === 'Waiting for launch...') {
      code.textContent = '';
    }
    code.textContent += (code.textContent ? '\n' : '') + text;
  }

  scrollLog();
}

function scrollLog() {
  const logEl = document.getElementById('launch-log-output');
  if (logEl) {
    logEl.scrollTop = logEl.scrollHeight;
  }
}

function cleanup() {
  if (unlistenLaunchLog) { unlistenLaunchLog(); unlistenLaunchLog = null; }
  if (unlistenLaunchStarted) { unlistenLaunchStarted(); unlistenLaunchStarted = null; }
  if (unlistenLaunchExited) { unlistenLaunchExited(); unlistenLaunchExited = null; }
}

// --- Fractional Scaling ---

function hasFractionalScaling() {
  return detectedMonitors.some(m => m.scale != null && Math.abs(m.scale - 1.0) > 0.01);
}

// --- Helpers ---

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
