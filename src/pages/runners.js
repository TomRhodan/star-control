import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// --- State ---

let config = null;
let installedRunners = [];
let availableRunners = [];
let fetchErrors = [];
let selectedSource = 'LUG';
let isInstallingRunner = false;
let unlistenRunnerProgress = null;
let isActivatingRunner = false;
let activatingRunnerName = '';

let dxvkReleases = [];
let dxvkStatus = null;
let isInstallingDxvk = false;
let unlistenDxvkProgress = null;

let currentDpi = 96;
let prefixToolLog = [];
let isRunningPrefixTool = false;
let unlistenPrefixLog = null;
let powershellInstalled = false;

// Track which sections are still loading
let loadingFlags = { installed: true, available: true, dxvk: true, dxvkReleases: true, dpi: true };

// Cache state
let runnerCache = { runners: [], cached_at: 0 };
let dxvkCache = { releases: [], cached_at: 0 };

// Reference to current container for incremental updates
let activeContainer = null;

// --- Main Render ---

export function renderRunners(container) {
  // Reset state
  config = null;
  installedRunners = [];
  availableRunners = [];
  fetchErrors = [];
  dxvkReleases = [];
  dxvkStatus = null;
  currentDpi = 96;
  loadingFlags = { installed: true, available: true, dxvk: true, dxvkReleases: true, dpi: true };
  activeContainer = container;

  // Render loading skeleton immediately
  container.innerHTML = `
    <div class="page-header">
      <h1>Wine Runners</h1>
      <p class="page-subtitle">Manage Wine/Proton compatibility layers</p>
    </div>
    <div class="card">
      <div class="runners-loading-state">
        <div class="runners-loading-spinner"></div>
        <span>Loading configuration...</span>
      </div>
    </div>
  `;

  // Start loading on next macrotask so the spinner paints first
  setTimeout(() => loadData(container), 0);
}

function loadData(container) {
  // Load config and caches in parallel
  Promise.all([
    invoke('load_config').catch(() => null),
    invoke('load_runner_cache').catch(() => ({ runners: [], cached_at: 0 })),
    invoke('load_dxvk_cache').catch(() => ({ releases: [], cached_at: 0 })),
  ]).then(([cfg, runnerCacheData, dxvkCacheData]) => {
    config = cfg;
    runnerCache = runnerCacheData;
    dxvkCache = dxvkCacheData;

    if (activeContainer !== container) return;

    if (!config) {
      renderNoConfig(container);
      return;
    }

    // Use cached data immediately if available (less than 1 hour old)
    // Note: installed flags will be synced in fireDataFetches after scan_runners completes
    const cacheAge = Date.now() / 1000 - (runnerCache.cached_at || 0);
    const dxvkCacheAge = Date.now() / 1000 - (dxvkCache.cached_at || 0);

    if (runnerCache.runners && runnerCache.runners.length > 0 && cacheAge < 3600) {
      // Keep cached runners but flags will be synced in fireDataFetches
      availableRunners = runnerCache.runners.map(r => ({ ...r, installed: false }));
      loadingFlags.available = false;
    }
    if (dxvkCache.releases && dxvkCache.releases.length > 0 && dxvkCacheAge < 3600) {
      dxvkReleases = dxvkCache.releases;
      loadingFlags.dxvkReleases = false;
    }

    // Render skeleton — yield a frame so the browser paints it
    renderPageSkeleton(container);

    // Wait for a real paint frame, then fire data fetches for missing/expired data
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (activeContainer !== container) return;
        fireDataFetches(container);
      });
    });
  });
}

function syncAvailableRunners(container, forceRefresh) {
  // Only fetch available runners if cache is empty/expired or force refresh
  const cacheAge = Date.now() / 1000 - (runnerCache.cached_at || 0);
  if (forceRefresh || !runnerCache.runners || runnerCache.runners.length === 0 || cacheAge >= 3600) {
    invoke('fetch_available_runners', { basePath: config.install_path }).then(result => {
      if (activeContainer !== container) return;
      availableRunners = result.runners || [];
      fetchErrors = result.errors || [];
      // Sync installed flags
      const installedNames = new Set(installedRunners.map(r => r.name));
      availableRunners = availableRunners.map(r => ({ ...r, installed: installedNames.has(r.name) }));
      loadingFlags.available = false;

      // Save to cache
      invoke('save_runner_cache', { runners: availableRunners }).catch(() => {});

      patchSection('download-runners-slot', renderDownloadRunnersContent());
      bindDownloadRunnerEvents(container);
    }).catch(err => {
      fetchErrors = [String(err)];
      loadingFlags.available = false;
      // Use cached data on error
      if (runnerCache.runners && runnerCache.runners.length > 0) {
        availableRunners = runnerCache.runners;
        const installedNames = new Set(installedRunners.map(r => r.name));
        availableRunners = availableRunners.map(r => ({ ...r, installed: installedNames.has(r.name) }));
      }
      patchSection('download-runners-slot', renderDownloadRunnersContent());
    });
  } else {
    // Sync installed flags with cached data
    const installedNames = new Set(installedRunners.map(r => r.name));
    availableRunners = runnerCache.runners.map(r => ({ ...r, installed: installedNames.has(r.name) }));
    loadingFlags.available = false;
    patchSection('download-runners-slot', renderDownloadRunnersContent());
    bindDownloadRunnerEvents(container);
  }
}

function fireDataFetches(container, forceRefresh = false) {
  // First scan installed runners
  invoke('scan_runners', { basePath: config.install_path }).then(result => {
    if (activeContainer !== container) return;
    installedRunners = result.runners || [];
    loadingFlags.installed = false;
    patchSection('installed-runners-slot', renderInstalledRunnersContent());
    bindInstalledRunnerEvents(container);

    // After scan completes, sync available runners
    syncAvailableRunners(container, forceRefresh);
  }).catch(() => {
    loadingFlags.installed = false;
    patchSection('installed-runners-slot', renderInstalledRunnersContent());
    // Even on error, try to sync with empty installed runners
    syncAvailableRunners(container, forceRefresh);
  });

  // Also start DXVK detection in parallel
  invoke('detect_dxvk_version', { basePath: config.install_path }).then(result => {
    if (activeContainer !== container) return;
    dxvkStatus = result;
    loadingFlags.dxvk = false;
    patchSection('dxvk-status-slot', renderDxvkStatusContent());
    // Re-render releases after detection completes to show correct "Current" status
    patchSection('dxvk-releases-slot', renderDxvkReleasesContent());
    bindDxvkEvents(container);
  }).catch(() => {
    loadingFlags.dxvk = false;
    patchSection('dxvk-status-slot', renderDxvkStatusContent());
    // Also re-render releases on error
    patchSection('dxvk-releases-slot', renderDxvkReleasesContent());
    bindDxvkEvents(container);
  });

  // Only fetch DXVK releases if cache is empty/expired or force refresh
  const dxvkCacheAge = Date.now() / 1000 - (dxvkCache.cached_at || 0);
  if (forceRefresh || !dxvkReleases.length || dxvkCacheAge >= 3600) {
    invoke('fetch_dxvk_releases').then(result => {
      if (activeContainer !== container) return;
      dxvkReleases = result || [];
      loadingFlags.dxvkReleases = false;

      // Save to cache
      invoke('save_dxvk_cache', { releases: dxvkReleases }).catch(() => {});

      patchSection('dxvk-releases-slot', renderDxvkReleasesContent());
      bindDxvkEvents(container);
    }).catch(() => {
      loadingFlags.dxvkReleases = false;
      // Use cached data on error
      if (dxvkCache.releases && dxvkCache.releases.length > 0) {
        dxvkReleases = dxvkCache.releases;
      }
      patchSection('dxvk-releases-slot', renderDxvkReleasesContent());
    });
  } else {
    loadingFlags.dxvkReleases = false;
    patchSection('dxvk-releases-slot', renderDxvkReleasesContent());
    bindDxvkEvents(container);
  }

  if (config.selected_runner) {
    invoke('get_dpi', { basePath: config.install_path, runnerName: config.selected_runner }).then(result => {
      if (activeContainer !== container) return;
      currentDpi = result || 96;
      loadingFlags.dpi = false;
      patchSection('prefix-tools-slot', renderPrefixToolsContent());
      bindPrefixToolEvents(container);
    }).catch(() => {
      loadingFlags.dpi = false;
      patchSection('prefix-tools-slot', renderPrefixToolsContent());
      bindPrefixToolEvents(container);
    });

    // Detect PowerShell status
    invoke('detect_powershell', { basePath: config.install_path }).then(result => {
      if (activeContainer !== container) return;
      powershellInstalled = result;
      patchSection('prefix-tools-slot', renderPrefixToolsContent());
      bindPrefixToolEvents(container);
    }).catch(() => {
      // Ignore errors, PowerShell detection is optional
    });
  } else {
    loadingFlags.dpi = false;
  }
}

// --- DOM Patching ---

function patchSection(slotId, html) {
  const slot = document.getElementById(slotId);
  if (slot) slot.innerHTML = html;
}

// --- Page Skeleton ---

function renderNoConfig(container) {
  container.innerHTML = `
    <div class="page-header">
      <h1>Wine Runners</h1>
      <p class="page-subtitle">Manage Wine/Proton compatibility layers</p>
    </div>
    <div class="card">
      <div class="runners-guard-notice">
        <div class="runners-guard-icon">\u2699</div>
        <h3>Configuration Required</h3>
        <p>Please run the Installation wizard first to set up your install path.</p>
        <button class="btn btn-primary" id="btn-goto-install">Go to Installation</button>
      </div>
    </div>
  `;
  const gotoBtn = document.getElementById('btn-goto-install');
  if (gotoBtn) {
    gotoBtn.addEventListener('click', () => {
      const link = document.querySelector('.nav-link[data-page="installation"]');
      if (link) link.click();
    });
  }
}

function renderPageSkeleton(container) {
  const hasRunner = !!config.selected_runner;
  const hasPrefix = !!config.install_path;
  const spinner = `<div class="runners-loading-state"><div class="runners-loading-spinner"></div><span>Loading...</span></div>`;

  container.innerHTML = `
    <div class="page-header">
      <h1>Wine Runners</h1>
      <p class="page-subtitle">Manage Wine/Proton compatibility layers</p>
    </div>

    <div class="card">
      <div class="card-header-row">
        <h3 data-tooltip="Wine/Proton runners available on your system" data-tooltip-pos="right">Installed Runners</h3>
        <button class="btn-sm" id="btn-refresh-installed">Refresh</button>
      </div>
      <div id="installed-runners-slot">${spinner}</div>
    </div>

    <div class="card">
      <div class="card-header-row">
        <h3 data-tooltip="Download Wine/Proton runners from community sources" data-tooltip-pos="right">Download Runners</h3>
        <span class="runner-cache-time">Cached: ${formatCacheTime(runnerCache.cached_at)}</span>
        <button class="btn-sm" id="btn-refresh-available">Refresh</button>
      </div>
      <div class="runner-source-tabs" id="source-tabs">
        ${['LUG', 'Mactan', 'Kron4ek', 'RawFox'].map(s => `
          <button class="source-tab ${selectedSource === s ? 'active' : ''}" data-source="${s}">${s}</button>
        `).join('')}
      </div>
      <div id="download-runners-slot">${spinner}</div>
      <div class="runner-install-overlay" id="runner-install-overlay" style="display:none">
        <div class="runner-install-name" id="install-runner-name"></div>
        <div class="progress-bar-track">
          <div class="progress-bar-fill" id="install-progress-fill"></div>
        </div>
        <div class="runner-install-status" id="install-status">Preparing...</div>
        <button class="btn-sm" id="btn-cancel-runner-install">Cancel</button>
      </div>
    </div>

    <div class="runners-tools-grid">
      <div class="card">
        <div class="card-header-row">
          <h3 data-tooltip="DirectX to Vulkan translation layer" data-tooltip-pos="right">DXVK</h3>
          <button class="btn-sm" id="btn-refresh-dxvk">Refresh</button>
        </div>
        ${hasPrefix
          ? `<div id="dxvk-status-slot">${spinner}</div>
             <div id="dxvk-releases-slot">${spinner}</div>
             <div class="runner-install-overlay" id="dxvk-install-overlay" style="display:none">
               <div class="runner-install-name" id="dxvk-install-name"></div>
               <div class="progress-bar-track">
                 <div class="progress-bar-fill" id="dxvk-progress-fill"></div>
               </div>
               <div class="runner-install-status" id="dxvk-install-status">Preparing...</div>
             </div>`
          : '<div class="runners-guard-notice-inline">Run Installation first to manage DXVK.</div>'
        }
      </div>

      <div class="card">
        <h3 data-tooltip="Wine prefix configuration and utility tools" data-tooltip-pos="right">Prefix Tools</h3>
        <div id="prefix-tools-slot">
          ${hasRunner && hasPrefix
            ? spinner
            : `<div class="runners-guard-notice-inline">${!hasRunner ? 'Select a runner first to use prefix tools.' : 'Run Installation first to use prefix tools.'}</div>`
          }
        </div>
      </div>
    </div>
  `;

  // Bind skeleton-level events that don't change
  bindSkeletonEvents(container);
}

// --- Section Content Renderers (just the inner content, not the card wrapper) ---

function renderInstalledRunnersContent() {
  if (installedRunners.length === 0) {
    return '<div class="runner-empty-notice">No runners installed yet. Download one below.</div>';
  }

  const activeRunner = installedRunners.find(r => config.selected_runner === r.name);
  const otherRunners = installedRunners.filter(r => config.selected_runner !== r.name);

  // Active runner display
  let activeHtml;
  if (activeRunner) {
    activeHtml = `
      <div class="active-runner-display">
        <div class="active-runner-label">Active Runner</div>
        <div class="active-runner-name">
          <span class="installed-runner-indicator active"></span>
          ${escapeHtml(activeRunner.name)}
        </div>
      </div>
    `;
  } else {
    activeHtml = `
      <div class="active-runner-display empty">
        <div class="active-runner-label">Active Runner</div>
        <div class="active-runner-name-empty">No runner selected</div>
      </div>
    `;
  }

  // Activating overlay
  let activatingHtml = '';
  if (isActivatingRunner) {
    activatingHtml = `
      <div class="runner-activating-overlay">
        <div class="runners-loading-spinner"></div>
        <span>Activating ${escapeHtml(activatingRunnerName)}...</span>
      </div>
    `;
  }

  // Other installed runners
  let listHtml = '';
  if (otherRunners.length > 0) {
    listHtml = `
      <div class="installed-runners-list">
        ${otherRunners.map(r => `
          <div class="installed-runner-item">
            <div class="installed-runner-info">
              <span class="installed-runner-indicator"></span>
              <span class="installed-runner-name">${escapeHtml(r.name)}</span>
            </div>
            <div class="installed-runner-actions">
              <button class="btn-sm btn-select-runner" data-name="${escapeHtml(r.name)}" ${isActivatingRunner ? 'disabled' : ''}>Select</button>
              <button class="btn-sm btn-danger-sm btn-delete-runner" data-name="${escapeHtml(r.name)}" ${isActivatingRunner ? 'disabled' : ''}>Delete</button>
            </div>
          </div>
        `).join('')}
      </div>
    `;
  }

  return activeHtml + activatingHtml + listHtml;
}

function renderDownloadRunnersContent() {
  const filtered = availableRunners.filter(r => r.source === selectedSource);

  const errorsHtml = fetchErrors.length > 0
    ? fetchErrors.map(e => `<div class="runner-fetch-error">${escapeHtml(e)}</div>`).join('')
    : '';

  let listHtml;
  if (filtered.length === 0 && fetchErrors.length === 0) {
    listHtml = '<div class="runner-empty-notice">No runners available from this source.</div>';
  } else if (filtered.length === 0) {
    listHtml = '';
  } else {
    listHtml = `
      <div class="runner-available-list" id="runner-available-list">
        ${filtered.map(r => `
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
              : `<button class="btn-sm btn-install" data-url="${escapeHtml(r.download_url)}" data-file="${escapeHtml(r.file_name)}" data-name="${escapeHtml(r.name)}">Install</button>`
            }
          </div>
        `).join('')}
      </div>
    `;
  }

  return errorsHtml + listHtml;
}

function renderDxvkStatusContent() {
  if (dxvkStatus && dxvkStatus.installed) {
    return `
      <div class="dxvk-current">
        <span class="dxvk-current-label">Current:</span>
        <span class="dxvk-current-version">${escapeHtml(dxvkStatus.version || 'Unknown')}</span>
      </div>
      <div class="dxvk-dll-badges">
        ${dxvkStatus.dlls_found.map(dll => `<span class="dxvk-dll-badge">${escapeHtml(dll)}</span>`).join('')}
      </div>
    `;
  }
  return '<div class="dxvk-current"><span class="dxvk-current-label">Not installed</span></div>';
}

function renderDxvkReleasesContent() {
  if (dxvkReleases.length === 0) {
    return '<div class="runner-empty-notice">No releases found.</div>';
  }

  return `
    <div class="dxvk-releases-list">
      ${dxvkReleases.map(r => {
        const isCurrent = dxvkStatus && dxvkStatus.version === r.version;
        return `
          <div class="runner-available-item ${isCurrent ? 'installed' : ''}">
            <div class="runner-item-info">
              <span class="runner-item-name">${escapeHtml(r.version)}</span>
              <span class="runner-item-meta">
                <span class="runner-item-size">${formatSize(r.size_bytes)}</span>
              </span>
            </div>
            ${isCurrent
              ? '<span class="runner-installed-badge">Current</span>'
              : `<button class="btn-sm btn-install btn-install-dxvk" data-url="${escapeHtml(r.download_url)}" data-version="${escapeHtml(r.version)}">Install</button>`
            }
          </div>
        `;
      }).join('')}
    </div>
  `;
}

function renderPrefixToolsContent() {
  if (!config.selected_runner || !config.install_path) {
    const msg = !config.selected_runner
      ? 'Select a runner first to use prefix tools.'
      : 'Run Installation first to use prefix tools.';
    return `<div class="runners-guard-notice-inline">${msg}</div>`;
  }

  const logHtml = prefixToolLog.length > 0
    ? `<div class="prefix-tool-log" id="prefix-tool-log"><code>${escapeHtml(prefixToolLog.join('\n'))}</code></div>`
    : '';

  return `
    <div class="prefix-tool-row">
      <div class="prefix-tool-info">
        <span class="prefix-tool-name">Winecfg</span>
        <span class="prefix-tool-hint">Open Wine configuration window</span>
      </div>
      <button class="btn-sm btn-install" id="btn-winecfg">Launch</button>
    </div>

    <div class="prefix-tool-divider"></div>

    <div class="prefix-tool-row">
      <div class="prefix-tool-info">
        <span class="prefix-tool-name">DPI Setting</span>
        <span class="prefix-tool-hint">Current: ${currentDpi} DPI</span>
      </div>
      <div class="dpi-presets" id="dpi-presets">
        ${[
          { v: 96, tip: '100% scaling (default)' },
          { v: 120, tip: '125% scaling' },
          { v: 144, tip: '150% scaling' },
          { v: 192, tip: '200% scaling (HiDPI)' },
        ].map(({ v, tip }) => `
          <button class="dpi-btn ${currentDpi === v ? 'active' : ''}" data-dpi="${v}" data-tooltip="${tip}">${v}</button>
        `).join('')}
      </div>
    </div>

    <div class="prefix-tool-divider"></div>

    <div class="prefix-tool-row">
      <div class="prefix-tool-info">
        <span class="prefix-tool-name">PowerShell</span>
        <span class="prefix-tool-hint">${powershellInstalled ? 'Installed' : 'Install via winetricks (takes several minutes)'}</span>
      </div>
      ${powershellInstalled
        ? '<span class="runner-installed-badge">Installed</span>'
        : `<button class="btn-sm btn-install" id="btn-install-powershell" ${isRunningPrefixTool ? 'disabled' : ''}>
            ${isRunningPrefixTool ? 'Installing...' : 'Install'}
          </button>`
      }
    </div>

    ${logHtml}
  `;
}

// --- Event Binding ---

function bindSkeletonEvents(container) {
  // Refresh installed runners
  const refreshInstalledBtn = document.getElementById('btn-refresh-installed');
  if (refreshInstalledBtn) {
    refreshInstalledBtn.addEventListener('click', () => {
      loadingFlags.installed = true;
      patchSection('installed-runners-slot', '<div class="runners-loading-state"><div class="runners-loading-spinner"></div><span>Loading...</span></div>');
      fireDataFetches(container, false);
    });
  }

  // Refresh available runners
  const refreshAvailableBtn = document.getElementById('btn-refresh-available');
  if (refreshAvailableBtn) {
    refreshAvailableBtn.addEventListener('click', () => {
      loadingFlags.available = true;
      patchSection('download-runners-slot', '<div class="runners-loading-state"><div class="runners-loading-spinner"></div><span>Refreshing...</span></div>');
      fireDataFetches(container, true);
    });
  }

  // Refresh DXVK
  const refreshDxvkBtn = document.getElementById('btn-refresh-dxvk');
  if (refreshDxvkBtn) {
    refreshDxvkBtn.addEventListener('click', () => {
      loadingFlags.dxvkReleases = true;
      patchSection('dxvk-releases-slot', '<div class="runners-loading-state"><div class="runners-loading-spinner"></div><span>Refreshing...</span></div>');
      fireDataFetches(container, true);
    });
  }

  // Source tabs
  container.querySelectorAll('#source-tabs .source-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      selectedSource = tab.dataset.source;
      container.querySelectorAll('#source-tabs .source-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      patchSection('download-runners-slot', renderDownloadRunnersContent());
      bindDownloadRunnerEvents(container);
    });
  });

  // Cancel runner install
  const cancelBtn = document.getElementById('btn-cancel-runner-install');
  if (cancelBtn) {
    cancelBtn.addEventListener('click', async () => {
      try { await invoke('cancel_runner_install'); } catch { /* ignore */ }
    });
  }
}

function bindInstalledRunnerEvents(container) {
  const slot = document.getElementById('installed-runners-slot');
  if (!slot) return;

  slot.querySelectorAll('.btn-select-runner').forEach(btn => {
    btn.addEventListener('click', () => selectRunner(btn.dataset.name, container));
  });

  slot.querySelectorAll('.btn-delete-runner').forEach(btn => {
    btn.addEventListener('click', () => deleteRunner(btn.dataset.name, container));
  });
}

function bindDownloadRunnerEvents(container) {
  const slot = document.getElementById('download-runners-slot');
  if (!slot) return;

  slot.querySelectorAll('.btn-install').forEach(btn => {
    btn.addEventListener('click', () => {
      installRunner(btn.dataset.url, btn.dataset.file, btn.dataset.name, container);
    });
  });
}

function bindDxvkEvents(container) {
  const slot = document.getElementById('dxvk-releases-slot');
  if (!slot) return;

  slot.querySelectorAll('.btn-install-dxvk').forEach(btn => {
    btn.addEventListener('click', () => {
      installDxvk(btn.dataset.url, btn.dataset.version, container);
    });
  });
}

function bindPrefixToolEvents(container) {
  const winecfgBtn = document.getElementById('btn-winecfg');
  if (winecfgBtn) {
    winecfgBtn.addEventListener('click', launchWinecfg);
  }

  document.querySelectorAll('.dpi-btn').forEach(btn => {
    btn.addEventListener('click', () => setDpi(parseInt(btn.dataset.dpi), container));
  });

  const psBtn = document.getElementById('btn-install-powershell');
  if (psBtn) {
    psBtn.addEventListener('click', () => installPowershell(container));
  }

  scrollPrefixLog();
}

// --- Actions ---

async function selectRunner(name, container) {
  if (isActivatingRunner) return;
  isActivatingRunner = true;
  activatingRunnerName = name;

  // Show activating state immediately
  patchSection('installed-runners-slot', renderInstalledRunnersContent());
  bindInstalledRunnerEvents(container);

  config.selected_runner = name;
  try {
    await invoke('save_config', { config });
    try {
      currentDpi = await invoke('get_dpi', { basePath: config.install_path, runnerName: name });
    } catch {
      currentDpi = 96;
    }
  } catch (err) {
    console.error('Failed to save config:', err);
  }

  isActivatingRunner = false;
  activatingRunnerName = '';

  // Update installed runners + prefix tools sections
  patchSection('installed-runners-slot', renderInstalledRunnersContent());
  bindInstalledRunnerEvents(container);
  patchSection('prefix-tools-slot', renderPrefixToolsContent());
  bindPrefixToolEvents(container);
}

async function deleteRunner(name, container) {
  if (config.selected_runner === name) return;

  try {
    await invoke('delete_runner', { runnerName: name, basePath: config.install_path });
    installedRunners = installedRunners.filter(r => r.name !== name);
    availableRunners = availableRunners.map(r =>
      r.name === name ? { ...r, installed: false } : r
    );
  } catch (err) {
    console.error('Failed to delete runner:', err);
  }
  patchSection('installed-runners-slot', renderInstalledRunnersContent());
  bindInstalledRunnerEvents(container);
  patchSection('download-runners-slot', renderDownloadRunnersContent());
  bindDownloadRunnerEvents(container);
}

async function installRunner(downloadUrl, fileName, displayName, container) {
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

  if (unlistenRunnerProgress) { unlistenRunnerProgress(); unlistenRunnerProgress = null; }

  try {
    unlistenRunnerProgress = await listen('runner-download-progress', (event) => {
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
  } catch { /* listen failed */ }

  try {
    await invoke('install_runner', { downloadUrl, fileName, basePath: config.install_path });
  } catch (err) {
    if (statusEl) statusEl.textContent = `Error: ${err}`;
  }

  if (unlistenRunnerProgress) { unlistenRunnerProgress(); unlistenRunnerProgress = null; }
  isInstallingRunner = false;

  await delay(1200);
  if (overlay) overlay.style.display = 'none';

  // Refresh installed runners
  try {
    const scanResult = await invoke('scan_runners', { basePath: config.install_path });
    installedRunners = scanResult.runners || [];
    const installedNames = new Set(installedRunners.map(r => r.name));
    availableRunners = availableRunners.map(r => ({ ...r, installed: installedNames.has(r.name) }));
  } catch { /* ignore */ }

  patchSection('installed-runners-slot', renderInstalledRunnersContent());
  bindInstalledRunnerEvents(container);
  patchSection('download-runners-slot', renderDownloadRunnersContent());
  bindDownloadRunnerEvents(container);
}

async function installDxvk(downloadUrl, version, container) {
  if (isInstallingDxvk) return;
  isInstallingDxvk = true;

  const overlay = document.getElementById('dxvk-install-overlay');
  const nameEl = document.getElementById('dxvk-install-name');
  const fillEl = document.getElementById('dxvk-progress-fill');
  const statusEl = document.getElementById('dxvk-install-status');

  if (overlay) overlay.style.display = '';
  if (nameEl) nameEl.textContent = `DXVK ${version}`;
  if (fillEl) fillEl.style.width = '0%';
  if (statusEl) statusEl.textContent = 'Starting download...';

  if (unlistenDxvkProgress) { unlistenDxvkProgress(); unlistenDxvkProgress = null; }

  try {
    unlistenDxvkProgress = await listen('dxvk-progress', (event) => {
      const p = event.payload;
      const fill = document.getElementById('dxvk-progress-fill');
      const status = document.getElementById('dxvk-install-status');
      if (!fill || !status) return;

      fill.style.width = `${p.percent.toFixed(1)}%`;
      status.textContent = p.message;

      if (p.phase === 'extracting') {
        fill.classList.add('extracting');
      } else {
        fill.classList.remove('extracting');
      }
    });
  } catch { /* listen failed */ }

  try {
    await invoke('install_dxvk', { downloadUrl, version, basePath: config.install_path });
  } catch (err) {
    if (statusEl) statusEl.textContent = `Error: ${err}`;
  }

  if (unlistenDxvkProgress) { unlistenDxvkProgress(); unlistenDxvkProgress = null; }
  isInstallingDxvk = false;

  await delay(1200);

  try {
    dxvkStatus = await invoke('detect_dxvk_version', { basePath: config.install_path });
  } catch { /* ignore */ }

  patchSection('dxvk-status-slot', renderDxvkStatusContent());
  patchSection('dxvk-releases-slot', renderDxvkReleasesContent());
  bindDxvkEvents(container);
  if (overlay) overlay.style.display = 'none';
}

async function launchWinecfg() {
  if (!config || !config.selected_runner) return;
  try {
    await invoke('run_winecfg', {
      basePath: config.install_path,
      runnerName: config.selected_runner,
    });
  } catch (err) {
    console.error('Failed to launch winecfg:', err);
  }
}

async function setDpi(dpi, container) {
  if (!config || !config.selected_runner) return;
  try {
    await invoke('set_dpi', {
      basePath: config.install_path,
      runnerName: config.selected_runner,
      dpi,
    });
    currentDpi = dpi;
  } catch (err) {
    console.error('Failed to set DPI:', err);
  }
  patchSection('prefix-tools-slot', renderPrefixToolsContent());
  bindPrefixToolEvents(container);
}

async function installPowershell(container) {
  if (isRunningPrefixTool || !config || !config.selected_runner) return;
  isRunningPrefixTool = true;
  prefixToolLog = [];
  patchSection('prefix-tools-slot', renderPrefixToolsContent());
  bindPrefixToolEvents(container);

  if (unlistenPrefixLog) { unlistenPrefixLog(); unlistenPrefixLog = null; }

  try {
    unlistenPrefixLog = await listen('prefix-tool-log', (event) => {
      const line = event.payload;
      prefixToolLog.push(line);
      appendPrefixLogLine(line);
    });
  } catch { /* listen failed */ }

  try {
    await invoke('install_powershell', {
      basePath: config.install_path,
      runnerName: config.selected_runner,
    });
    prefixToolLog.push('Done.');

    // Refresh PowerShell status after installation
    try {
      powershellInstalled = await invoke('detect_powershell', { basePath: config.install_path });
    } catch { /* ignore */ }
  } catch (err) {
    prefixToolLog.push(`ERROR: ${err}`);
  }

  if (unlistenPrefixLog) { unlistenPrefixLog(); unlistenPrefixLog = null; }
  isRunningPrefixTool = false;
  patchSection('prefix-tools-slot', renderPrefixToolsContent());
  bindPrefixToolEvents(container);
}

function appendPrefixLogLine(text) {
  const logEl = document.getElementById('prefix-tool-log');
  if (!logEl) return;
  const code = logEl.querySelector('code');
  if (code) {
    code.textContent += (code.textContent ? '\n' : '') + text;
  }
  scrollPrefixLog();
}

function scrollPrefixLog() {
  const logEl = document.getElementById('prefix-tool-log');
  if (logEl) logEl.scrollTop = logEl.scrollHeight;
}

// --- Helpers ---

function formatSize(bytes) {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const val = bytes / Math.pow(1024, i);
  return `${val.toFixed(i > 0 ? 1 : 0)} ${units[i]}`;
}

function formatCacheTime(timestamp) {
  if (!timestamp) return 'Never';
  const date = new Date(timestamp * 1000);
  return date.toLocaleString('de-DE', { dateStyle: 'short', timeStyle: 'short' });
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

function delay(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}
