import { invoke } from '@tauri-apps/api/core';
import { router } from '../router.js';
import { requestAutoLaunch } from './launch.js';

let dashConfig = null;
let dashInstallStatus = null;

export function renderDashboard(container) {
  container.innerHTML = `
    <div class="page-header">
      <h1>Dashboard</h1>
      <p class="page-subtitle">Star Citizen Manager Overview</p>
    </div>
    <div class="card-grid" id="dash-grid">
      <div class="card"><div class="card-icon status-unknown">?</div><h3>Star Citizen</h3><p class="card-text">Checking...</p><span class="badge badge-neutral">Loading</span></div>
      <div class="card"><div class="card-icon">&#x1F377;</div><h3>Wine Runner</h3><p class="card-text">Checking...</p><span class="badge badge-neutral">Loading</span></div>
      <div class="card card-launch"><h3>Quick Launch</h3><p class="card-text">Checking installation...</p><button class="btn btn-primary btn-lg" disabled>Launch Star Citizen</button></div>
    </div>
  `;

  loadDashboard();
}

async function loadDashboard() {
  try {
    dashConfig = await invoke('load_config');
  } catch (e) {
    dashConfig = null;
  }

  if (dashConfig) {
    try {
      dashInstallStatus = await invoke('check_installation', { config: dashConfig });
    } catch (e) {
      dashInstallStatus = null;
    }
  }

  renderCards();
}

function renderCards() {
  const grid = document.getElementById('dash-grid');
  if (!grid) return;

  const installed = dashInstallStatus?.installed === true;
  const hasRunner = dashInstallStatus?.has_runner === true;
  const runnerName = dashConfig?.selected_runner || null;
  const installPath = dashConfig?.install_path || null;

  // Star Citizen card
  let scIcon, scText, scBadge;
  if (!dashConfig) {
    scIcon = '<div class="card-icon status-unknown">?</div>';
    scText = 'No configuration found';
    scBadge = '<span class="badge badge-neutral">Not configured</span>';
  } else if (installed) {
    scIcon = '<div class="card-icon status-ok">&#x2713;</div>';
    scText = installPath || 'Installed';
    scBadge = '<span class="badge badge-ok">Installed</span>';
  } else {
    scIcon = '<div class="card-icon status-warn">!</div>';
    scText = dashInstallStatus?.message || 'Not fully installed';
    scBadge = '<span class="badge badge-warn">Incomplete</span>';
  }

  // Runner card
  let runnerText, runnerBadge;
  if (!runnerName) {
    runnerText = 'No runner selected';
    runnerBadge = '<span class="badge badge-neutral">Not configured</span>';
  } else if (hasRunner) {
    runnerText = runnerName;
    runnerBadge = '<span class="badge badge-ok">Ready</span>';
  } else {
    runnerText = `${runnerName} (not found)`;
    runnerBadge = '<span class="badge badge-warn">Missing</span>';
  }

  // Quick Launch card
  let launchText, launchBtnDisabled;
  if (installed) {
    launchText = 'Star Citizen is ready to launch';
    launchBtnDisabled = false;
  } else {
    launchText = 'Complete installation first';
    launchBtnDisabled = true;
  }

  grid.innerHTML = `
    <div class="card">
      ${scIcon}
      <h3>Star Citizen</h3>
      <p class="card-text">${escapeHtml(scText)}</p>
      ${scBadge}
    </div>
    <div class="card">
      <div class="card-icon">&#x1F377;</div>
      <h3>Wine Runner</h3>
      <p class="card-text">${escapeHtml(runnerText)}</p>
      ${runnerBadge}
    </div>
    <div class="card card-launch">
      <h3>Quick Launch</h3>
      <p class="card-text">${escapeHtml(launchText)}</p>
      <button class="btn btn-primary btn-lg" id="dash-launch-btn" ${launchBtnDisabled ? 'disabled' : ''}>Launch Star Citizen</button>
    </div>
  `;

  const btn = document.getElementById('dash-launch-btn');
  if (btn && !launchBtnDisabled) {
    btn.addEventListener('click', () => {
      requestAutoLaunch();
      router.navigate('launch');
    });
  }
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
