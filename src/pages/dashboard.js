import { invoke } from '@tauri-apps/api/core';
import { openUrl, openPath } from '@tauri-apps/plugin-opener';
import { router } from '../router.js';
import { requestAutoLaunch } from './launch.js';

let dashConfig = null;
let dashInstallStatus = null;
let dashLocStatus = null;
let dashLocUpdate = null;
let dashScVersion = null;

export function renderDashboard(container) {
  container.innerHTML = `
    <div class="page-header">
      <h1>Command Center</h1>
      <p class="page-subtitle">News, Status & Community at a glance</p>
    </div>
    <div class="dash-status-row" id="dash-status-row">
      ${renderStatusSkeleton()}
    </div>
    <div class="dash-main">
      <div class="dash-panel" id="dash-news-panel">
        <div class="dash-panel-title"><span class="dash-panel-title-icon">&#9783;</span> RSI News</div>
        <div id="dash-news-content">${renderNewsSkeleton()}</div>
      </div>
      <div class="dash-right-col">
        <div class="dash-panel" id="dash-status-panel">
          <div class="dash-panel-title"><span class="dash-panel-title-icon">&#9673;</span> Server Status</div>
          <div id="dash-server-content">${renderServerSkeleton()}</div>
        </div>
        <div class="dash-panel" id="dash-stats-panel">
          <div class="dash-panel-title"><span class="dash-panel-title-icon">&#9734;</span> Community</div>
          <div id="dash-stats-content">${renderStatsSkeleton()}</div>
        </div>
      </div>
    </div>
  `;

  loadAll();
}

function renderStatusSkeleton() {
  return `
    <div class="dash-card dash-card--neutral">
      <div class="dash-card-header"><span class="dash-card-title">Star Citizen</span><span class="badge badge-neutral">Loading</span></div>
      <div class="dash-card-body"><div class="dash-skeleton dash-skeleton-line medium"></div><div class="dash-skeleton dash-skeleton-line short"></div></div>
    </div>
    <div class="dash-card dash-card--neutral">
      <div class="dash-card-header"><span class="dash-card-title">Wine Runner</span><span class="badge badge-neutral">Loading</span></div>
      <div class="dash-card-body"><div class="dash-skeleton dash-skeleton-line medium"></div></div>
    </div>
    <div class="dash-card dash-card--launch">
      <div class="dash-card-launch-inner"><span class="dash-card-launch-label">Checking...</span><button class="btn btn-primary btn-lg" disabled>Launch Star Citizen</button></div>
    </div>
  `;
}

function renderNewsSkeleton() {
  let html = '<div class="dash-news-list">';
  for (let i = 0; i < 4; i++) {
    html += `
      <div style="padding: 12px;">
        <div class="dash-skeleton dash-skeleton-line" style="width:${70 + i * 5}%"></div>
        <div class="dash-skeleton dash-skeleton-line short"></div>
      </div>`;
  }
  html += '</div>';
  return html;
}

function renderServerSkeleton() {
  let html = '<div class="dash-server-list">';
  for (let i = 0; i < 3; i++) {
    html += '<div class="dash-skeleton dash-skeleton-block"></div>';
  }
  html += '</div>';
  return html;
}

function renderStatsSkeleton() {
  let html = '<div class="dash-stats-grid">';
  for (let i = 0; i < 3; i++) {
    html += '<div class="dash-skeleton dash-skeleton-block"></div>';
  }
  html += '</div>';
  return html;
}

async function loadAll() {
  // Load all data sources in parallel
  const localPromise = loadLocalStatus();
  const newsPromise = loadNews();
  const serverPromise = loadServerStatus();
  const statsPromise = loadCommunityStats();

  await Promise.allSettled([localPromise, newsPromise, serverPromise, statsPromise]);
}

// ── Local Status Cards ──────────────────────────────

async function loadLocalStatus() {
  try {
    dashConfig = await invoke('load_config');
  } catch {
    dashConfig = null;
  }

  if (dashConfig) {
    try {
      dashInstallStatus = await invoke('check_installation', { config: dashConfig });
    } catch {
      dashInstallStatus = null;
    }

    if (dashConfig.install_path) {
      try {
        const versions = await invoke('detect_sc_versions', { gamePath: dashConfig.install_path });
        if (versions.length > 0) {
          dashScVersion = versions[0].version;
          dashLocStatus = await invoke('get_localization_status', {
            gamePath: dashConfig.install_path,
            version: dashScVersion,
          });
          if (dashLocStatus?.installed) {
            try {
              dashLocUpdate = await invoke('check_localization_update', {
                gamePath: dashConfig.install_path,
                version: dashScVersion,
              });
            } catch {
              dashLocUpdate = null;
            }
          }
        }
      } catch {
        dashLocStatus = null;
      }
    }
  }

  renderStatusCards();
}

function renderStatusCards() {
  const grid = document.getElementById('dash-status-row');
  if (!grid) return;

  const installed = dashInstallStatus?.installed === true;
  const hasRunner = dashInstallStatus?.has_runner === true;
  const runnerName = dashConfig?.selected_runner || null;
  const installPath = dashConfig?.install_path || null;

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

  let launchText, launchDisabled;
  if (installed) {
    launchText = 'Ready to launch';
    launchDisabled = false;
  } else {
    launchText = 'Complete installation first';
    launchDisabled = true;
  }

  // Determine SC status class
  const scStatusClass = !dashConfig ? 'neutral' : installed ? 'ok' : 'warn';
  const runnerStatusClass = !runnerName ? 'neutral' : hasRunner ? 'ok' : 'warn';
  const locLang = dashLocStatus?.language_name || dashLocStatus?.language_code || null;

  grid.innerHTML = `
    <div class="dash-card dash-card--${scStatusClass}">
      <div class="dash-card-header">
        <span class="dash-card-title">Star Citizen</span>
        ${scBadge}
      </div>
      <div class="dash-card-body">
        <div class="dash-card-row">
          <span class="dash-card-label">Path</span>
          <span class="dash-card-value mono">${escapeHtml(installPath || 'Not configured')}</span>
        </div>
        ${dashLocStatus?.installed ? `
        <div class="dash-card-row">
          <span class="dash-card-label">Language</span>
          <span class="dash-card-value">${escapeHtml(locLang)} ${dashLocUpdate?.update_available
            ? '<span class="dash-card-dot dot-warn"></span>'
            : '<span class="dash-card-dot dot-ok"></span>'}</span>
        </div>` : ''}
      </div>
      <div class="dash-card-actions">
        ${installPath ? `<button class="btn btn-sm" id="dash-open-folder">Open Folder</button>` : ''}
        ${dashLocUpdate?.update_available ? `<button class="btn btn-sm dash-btn-update" id="dash-loc-update">Update Translation</button>` : ''}
      </div>
    </div>
    <div class="dash-card dash-card--${runnerStatusClass}">
      <div class="dash-card-header">
        <span class="dash-card-title">Wine Runner</span>
        ${runnerBadge}
      </div>
      <div class="dash-card-body">
        <div class="dash-card-row">
          <span class="dash-card-label">Runner</span>
          <span class="dash-card-value mono">${escapeHtml(runnerName || 'None')}</span>
        </div>
      </div>
      <div class="dash-card-actions"><button class="btn btn-sm" id="dash-manage-runners">Manage Runners</button></div>
    </div>
    <div class="dash-card dash-card--launch">
      <div class="dash-card-launch-inner">
        <span class="dash-card-launch-label">${escapeHtml(launchText)}</span>
        <button class="btn btn-primary btn-lg" id="dash-launch-btn" ${launchDisabled ? 'disabled' : ''}>Launch Star Citizen</button>
      </div>
    </div>
  `;

  const btn = document.getElementById('dash-launch-btn');
  if (btn && !launchDisabled) {
    btn.addEventListener('click', () => {
      requestAutoLaunch();
      router.navigate('launch');
    });
  }

  const folderBtn = document.getElementById('dash-open-folder');
  if (folderBtn && installPath) {
    folderBtn.addEventListener('click', () => openPath(installPath));
  }

  const runnersBtn = document.getElementById('dash-manage-runners');
  if (runnersBtn) {
    runnersBtn.addEventListener('click', () => router.navigate('runners'));
  }

  const updateBtn = document.getElementById('dash-loc-update');
  if (updateBtn && dashLocStatus && dashScVersion) {
    updateBtn.addEventListener('click', async () => {
      updateBtn.disabled = true;
      updateBtn.textContent = 'Updating...';
      try {
        const languages = await invoke('get_available_languages');
        const source = languages.find(l => l.language_code === dashLocStatus.language_code);
        if (source) {
          await invoke('install_localization', {
            gamePath: dashConfig.install_path,
            version: dashScVersion,
            languageCode: source.language_code,
            sourceRepo: source.source_repo,
            languageName: source.language_name,
            sourceLabel: source.source_label,
          });
          dashLocUpdate = null;
          await loadLocalStatus();
        }
      } catch {
        updateBtn.textContent = 'Update failed';
        updateBtn.disabled = false;
      }
    });
  }
}

// ── RSI News ──────────────────────────────────────

async function loadNews() {
  const el = document.getElementById('dash-news-content');
  if (!el) return;

  try {
    const result = await invoke('fetch_rsi_news');
    if (result.error && result.items.length === 0) {
      el.innerHTML = renderError('Could not load news', () => loadNews());
      return;
    }
    renderNewsItems(el, result.items);
  } catch {
    el.innerHTML = renderError('Could not load news', () => loadNews());
  }
}

function renderNewsItems(el, items) {
  if (items.length === 0) {
    el.innerHTML = '<div class="dash-error"><span class="dash-error-msg">No news available</span></div>';
    return;
  }

  let html = '<div class="dash-news-list">';
  for (const item of items) {
    const category = item.category
      ? `<span class="dash-news-category">${escapeHtml(item.category)}</span>`
      : '';
    html += `
      <div class="dash-news-item" data-url="${escapeAttr(item.link)}">
        <div class="dash-news-header">
          <span class="dash-news-title">${escapeHtml(item.title)}</span>
          <span class="dash-news-meta">${escapeHtml(item.relative_time)}</span>
        </div>
        <div class="dash-news-summary">${category}${escapeHtml(item.summary)}</div>
      </div>`;
  }
  html += '</div>';
  el.innerHTML = html;

  el.querySelectorAll('.dash-news-item').forEach(item => {
    item.addEventListener('click', () => {
      const url = item.dataset.url;
      if (url) openUrl(url);
    });
  });
}

// ── Server Status ──────────────────────────────────

async function loadServerStatus() {
  const el = document.getElementById('dash-server-content');
  if (!el) return;

  try {
    const result = await invoke('fetch_server_status');
    if (result.error && result.components.length === 0) {
      el.innerHTML = renderError('Could not load server status', () => loadServerStatus());
      return;
    }
    renderServerComponents(el, result.components);
  } catch {
    el.innerHTML = renderError('Could not load server status', () => loadServerStatus());
  }
}

function renderServerComponents(el, components) {
  if (components.length === 0) {
    el.innerHTML = '<div class="dash-error"><span class="dash-error-msg">No status data</span></div>';
    return;
  }

  const statusLabels = {
    operational: 'Operational',
    degraded: 'Degraded',
    major_outage: 'Major Outage',
    unknown: 'Unknown',
  };

  let html = '<div class="dash-server-list">';
  for (const comp of components) {
    const label = statusLabels[comp.status] || 'Unknown';
    html += `
      <div class="dash-server-row">
        <span class="dash-server-name">${escapeHtml(comp.name)}</span>
        <span class="dash-server-badge">
          <span class="dash-status-dot ${comp.status}"></span>
          <span class="${comp.status}">${label}</span>
        </span>
      </div>`;
  }
  html += '</div>';
  el.innerHTML = html;
}

// ── Community Stats ──────────────────────────────────

async function loadCommunityStats() {
  const el = document.getElementById('dash-stats-content');
  if (!el) return;

  try {
    const result = await invoke('fetch_community_stats');
    if (result.error || !result.stats) {
      el.innerHTML = renderError('Could not load community stats', () => loadCommunityStats());
      return;
    }
    renderStats(el, result.stats);
  } catch {
    el.innerHTML = renderError('Could not load community stats', () => loadCommunityStats());
  }
}

function renderStats(el, stats) {
  el.innerHTML = `
    <div class="dash-stats-grid">
      <div class="dash-stat-item">
        <div class="dash-stat-value">${escapeHtml(stats.funds)}</div>
        <div class="dash-stat-label">Total Funding</div>
      </div>
      <div class="dash-stat-item">
        <div class="dash-stat-value">${escapeHtml(stats.fans)}</div>
        <div class="dash-stat-label">Star Citizens</div>
      </div>
      <div class="dash-stat-item">
        <div class="dash-stat-value">${escapeHtml(stats.fleet)}</div>
        <div class="dash-stat-label">Ships in Fleet</div>
      </div>
    </div>
  `;
}

// ── Helpers ──────────────────────────────────────────

function renderError(message, retryFn) {
  const id = 'retry-' + Math.random().toString(36).slice(2, 8);
  setTimeout(() => {
    const btn = document.getElementById(id);
    if (btn) btn.addEventListener('click', retryFn);
  }, 0);
  return `
    <div class="dash-error">
      <span class="dash-error-msg">${escapeHtml(message)}</span>
      <button class="dash-retry-btn" id="${id}">Retry</button>
    </div>`;
}

function escapeHtml(str) {
  if (!str) return '';
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

function escapeAttr(str) {
  if (!str) return '';
  return str.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/'/g, '&#39;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}
