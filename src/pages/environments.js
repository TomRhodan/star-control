/**
 * Star Control - Profiles Page
 *
 * This module manages Star Citizen configuration:
 * - USER.cfg editing (resolution, graphics settings)
 * - Controller/action map management
 * - Backup and restore of profiles
 * - Device (joystick) reordering
 * - Localization (language pack) management
 *
 * @module pages/profiles
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { confirm } from '../utils/dialogs.js';
import { escapeHtml } from '../utils.js';

// ==================== Debug Logging ====================

/**
 * Centralized debug logging function.
 * Logs to both console and to file via Rust backend.
 * @param {string} category - Log category (e.g., 'BINDING', 'CAPTURE', 'UI')
 * @param {string} level - Log level: 'debug', 'info', 'warn', 'error'
 * @param {string} message - Log message
 */
function debugLog(category, level, message) {
    // Always log to console
    const prefix = `[${category}]`;
    switch (level) {
        case 'error':
            console.error(prefix, message);
            break;
        case 'warn':
            console.warn(prefix, message);
            break;
        case 'info':
            console.info(prefix, message);
            break;
        default:
            console.log(prefix, message);
    }

    // Also send to Rust logging system (file)
    invoke('app_log', { level, category, message }).catch(e => {
        console.error('Failed to send log to backend:', e);
    });
}

// ==================== State ====================

/** @type {Object|null} Application configuration */
let config = null;
let userCfgSettings = {};
let activeScVersion = null;
let scVersions = [];

// Data.p4k copy state
let copyingVersion = null; // { version: string, startTime: number }

// New state
let parsedActionMaps = null;
let actionDefinitions = null;
let completeBindingList = [];
let exportedLayouts = [];
let selectedBindingSource = null; // null = active profile, string = layout filename
let backups = [];
let bindingFilter = '';
let bindingCategory = 'all';
let collapsedCategories = new Set(['performance', 'quality', 'shaders', 'textures', 'effects', 'clarity', 'lod']);
window.expandedBindingCategories = new Set(); // Track expanded binding categories - must be global for inline onclick
let draggedJoystickInstance = null;
let activeProfileTab = 'profile'; // 'profile' | 'usercfg' | 'localization'
let lastRestoredBackupId = null;
const lastRestoredPerVersion = {};
let activeProfileStatus = null; // { matched, files } from check_profile_status
let showChangesPanel = false;
let savedUserCfgSnapshot = {}; // snapshot of settings at load/apply time
let localizationStatus = null;
let localizationLabels = {}; // HashMap: technical ID -> Translated Label
let availableLanguages = [];
let localizationLoaded = false;
let localizationLoading = false;

// Binding editor state
let bindingEditorAction = null; // The action being edited
let bindingEditorDevice = 'keyboard'; // 'keyboard' | 'mouse' | 'gamepad' | 'joystick'
let customizedOnly = false; // Filter to show only customized bindings

let useHumanReadable = true; // Toggle between human-readable and raw mode
let renderGeneration = 0; // Monotonic counter to discard stale renders
let migrationChecked = false;

// Track which collapsible panels are open (persists across re-renders within session)
if (!window.expandedPanels) window.expandedPanels = { bindings: false, devices: false };

// ==================== Contextual Hints ====================

function getDismissedHints() {
  try {
    return JSON.parse(localStorage.getItem('starcontrol-dismissed-hints') || '[]');
  } catch { return []; }
}

function dismissHint(id) {
  const dismissed = getDismissedHints();
  if (!dismissed.includes(id)) {
    dismissed.push(id);
    localStorage.setItem('starcontrol-dismissed-hints', JSON.stringify(dismissed));
  }
  const el = document.querySelector(`.hint-banner[data-hint-id="${id}"]`);
  if (el) el.remove();
}

function renderHint(id, html) {
  if (getDismissedHints().includes(id)) return '';
  return `
    <div class="hint-banner" data-hint-id="${id}">
      <svg class="hint-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10"></circle>
        <line x1="12" y1="16" x2="12" y2="12"></line>
        <line x1="12" y1="8" x2="12.01" y2="8"></line>
      </svg>
      <span class="hint-text">${html}</span>
      <button class="hint-dismiss" data-action="dismiss-hint" data-hint-id="${id}">Got it</button>
    </div>
  `;
}

// Default USER.cfg settings
const DEFAULT_SETTINGS = {
  // Essential (visible by default)
  r_width: { value: 1920, label: 'Resolution Width', min: 640, max: 7680, step: 1, category: 'essential', type: 'number', desc: 'Horizontal screen resolution' },
  r_height: { value: 1080, label: 'Resolution Height', min: 480, max: 4320, step: 1, category: 'essential', type: 'number', desc: 'Vertical screen resolution' },
  r_Fullscreen: { value: 2, label: 'Window Mode', min: 0, max: 2, step: 1, category: 'essential', desc: '0=Windowed, 1=Fullscreen, 2=Borderless', labels: ['Windowed', 'Fullscreen', 'Borderless'] },
  'r.graphicsRenderer': { value: 0, label: 'Graphics Renderer', min: 0, max: 1, step: 1, category: 'essential', desc: '0=Vulkan, 1=DX11', labels: ['Vulkan', 'DX11'] },
  r_VSync: { value: 0, label: 'VSync', min: 0, max: 1, type: 'toggle', category: 'essential', desc: 'Enable vertical sync' },
  sys_MaxFPS: { value: 0, label: 'Max FPS', min: 0, max: 300, step: 5, category: 'essential', desc: 'Maximum FPS (0 = unlimited)' },
  sys_MaxIdleFPS: { value: 30, label: 'Max Idle FPS', min: 5, max: 120, step: 5, category: 'essential', desc: 'Max FPS when window not focused' },
  'r.TSR': { value: 0, label: 'TSR (Upscaling)', min: 0, max: 1, type: 'toggle', category: 'essential', desc: 'Temporal Super Resolution' },
  'pl_pit.forceSoftwareCursor': { value: 0, label: 'Software Cursor', min: 0, max: 1, type: 'toggle', category: 'essential', desc: 'Force software cursor (helps with multi-monitor)' },
  // Performance
  sys_budget_sysmem: { value: 16384, label: 'System RAM (MB)', min: 4096, max: 65536, step: 4096, category: 'performance', desc: 'Amount of system RAM available to Star Citizen' },
  sys_budget_videomem: { value: 8192, label: 'Video RAM (MB)', min: 2048, max: 24576, step: 2048, category: 'performance', desc: 'Amount of video RAM available' },
  sys_streaming_CPU: { value: 1, label: 'Streaming CPU', min: 0, max: 1, type: 'toggle', category: 'performance', desc: 'Enable CPU-based texture streaming' },
  sys_limit_phys_thread_count: { value: 0, label: 'Physics Thread Limit', min: 0, max: 16, step: 1, category: 'performance', desc: 'Limit physics threads (0 = auto)' },
  sys_PakStreamCache: { value: 1, label: 'Pak Stream Cache', min: 0, max: 1, type: 'toggle', category: 'performance', desc: 'Enable pak file caching' },
  ca_thread: { value: 1, label: 'Audio Thread', min: 0, max: 1, type: 'toggle', category: 'performance', desc: 'Enable dedicated audio thread' },
  e_ParticlesThread: { value: 1, label: 'Particles Thread', min: 0, max: 1, type: 'toggle', category: 'performance', desc: 'Enable particle threading' },
  sys_job_system_enable: { value: 1, label: 'Job System', min: 0, max: 1, type: 'toggle', category: 'performance', desc: 'Enable job system' },
  sys_spec_Quality: { value: 3, label: 'Overall Quality', min: 1, max: 4, step: 1, category: 'quality', desc: 'Overall graphics quality preset (1=Low, 4=Very High)' },
  sys_spec_GameEffects: { value: 3, label: 'Game Effects', min: 1, max: 4, step: 1, category: 'quality', desc: 'Quality of game effects' },
  sys_spec_Light: { value: 3, label: 'Lighting', min: 1, max: 4, step: 1, category: 'quality', desc: 'Lighting quality' },
  sys_spec_ObjectDetail: { value: 3, label: 'Object Detail', min: 1, max: 4, step: 1, category: 'quality', desc: 'Object detail level' },
  sys_spec_Particles: { value: 3, label: 'Particles', min: 1, max: 4, step: 1, category: 'quality', desc: 'Particle effects quality' },
  sys_spec_Physics: { value: 3, label: 'Physics', min: 1, max: 4, step: 1, category: 'quality', desc: 'Physics simulation quality' },
  sys_spec_PostProcessing: { value: 3, label: 'Post Processing', min: 1, max: 4, step: 1, category: 'quality', desc: 'Post-processing effects' },
  sys_spec_Shading: { value: 3, label: 'Shading', min: 1, max: 4, step: 1, category: 'quality', desc: 'Shading quality' },
  sys_spec_Shadows: { value: 3, label: 'Shadows', min: 1, max: 4, step: 1, category: 'quality', desc: 'Shadow quality' },
  sys_spec_Sound: { value: 3, label: 'Sound', min: 1, max: 4, step: 1, category: 'quality', desc: 'Sound quality' },
  sys_spec_Texture: { value: 3, label: 'Textures', min: 1, max: 4, step: 1, category: 'quality', desc: 'Texture quality' },
  sys_spec_TextureResolution: { value: 3, label: 'Texture Resolution', min: 1, max: 4, step: 1, category: 'quality', desc: 'Texture resolution scale' },
  sys_spec_VolumetricEffects: { value: 3, label: 'Volumetric Effects', min: 1, max: 4, step: 1, category: 'quality', desc: 'Volumetric effects quality' },
  sys_spec_Water: { value: 3, label: 'Water', min: 1, max: 4, step: 1, category: 'quality', desc: 'Water rendering quality' },
  q_Quality: { value: 3, label: 'Shader Quality', min: 0, max: 3, step: 1, category: 'shaders', desc: 'Overall shader quality (0-3)' },
  q_Renderer: { value: 3, label: 'Renderer', min: 0, max: 3, step: 1, category: 'shaders', desc: 'Renderer shader quality' },
  q_ShaderFX: { value: 3, label: 'FX Shaders', min: 0, max: 3, step: 1, category: 'shaders', desc: 'Special effects shaders' },
  q_ShaderGeneral: { value: 3, label: 'General', min: 0, max: 3, step: 1, category: 'shaders', desc: 'General shaders' },
  q_ShaderPostProcess: { value: 3, label: 'Post Process', min: 0, max: 3, step: 1, category: 'shaders', desc: 'Post-processing shaders' },
  q_ShaderShadow: { value: 3, label: 'Shadow', min: 0, max: 3, step: 1, category: 'shaders', desc: 'Shadow shaders' },
  r_TexMaxAnisotropy: { value: 16, label: 'Anisotropy', min: 0, max: 16, step: 1, category: 'textures', desc: 'Maximum anisotropy level (0, 2, 4, 8, 16)' },
  r_TexturesStreamingResidencyEnabled: { value: 1, label: 'Texture Streaming', min: 0, max: 1, type: 'toggle', category: 'textures', desc: 'Enable texture streaming' },
  r_TexturesStreamPoolSize: { value: 8192, label: 'Stream Pool Size (MB)', min: 2048, max: 16384, step: 1024, category: 'textures', desc: 'Texture stream pool size' },
  r_SSAOQuality: { value: 2, label: 'SSAO Quality', min: 0, max: 4, step: 1, category: 'effects', desc: 'Screen Space Ambient Occlusion quality (0=off)' },
  r_ssdoHalfRes: { value: 1, label: 'SSDO Half Res', min: 0, max: 1, type: 'toggle', category: 'effects', desc: 'Run SSDO at half resolution' },
  r_FogShadows: { value: 0, label: 'Fog Shadows', min: 0, max: 1, type: 'toggle', category: 'effects', desc: 'Enable fog shadows' },
  e_Tessellation: { value: 0, label: 'Tessellation', min: 0, max: 1, type: 'toggle', category: 'effects', desc: 'Enable tessellation' },
  e_ParticlesShadows: { value: 0, label: 'Particle Shadows', min: 0, max: 1, type: 'toggle', category: 'effects', desc: 'Enable particle shadows' },
  r_HDRBloomRatio: { value: 0, label: 'HDR Bloom', min: 0, max: 1, type: 'toggle', category: 'clarity', desc: 'HDR Bloom effect (0=off)' },
  r_DepthOfField: { value: 0, label: 'Depth of Field', min: 0, max: 1, type: 'toggle', category: 'clarity', desc: 'Enable depth of field' },
  r_MotionBlur: { value: 0, label: 'Motion Blur', min: 0, max: 1, type: 'toggle', category: 'clarity', desc: 'Enable motion blur' },
  r_Sharpening: { value: 1, label: 'Sharpening', min: 0, max: 1, type: 'toggle', category: 'clarity', desc: 'Enable sharpening' },
  r_Flares: { value: 0, label: 'Lens Flares', min: 0, max: 1, type: 'toggle', category: 'clarity', desc: 'Enable lens flares' },
  r_ColorGrading: { value: 0, label: 'Color Grading', min: 0, max: 1, type: 'toggle', category: 'clarity', desc: 'Enable color grading' },
  e_ViewDistRatio: { value: 100, label: 'View Distance', min: 0, max: 255, step: 5, category: 'lod', desc: 'General view distance (0-255)' },
  e_ViewDistRatioDetail: { value: 100, label: 'Detail Distance', min: 0, max: 255, step: 5, category: 'lod', desc: 'Detail view distance' },
  e_VegetationMinSize: { value: 0.5, label: 'Vegetation Min Size', min: 0, max: 2, step: 0.1, category: 'lod', desc: 'Minimum vegetation size' },
};

const QUALITY_LEVELS = ['', 'Low', 'Medium', 'High', 'Very High'];
const SHADER_LEVELS = ['', 'Low', 'Medium', 'High'];

// ==================== Entry Point ====================

/**
 * Set the active profile tab and render
 * @param {string} tab - The tab to show: 'profile', 'usercfg', 'localization'
 */
export function setActiveProfileTab(tab) {
  activeProfileTab = tab;
}

export async function renderEnvironments(container) {
  // Increment generation to discard stale renders from overlapping calls
  const thisGeneration = ++renderGeneration;

  // One-time migration: rename old binding_database.json to .bak
  if (!migrationChecked) {
    migrationChecked = true;
    try {
      const migrated = await invoke('migrate_binding_database');
      if (migrated) {
        showNotification('Old binding database migrated. Bindings are now managed per profile.', 'info');
      }
    } catch (e) {
      console.warn('[profiles] binding_database migration failed:', e);
    }
  }

  try {
    config = await invoke('load_config');
  } catch (e) {
    config = { install_path: '', log_level: 'info' };
  }

  if (config?.install_path) {
    try {
      scVersions = await invoke('detect_sc_versions', { gp: config.install_path });
    } catch (e) {
      console.error('[profiles] detect_sc_versions error:', e);
      scVersions = [];
    }
  }

  if (scVersions.length > 0 && !activeScVersion) {
    activeScVersion = scVersions[0].version;
  }

  // Load localization labels in background if possible
  if (config?.install_path && activeScVersion && !localizationLoaded && !localizationLoading) {
    loadLocalizationLabels().then((loaded) => {
      if (loaded) {
        // Re-render if on profile tab with bindings visible
        const content = document.getElementById('content');
        if (content && activeProfileTab === 'profile' && lastRestoredBackupId) {
          loadCompleteBindingList().then(() => renderEnvironments(content));
        }
      }
    }).catch(e => console.error('Failed to load localization labels:', e));
  }

  // Show loading skeleton while data loads
  container.innerHTML = `
    <div class="profiles-loading-skeleton">
      <div class="dash-skeleton">
        <div class="dash-skeleton-line medium"></div>
        <div class="dash-skeleton-line short"></div>
        <div class="dash-skeleton-block" style="height: 120px;"></div>
        <div class="dash-skeleton-block" style="height: 200px;"></div>
      </div>
    </div>
  `;

  // Load active profile state from disk
  try {
    const saved = await invoke('load_active_profiles');
    Object.assign(lastRestoredPerVersion, saved);
    if (activeScVersion && saved[activeScVersion]) {
      lastRestoredBackupId = saved[activeScVersion];
    }
  } catch (e) { /* ignore */ }

  // Load all data in parallel
  await Promise.all([
    loadActionDefinitions(),
    loadDevicesAndBindings(),
    loadCompleteBindingList(),
    loadExportedLayouts(),
    loadBackups(),
    loadUserCfgSettings(),
    loadLocalizationData(),
  ]);
  await loadProfileStatus();

  // Discard this render if a newer renderEnvironments call has started
  if (thisGeneration !== renderGeneration) return;

  // Render
  let html = `
    <div class="page-header">
      <h1>Environments</h1>
      <p class="page-subtitle">Manage Star Citizen environments, game data, and settings</p>
    </div>
    <div class="sc-settings">
      ${renderVersionSelector()}
      ${renderMainContent()}
    </div>
  `;

  container.innerHTML = html;

  attachProfilesEventListeners();
}

// ==================== Data Loading ====================

async function loadActionDefinitions() {
  try {
    actionDefinitions = await invoke('get_action_definitions');
  } catch (e) {
    console.error('Failed to load action definitions:', e);
    actionDefinitions = null;
  }
}

let bindingStats = { total: 0, custom: 0 };

async function loadCompleteBindingList() {
  completeBindingList = [];
  bindingStats = { total: 0, custom: 0 };

  if (!config?.install_path || !activeScVersion || !lastRestoredBackupId) return;

  try {
    const result = await invoke('get_profile_bindings', {
      gp: config.install_path,
      v: activeScVersion,
      profileId: lastRestoredBackupId,
    });
    completeBindingList = result.bindings || [];
    bindingStats = result.stats || { total: 0, custom: 0 };
  } catch (e) {
    debugLog('BINDING', 'error', 'Failed to load profile bindings: ' + e);
    completeBindingList = [];
    bindingStats = { total: 0, custom: 0 };
  }
}


async function loadDevicesAndBindings() {
  if (!config?.install_path || !activeScVersion) {
    parsedActionMaps = null;
    return;
  }
  try {
    parsedActionMaps = await invoke('parse_actionmaps', {
      gp: config.install_path,
      v: activeScVersion,
      source: selectedBindingSource,
    });
  } catch (e) {
    parsedActionMaps = null;
  }
}

async function loadExportedLayouts() {
  if (!config?.install_path || !activeScVersion) {
    exportedLayouts = [];
    return;
  }
  try {
    exportedLayouts = await invoke('list_exported_layouts', {
      gamePath: config.install_path,
      version: activeScVersion,
    });
  } catch (e) {
    exportedLayouts = [];
  }
}


async function loadBackups() {
  if (!activeScVersion) {
    backups = [];
    return;
  }
  try {
    backups = await invoke('list_backups', { v: activeScVersion });
  } catch (e) {
    backups = [];
  }
}

async function loadProfileStatus() {
  if (!config?.install_path || !activeScVersion || !lastRestoredBackupId) {
    activeProfileStatus = null;
    return;
  }
  try {
    activeProfileStatus = await invoke('check_profile_status', {
      gp: config.install_path, v: activeScVersion, bid: lastRestoredBackupId,
    });
  } catch (e) {
    activeProfileStatus = null;
  }
}

async function loadUserCfgSettings() {
  if (!config?.install_path || !activeScVersion) {
    userCfgSettings = {};
    return;
  }
  try {
    const content = await invoke('read_user_cfg', { gp: config.install_path, v: activeScVersion });
    userCfgSettings = parseUserCfg(content);
  } catch (e) {
    userCfgSettings = {};
  }
  savedUserCfgSnapshot = { ...userCfgSettings };
}

async function loadLocalizationLabels() {
  if (!config?.install_path || !activeScVersion) {
    localizationLabels = {};
    localizationLoaded = false;
    return false; // Return whether we actually loaded anything
  }
  
  if (localizationLoading || localizationLoaded) return false;
  localizationLoading = true;
  
  try {
    // This uses the cached labels if available
    localizationLabels = await invoke('get_localization_labels', {
      gamePath: config.install_path,
      version: activeScVersion,
      language: localizationStatus?.language_name?.toLowerCase() || 'english'
    });
    console.log(`[Localization] Loaded ${Object.keys(localizationLabels).length} labels`);
    localizationLoaded = true;
    return true; // Successfully loaded
  } catch (e) {
    if (e !== "Localization loading already in progress") {
      console.error('Failed to load localization labels:', e);
    }
    return false;
  } finally {
    localizationLoading = false;
  }
}

async function loadLocalizationData() {
  if (!config?.install_path || !activeScVersion) {
    localizationStatus = null;
    availableLanguages = [];
    return;
  }
  try {
    const [status, languages] = await Promise.all([
      invoke('get_localization_status', { gamePath: config.install_path, version: activeScVersion }),
      invoke('get_available_languages'),
    ]);
    localizationStatus = status;
    availableLanguages = languages;
  } catch (e) {
    localizationStatus = null;
    availableLanguages = [];
  }
}

function parseUserCfg(content) {
  const settings = {};
  const lines = content.split('\n');
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed && !trimmed.startsWith(';') && !trimmed.startsWith('#')) {
      const match = trimmed.match(/^([\w.]+)\s*=\s*(.+)$/);
      if (match) {
        const key = match[1];
        let value = match[2].trim();
        // Strip inline comments
        const commentIdx = value.indexOf(';');
        if (commentIdx > 0) value = value.substring(0, commentIdx).trim();
        if (!isNaN(value) && value !== '') {
          value = parseFloat(value);
        }
        settings[key] = value;
      }
    }
  }
  return settings;
}

// ==================== Version Selector ====================
const STANDARD_VERSIONS = ['LIVE', 'PTU', 'EPTU', 'TECH-PREVIEW', 'HOTFIX'];

// ==================== Version Selector ====================

function renderVersionSelector() {
  if (scVersions.length === 0 && (!config?.install_path)) {
    return `
      <div class="sc-version-notice">
        <div class="notice-icon">
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path>
          </svg>
        </div>
        <h3>No SC Versions Found</h3>
        <p>No Star Citizen installations detected. Please set your base directory in Settings.</p>
        <p class="notice-path">${escapeHtml(config?.install_path || 'Not set')}</p>
      </div>
    `;
  }

  // Combine detected versions and standard versions
  const allVersionNames = [...scVersions.map(v => v.version)];
  for (const sv of STANDARD_VERSIONS) {
    if (!allVersionNames.includes(sv)) {
      allVersionNames.push(sv);
    }
  }

  // Sort versions: LIVE first, then PTU, then others from STANDARD, then rest
  allVersionNames.sort((a, b) => {
    const order = { 'LIVE': 0, 'PTU': 1, 'EPTU': 2, 'TECH-PREVIEW': 3, 'HOTFIX': 4 };
    const aOrder = order[a] !== undefined ? order[a] : 99;
    const bOrder = order[b] !== undefined ? order[b] : 99;
    return aOrder - bOrder || a.localeCompare(b);
  });

  return `
    <div class="sc-version-selector">
      <label class="section-label">Version</label>
      <div class="version-cards">
        ${allVersionNames.map(vName => {
          const v = scVersions.find(v => v.version === vName);
          const exists = !!v;
          const isActive = activeScVersion === vName;
          const hasDataP4k = exists && v.has_data_p4k !== false;
          const isCopying = copyingVersion && copyingVersion.version === vName;
          
          let statusClass = 'missing';
          if (exists) {
            statusClass = isCopying ? 'copying' : (hasDataP4k ? 'installed' : 'missing');
          } else {
            statusClass = 'not-installed';
          }
          
          return `
            <button class="sc-version-card ${isActive ? 'active' : ''} ${!exists ? 'sc-not-installed' : ''} ${exists && !hasDataP4k ? 'missing-data' : ''}"
                    data-version="${escapeHtml(vName)}">
              <div class="version-status-dot ${statusClass}"
                   title="${!exists ? 'Folder not created - click to manage' : (isCopying ? 'Copying Data.p4k...' : (hasDataP4k ? 'Ready' : 'Data.p4k missing'))}"></div>
              <span class="version-label">${escapeHtml(vName)}</span>
              ${exists && !hasDataP4k && !isCopying ? `<div class="version-copy-btn" data-version="${escapeHtml(vName)}" title="Copy Data.p4k from another version">⤵</div>` : ''}
              ${isCopying ? `<div class="version-copy-progress" data-version="${escapeHtml(vName)}">0%</div>` : ''}
            </button>
          `;
        }).join('')}
      </div>
    </div>
  `;
}

// ==================== Main Content ====================

function renderMainContent() {
  if (scVersions.length === 0 || !activeScVersion) {
    return '';
  }

  const vInfo = scVersions.find(v => v.version === activeScVersion);
  if (!vInfo) {
    return renderEmptyVersionState();
  }

  const tabs = [
    { key: 'profile', label: 'Profiles', tooltip: 'Save and load Star Citizen profile snapshots' },
    { key: 'usercfg', label: 'USER.cfg', tooltip: 'Configure graphics, performance, and quality settings' },
    { key: 'localization', label: 'Localization', tooltip: 'Install community translations' },
    { key: 'storage', label: 'Storage', tooltip: 'Manage Game Data and Versions' },
  ];

  let tabContent = '';
  if (activeProfileTab === 'profile') {
    tabContent = renderProfileTab();
  } else if (activeProfileTab === 'localization') {
    tabContent = renderLocalizationTab();
  } else if (activeProfileTab === 'usercfg') {
    tabContent = renderUserCfgUI();
  } else if (activeProfileTab === 'storage') {
    tabContent = renderStorageTab();
  }

  return `
    <div class="profile-tabs">
      ${tabs.map(t => `<button class="profile-tab ${activeProfileTab === t.key ? 'active' : ''}" data-tab="${t.key}" data-tooltip="${t.tooltip}" data-tooltip-pos="bottom">${t.label}</button>`).join('')}
    </div>
    <div class="profile-tab-content">
      ${tabContent}
    </div>
  `;
}

function renderEmptyVersionState() {
  const versionsWithP4k = scVersions.filter(v => v.has_data_p4k !== false).map(v => v.version);
  
  return `
    <div class="sc-version-notice" style="margin-top: 2rem;">
      <div class="notice-icon">
        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path>
          <line x1="12" y1="11" x2="12" y2="17"></line>
          <line x1="9" y1="14" x2="15" y2="14"></line>
        </svg>
      </div>
      <h3>${escapeHtml(activeScVersion)} Environment not found</h3>
      <p>This version folder does not exist yet in your Star Citizen directory.</p>
      
      <div class="empty-state-actions" style="display: flex; flex-direction: column; gap: 1rem; max-width: 400px; margin: 2rem auto;">
        <button class="btn btn-primary" id="btn-create-version" data-version="${escapeHtml(activeScVersion)}">
          Create Empty Folder
        </button>
        
        ${versionsWithP4k.length > 0 ? `
          <div style="border-top: 1px solid var(--border); padding-top: 1rem; margin-top: 0.5rem;">
            <p style="font-size: 0.8rem; color: var(--text-muted); margin-bottom: 0.5rem;">Initialize with Data.p4k from another version:</p>
            <div style="display: flex; gap: 0.5rem;">
              <select id="data-source-select" class="btn btn-sm" style="flex: 1; background: var(--bg-secondary);">
                ${versionsWithP4k.map(v => `<option value="${escapeHtml(v)}">${escapeHtml(v)}</option>`).join('')}
              </select>
              <button class="btn btn-sm" id="btn-link-p4k" data-version="${escapeHtml(activeScVersion)}" title="Space-saving symlink (recommended for Linux)">Symlink</button>
              <button class="btn btn-sm" id="btn-copy-p4k" data-version="${escapeHtml(activeScVersion)}" title="Full independent copy (uses 100GB+ extra)">Copy</button>
            </div>
          </div>
        ` : ''}
      </div>
    </div>
  `;
}

function renderStorageTab() {
  const vInfo = scVersions.find(v => v.version === activeScVersion);
  if (!vInfo) return '';

  const hasDataP4k = vInfo.has_data_p4k !== false;
  
  return `
    <div class="sc-section">
      <div class="sc-section-header">
        <h3>Version Storage Management</h3>
      </div>
      
      <div class="profile-info-card">
        <div class="profile-info-row">
          <span class="profile-info-label">Environment</span>
          <span class="profile-info-value"><strong>${escapeHtml(activeScVersion)}</strong></span>
        </div>
        <div class="profile-info-row">
          <span class="profile-info-label">Path</span>
          <span class="profile-info-value"><code>${escapeHtml(vInfo.path || 'Unknown')}</code></span>
        </div>
        <div class="profile-info-row">
          <span class="profile-info-label">Data.p4k</span>
          <span class="profile-info-value">
            ${hasDataP4k 
              ? '<span class="localization-installed-badge">Installed</span>' 
              : '<span class="text-muted">Missing</span>'}
          </span>
        </div>
      </div>
      
      <div class="storage-actions" style="margin-top: 2rem; display: flex; flex-direction: column; gap: 1rem;">
        <div style="padding: 1rem; border: 1px solid var(--border-color); border-radius: 8px; background: rgba(255, 50, 50, 0.05);">
          <h4 style="margin-top: 0; color: #ff6b6b; display: flex; align-items: center; gap: 0.5rem;">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>
            Danger Zone
          </h4>
          <p style="color: var(--text-secondary); margin-bottom: 1rem; font-size: 0.9rem;">
            Delete this entire environment folder from your hard drive to free up space. This action cannot be undone. 
            All profiles, settings, and the Data.p4k file for <strong>${escapeHtml(activeScVersion)}</strong> will be permanently removed.
          </p>
          <button class="btn btn-danger" id="btn-delete-version" data-version="${escapeHtml(activeScVersion)}">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="margin-right: 6px;"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
            Delete ${escapeHtml(activeScVersion)} Environment
          </button>
        </div>
      </div>
    </div>
  `;
}

function renderProfileTab() {
  const vInfo = scVersions.find(v => v.version === activeScVersion);
  const files = [];
  if (vInfo?.has_actionmaps) files.push('actionmaps.xml');
  if (vInfo?.has_attributes) files.push('attributes.xml');
  if (vInfo?.has_usercfg) files.push('USER.cfg');

  const activeBackup = lastRestoredBackupId ? backups.find(b => b.id === lastRestoredBackupId) : null;
  const hasScFiles = files.length > 0;
  const hasProfiles = backups.length > 0;
  const isDirty = activeBackup?.dirty === true;

  // Import banner for versions with no SC files
  const importBanner = !vInfo?.has_actionmaps && scVersions.length > 1 ? `
    <div class="import-banner" id="import-banner">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10"></circle>
        <line x1="12" y1="16" x2="12" y2="12"></line>
        <line x1="12" y1="8" x2="12.01" y2="8"></line>
      </svg>
      <span>No profiles found for <strong>${escapeHtml(activeScVersion)}</strong>. Import from another version?</span>
      <button class="btn btn-sm btn-primary" id="btn-import-banner">Import from Version</button>
      <button class="btn-icon" id="btn-import-banner-dismiss" title="Dismiss">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
      </button>
    </div>
  ` : '';

  // === Profiles Section (always at top) ===
  let profilesSection = '';
  if (hasScFiles && !hasProfiles) {
    // Empty state: first-time user
    profilesSection = `
      <div class="sc-section profiles-section">
        <div class="profile-empty-state">
          <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"></path>
            <polyline points="17 21 17 13 7 13 7 21"></polyline>
            <polyline points="7 3 7 8 15 8"></polyline>
          </svg>
          <div class="profile-empty-text">
            <p><strong>Save your Star Citizen settings as a profile</strong></p>
            <p class="text-muted">This backs up your keybindings, graphics, and controller settings. You can edit bindings, switch between setups, and restore anytime.</p>
          </div>
          <div class="profile-empty-actions">
            <button class="btn btn-primary" id="btn-save-first-profile">Save Current Settings</button>
            ${scVersions.length > 1 ? '<button class="btn btn-sm" id="btn-import-version">Import from Version</button>' : ''}
          </div>
        </div>
      </div>
    `;
  } else if (hasProfiles) {
    // Active profile header + profile cards
    let activeHeader = '';
    if (activeBackup) {
      const displayLabel = escapeHtml(activeBackup.label || activeBackup.created_at);
      let statusText = '';
      let statusClass = '';
      const isOutOfSync = activeProfileStatus && activeProfileStatus.files.length > 0 && !activeProfileStatus.matched;
      const showApplyButton = isDirty || isOutOfSync;

      if (isDirty) {
        statusText = 'Unsaved changes — click "Apply to SC" to push to game';
        statusClass = 'profile-status-changed';
      } else if (activeProfileStatus && activeProfileStatus.files.length > 0) {
        if (activeProfileStatus.matched) {
          statusText = 'In sync with Star Citizen';
          statusClass = 'profile-status-ok';
        } else {
          const changedCount = activeProfileStatus.files.filter(f => f.status !== 'unchanged').length;
          statusText = `${changedCount} file${changedCount !== 1 ? 's' : ''} changed since last apply`;
          statusClass = 'profile-status-changed';
        }
      }

      activeHeader = `
        <div class="profile-active-header">
          <div class="profile-active-info">
            <span class="profile-active-label">
              <span class="profile-active-star">★</span>
              ${displayLabel}
            </span>
            ${statusText ? `<span class="${statusClass}" ${!isDirty && statusClass === 'profile-status-changed' ? 'id="btn-toggle-changes"' : ''}>${statusText}</span>` : ''}
          </div>
          <div class="profile-active-actions">
            ${isOutOfSync ? `
              <button class="btn btn-sm btn-ghost" id="btn-revert-changes" title="Discard game changes and reload profile files">Revert</button>
              <button class="btn btn-sm" id="btn-update-profile" title="Overwrite this profile with your current game settings">Update Profile</button>
            ` : ''}
            ${showApplyButton ? `<button class="btn btn-primary btn-sm" id="btn-apply-to-sc" title="Push profile files to Star Citizen">Apply to SC ${escapeHtml(activeScVersion)}</button>` : ''}
          </div>
        </div>
        ${showApplyButton ? renderHint('apply-explain', 'This will copy the profile\'s files to your Star Citizen directory, overwriting the current settings.') : ''}
        ${showChangesPanel && activeProfileStatus && !activeProfileStatus.matched ? renderChangesPanel(activeProfileStatus.files) : ''}
      `;
    } else if (hasScFiles) {
      activeHeader = `
        <div class="profile-active-header profile-active-none">
          <div class="profile-active-info">
            <span class="text-muted">No profile loaded — load one below or save your current settings</span>
          </div>
        </div>
      `;
    }

    profilesSection = `
      <div class="sc-section profiles-section">
        ${renderHint('profiles-intro', 'Profiles snapshot your Star Citizen settings (keybindings, graphics, controllers). Load a profile to edit it, then use <strong>Apply to SC</strong> to push changes to the game.')}
        ${activeHeader}
        <div class="profiles-card-grid">
          ${backups.map(b => {
            const isActive = lastRestoredBackupId === b.id;
            return `
              <div class="profile-card ${isActive ? 'active' : ''}" data-backup-id="${escapeHtml(b.id)}">
                <div class="profile-card-header">
                  <span class="profile-card-name">${escapeHtml(b.label || 'Unnamed profile')}</span>
                  <div class="profile-card-actions">
                    <button class="btn-icon btn-icon-rename" data-action="rename-saved-profile" data-backup-id="${escapeHtml(b.id)}" title="Rename">
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path></svg>
                    </button>
                    <button class="btn-icon btn-icon-danger" data-action="delete-saved-profile" data-backup-id="${escapeHtml(b.id)}" title="Delete">
                      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
                    </button>
                  </div>
                </div>
                <div class="profile-card-meta">
                  <span class="profile-card-date">${escapeHtml(b.created_at)}</span>
                  <span class="backup-type-badge ${b.backup_type}">${escapeHtml(formatProfileTypeBadge(b.backup_type))}</span>
                  ${b.device_map?.length > 0 ? `<span class="backup-devices">${b.device_map.length} device${b.device_map.length !== 1 ? 's' : ''}</span>` : ''}
                  ${b.dirty ? '<span class="backup-dirty-badge">unsaved</span>' : ''}
                </div>
                ${!isActive ? `<button class="btn btn-sm profile-card-load" data-action="load-profile" data-backup-id="${escapeHtml(b.id)}">Load</button>` : '<span class="profile-card-active-badge">Active</span>'}
              </div>
            `;
          }).join('')}
          <div class="profile-card profile-card-add" id="btn-save-current" title="Save current Star Citizen settings as a new profile">
            <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="12" y1="5" x2="12" y2="19"></line>
              <line x1="5" y1="12" x2="19" y2="12"></line>
            </svg>
            <span>Save Current</span>
          </div>
        </div>
        ${scVersions.length > 1 ? `<div class="profiles-section-footer"><button class="btn btn-sm" id="btn-import-version">Import from Version</button></div>` : ''}
      </div>
    `;
  }

  // === Collapsible Keybindings (only when a profile is loaded) ===
  const bindingsCollapsible = activeBackup ? renderBindingsCollapsible() : '';

  // === Collapsible Devices (only when a profile is loaded) ===
  const devicesCollapsible = activeBackup ? renderDeviceMapCollapsible() : '';

  return `
    <div class="sc-version-installed">
      ${importBanner}
      ${profilesSection}
      ${bindingsCollapsible}
      ${devicesCollapsible}
    </div>
  `;
}

function renderLocalizationTab() {
  return `
    <div class="localization-tab">
      ${renderLocalizationStatus()}
      ${renderLanguageSelector()}
    </div>
  `;
}

function renderLocalizationStatus() {
  const status = localizationStatus;

  if (!status || !status.installed) {
    return `
      <div class="profile-info-card">
        <div class="profile-info-row">
          <span class="profile-info-label">Language</span>
          <span class="profile-info-value">English (default)</span>
        </div>
        <div class="profile-info-row">
          <span class="profile-info-label">Status</span>
          <span class="profile-info-value"><span class="text-muted">No community translation installed</span></span>
        </div>
      </div>
    `;
  }

  const langName = status.language_name || status.language_code || 'Unknown';
  const sizeStr = status.file_size ? formatFileSize(status.file_size) : 'Unknown';

  return `
    <div class="profile-info-card">
      <div class="profile-info-row">
        <span class="profile-info-label">Language</span>
        <span class="profile-info-value localization-lang-active">${escapeHtml(langName)}</span>
      </div>
      ${status.language_code ? `
        <div class="profile-info-row">
          <span class="profile-info-label">Code</span>
          <span class="profile-info-value"><code>${escapeHtml(status.language_code)}</code></span>
        </div>
      ` : ''}
      ${status.source_label ? `
        <div class="profile-info-row">
          <span class="profile-info-label">Source</span>
          <span class="profile-info-value">${escapeHtml(status.source_label)}</span>
        </div>
      ` : ''}
      ${status.installed_at ? `
        <div class="profile-info-row">
          <span class="profile-info-label">Installed</span>
          <span class="profile-info-value">${escapeHtml(status.installed_at)}</span>
        </div>
      ` : ''}
      <div class="profile-info-row">
        <span class="profile-info-label">File Size</span>
        <span class="profile-info-value">${sizeStr}</span>
      </div>
      <div class="profile-info-row">
        <span class="profile-info-label">Actions</span>
        <span class="profile-info-value">
          <button class="btn btn-sm btn-primary" id="btn-update-localization" ${localizationLoading ? 'disabled' : ''}>
            ${localizationLoading ? 'Updating...' : 'Update'}
          </button>
          <button class="btn btn-sm btn-danger-sm" id="btn-remove-localization" ${localizationLoading ? 'disabled' : ''}>Remove</button>
        </span>
      </div>
    </div>
  `;
}

function renderLanguageSelector() {
  if (availableLanguages.length === 0) {
    return '<div class="sc-hint">No languages available.</div>';
  }

  // Group languages by code
  const grouped = {};
  for (const lang of availableLanguages) {
    if (!grouped[lang.language_code]) {
      grouped[lang.language_code] = {
        language_code: lang.language_code,
        language_name: lang.language_name,
        flag: lang.flag,
        sources: [],
      };
    }
    grouped[lang.language_code].sources.push({
      source_repo: lang.source_repo,
      source_label: lang.source_label,
    });
  }

  const languages = Object.values(grouped);
  const isInstalled = localizationStatus?.installed;
  const installedCode = localizationStatus?.language_code;
  const installedSource = localizationStatus?.source_label;

  return `
    <div class="sc-section">
      <div class="sc-section-header">
        <h3>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10"></circle>
            <line x1="2" y1="12" x2="22" y2="12"></line>
            <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path>
          </svg>
          Available Languages
        </h3>
      </div>
      <div class="localization-languages-grid">
        ${languages.map(lang => {
          const isActive = isInstalled && installedCode === lang.language_code;
          return `
            <div class="localization-lang-card ${isActive ? 'active' : ''}">
              <div class="localization-lang-info">
                <span class="localization-lang-flag">${escapeHtml(lang.flag)}</span>
                <span class="localization-lang-name">${escapeHtml(lang.language_name)}</span>
                <span class="localization-lang-code">${escapeHtml(lang.language_code)}</span>
              </div>
              <div class="localization-lang-sources">
                ${lang.sources.map(src => {
                  const isSrcActive = isActive && installedSource === src.source_label;
                  return `
                    <div class="localization-source-item">
                      ${isSrcActive ? '<span class="localization-installed-badge">Installed</span>' : `
                        <button class="btn-install" data-action="install-lang"
                                data-lang-code="${escapeHtml(lang.language_code)}"
                                data-source-repo="${escapeHtml(src.source_repo)}"
                                data-lang-name="${escapeHtml(lang.language_name)}"
                                data-source-label="${escapeHtml(src.source_label)}"
                                ${localizationLoading ? 'disabled' : ''}>
                          Install
                        </button>
                      `}
                      <span class="localization-source-label">${escapeHtml(src.source_label)}</span>
                    </div>
                  `;
                }).join('')}
              </div>
            </div>
          `;
        }).join('')}
      </div>
      <div class="localization-hint">
        Translations are maintained by community volunteers. Audio always remains in English.
      </div>
    </div>
  `;
}

function formatFileSize(bytes) {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

// ==================== Localization Actions ====================

async function installLocalization(langCode, sourceRepo, displayName, sourceLabel) {
  if (!config?.install_path || !activeScVersion) return;
  localizationLoading = true;
  renderEnvironments(document.getElementById('content'));

  try {
    await invoke('install_localization', {
      gamePath: config.install_path,
      version: activeScVersion,
      languageCode: langCode,
      sourceRepo: sourceRepo,
      languageName: displayName,
      sourceLabel: sourceLabel,
    });
    showNotification(`${displayName} translation installed`, 'success');
    await Promise.all([loadLocalizationData(), loadUserCfgSettings()]);
  } catch (e) {
    showNotification(`Installation failed: ${e}`, 'error');
  }

  localizationLoading = false;
  renderEnvironments(document.getElementById('content'));
}

async function removeLocalization() {
  if (!config?.install_path || !activeScVersion) return;

  const langName = localizationStatus?.language_name || 'translation';
  const confirmed = await confirm(`Remove ${langName} translation?`, { title: 'Remove Translation', kind: 'warning' });
  if (!confirmed) return;

  localizationLoading = true;
  renderEnvironments(document.getElementById('content'));

  try {
    await invoke('remove_localization', {
      gamePath: config.install_path,
      version: activeScVersion,
    });
    showNotification('Translation removed', 'success');
    await Promise.all([loadLocalizationData(), loadUserCfgSettings()]);
  } catch (e) {
    showNotification(`Remove failed: ${e}`, 'error');
  }

  localizationLoading = false;
  renderEnvironments(document.getElementById('content'));
}

function resolveSourceRepo() {
  if (!localizationStatus?.installed) return null;
  const code = localizationStatus.language_code;
  const label = localizationStatus.source_label;
  const match = availableLanguages.find(
    l => l.language_code === code && l.source_label === label
  );
  return match?.source_repo || null;
}

// ==================== Devices Section ====================

// ==================== Keybindings Section ====================

/** Lightweight in-place refresh of the bindings list (no full page re-render, preserves scroll) */
function refreshBindingsInPlace() {
  // Re-render category HTML inside .bindings-body
  const body = document.querySelector('.bindings-body');
  if (!body) return;

  // Temporarily clear filter so renderBindingCategory renders ALL items
  const savedFilter = bindingFilter;
  bindingFilter = '';

  const sourceList = customizedOnly
    ? completeBindingList.filter(b => b.is_custom)
    : completeBindingList;

  const categorized = {};
  if (Array.isArray(sourceList)) {
    for (const b of sourceList) {
      const catKey = b.category || 'unknown';
      const catLabel = b.category_label || catKey;
      if (!categorized[catKey]) {
        categorized[catKey] = { label: catLabel, bindings: [] };
      }
      categorized[catKey].bindings.push(b);
    }
  }

  const categoryKeys = Object.keys(categorized).sort((a, b) => {
    const labelA = categorized[a].label || '';
    const labelB = categorized[b].label || '';
    return labelA.toLowerCase().localeCompare(labelB.toLowerCase());
  });

  body.innerHTML = categoryKeys.length === 0
    ? `<div class="sc-hint">${customizedOnly ? 'No customized bindings.' : 'No keybindings found.'}</div>`
    : categoryKeys.map(catKey => renderBindingCategory(catKey, categorized[catKey].label, categorized[catKey].bindings)).join('');

  // Restore filter
  bindingFilter = savedFilter;

  // Update stats badge
  const badge = document.querySelector('.binding-stats-badge');
  if (badge) badge.textContent = `${bindingStats.total} total / ${bindingStats.custom} customized`;

  // Re-attach binding-specific listeners on the new DOM
  attachBindingEventListeners();

  // Re-apply filter if one was active (hides non-matching rows via display style)
  if (bindingFilter) {
    const searchInput = document.getElementById('binding-search');
    if (searchInput) {
      searchInput.dispatchEvent(new Event('input'));
    }
  }
}

/** Attach only binding-related event listeners (search, add/edit/remove buttons) */
function attachBindingEventListeners() {
  document.getElementById('binding-search')?.addEventListener('input', (e) => {
    const term = e.target.value.toLowerCase().trim();
    bindingFilter = term;

    const categoryBlocks = document.querySelectorAll('.binding-category-block');
    categoryBlocks.forEach(block => {
      const rows = block.querySelectorAll('.binding-row');
      let hasVisibleRow = false;

      rows.forEach(row => {
        const searchIn = [
          row.dataset.actionName,
          row.dataset.displayName,
          row.dataset.input,
          row.dataset.deviceName,
          row.dataset.inputDisplay
        ].filter(Boolean).join(' ').toLowerCase();

        const matches = !term || searchIn.includes(term);
        row.style.display = matches ? '' : 'none';
        if (matches) hasVisibleRow = true;
      });

      block.style.display = hasVisibleRow ? '' : 'none';
      if (term.length > 1 && hasVisibleRow) {
        block.classList.add('expanded');
      }
    });
  });

  document.querySelectorAll('[data-action="add-binding"]').forEach(btn => {
    btn.addEventListener('click', () => {
      openBindingEditor(btn.dataset.actionName, btn.dataset.category, null);
    });
  });

  document.querySelectorAll('[data-action="edit-binding"]').forEach(btn => {
    btn.addEventListener('click', () => {
      openBindingEditor(btn.dataset.actionName, btn.dataset.category, btn.dataset.input || '');
    });
  });

  document.querySelectorAll('[data-action="remove-binding"], [data-action="remove-binding-direct"]').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const actionName = btn.dataset.actionName;
      const category = btn.dataset.category;

      if (!lastRestoredBackupId) {
        showNotification('No profile loaded.', 'error');
        return;
      }

      const confirmed = await confirm(`Remove binding for "${actionName}" from your profile?`, {
        title: 'Remove Binding',
        kind: 'warning',
      });
      if (confirmed) {
        try {
          await invoke('remove_profile_binding', {
            v: activeScVersion,
            profileId: lastRestoredBackupId,
            actionMap: category,
            actionName: actionName,
          });

          showNotification('Binding removed from profile', 'success');
          await loadBackups();
          await loadCompleteBindingList();
          refreshBindingsInPlace();
        } catch (err) {
          showNotification(`Failed to remove binding: ${err}`, 'error');
        }
      }
    });
  });
}

function renderDeviceMapCollapsible() {
  const activeBackup = lastRestoredBackupId ? backups.find(b => b.id === lastRestoredBackupId) : null;
  const deviceMap = activeBackup?.device_map || [];
  if (deviceMap.length === 0) return '';

  const isExpanded = window.expandedPanels?.devices === true;

  return `
    <div class="sc-section collapsible-section">
      <div class="collapsible-header" data-panel="devices">
        <span class="collapsible-toggle ${isExpanded ? '' : 'collapsed'}">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="6 9 12 15 18 9"></polyline></svg>
        </span>
        <h3>
          Devices
          <span class="binding-stats-badge">${deviceMap.length} mapped</span>
        </h3>
      </div>
      <div class="collapsible-content ${isExpanded ? '' : 'collapsed'}">
        ${renderHint('devices-intro', 'These are the controllers stored in this profile. Star Control matches them to your connected hardware by name when capturing input.')}
        <div class="device-map-list">
          ${deviceMap.map(dm => `
            <div class="device-map-item" data-product="${escapeHtml(dm.product_name)}">
              <span class="device-map-type">${dm.device_type === 'joystick' ? 'JS' : dm.device_type.substring(0, 2).toUpperCase()}</span>
              <span class="device-map-instance">js${dm.sc_instance}</span>
              <span class="device-map-name" title="${escapeHtml(dm.product_name)}">${escapeHtml(dm.alias || dm.product_name)}</span>
              <button class="btn btn-xs device-map-alias-btn" data-product="${escapeHtml(dm.product_name)}" data-alias="${escapeHtml(dm.alias || '')}" title="Set alias">✏</button>
            </div>
          `).join('')}
        </div>
      </div>
    </div>
  `;
}

function renderBindingsCollapsible() {
  // Apply customized filter
  const sourceList = customizedOnly
    ? completeBindingList.filter(b => b.is_custom)
    : completeBindingList;

  // Group by technical category name but display with label
  const categorized = {};
  if (Array.isArray(sourceList)) {
    for (const b of sourceList) {
      const catKey = b.category || 'unknown';
      const catLabel = b.category_label || catKey;
      if (!categorized[catKey]) {
        categorized[catKey] = { label: catLabel, bindings: [], customCount: 0 };
      }
      categorized[catKey].bindings.push(b);
      if (b.is_custom) categorized[catKey].customCount++;
    }
  }

  const categoryKeys = Object.keys(categorized).sort((a, b) => {
    const labelA = categorized[a].label || '';
    const labelB = categorized[b].label || '';
    return labelA.toLowerCase().localeCompare(labelB.toLowerCase());
  });

  const isExpanded = window.expandedPanels?.bindings === true;

  return `
    <div class="sc-section collapsible-section">
      <div class="collapsible-header" data-panel="bindings">
        <span class="collapsible-toggle ${isExpanded ? '' : 'collapsed'}">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="6 9 12 15 18 9"></polyline></svg>
        </span>
        <h3>
          Keybindings
          <span class="binding-stats-badge" title="Total actions / Customized by you">
            ${bindingStats.custom} customized / ${bindingStats.total} total
          </span>
          ${localizationLoading ? '<span class="loading-spinner-inline" title="Loading translations..."></span>' : ''}
        </h3>
      </div>
      <div class="collapsible-content ${isExpanded ? '' : 'collapsed'}">
        ${renderHint('bindings-intro', 'Changes you make here are saved to this profile only — your game files stay untouched until you click <strong>Apply to SC</strong> above.')}
        <div class="bindings-toolbar">
          <input type="text" class="input binding-search" id="binding-search"
                 placeholder="Search actions..." value="${escapeHtml(bindingFilter)}"
                 aria-label="Search bindings" />
          <label class="customized-only-toggle">
            <input type="checkbox" id="customized-only-toggle" ${customizedOnly ? 'checked' : ''}>
            <span>Customized only</span>
          </label>
        </div>
        <div class="bindings-body">
          ${categoryKeys.length === 0
            ? `<div class="sc-hint">${customizedOnly ? 'No customized bindings.' : 'No keybindings found. This profile may not contain actionmaps.xml.'}</div>`
            : categoryKeys.map(catKey => renderBindingCategory(catKey, categorized[catKey].label, categorized[catKey].bindings)).join('')
          }
        </div>
      </div>
    </div>
  `;
}

// Keep old name as alias for refreshBindingsInPlace which references renderBindingCategory
function renderBindingsSection() { return renderBindingsCollapsible(); }

function renderBindingCategory(categoryKey, label, items) {
  const query = (bindingFilter || '').toLowerCase();
  const isExpanded = query.length > 0 || window.expandedBindingCategories.has(categoryKey);

  // Initial filtering - search ALL fields including device name, button/axis/hat names
  const filteredItems = !query ? items : items.filter(b => {
    const deviceName = resolveDeviceLabel(b.current_input);
    const inputDisplay = useHumanReadable ? formatInputDisplayText(b.current_input) : b.current_input;

    return (
      (b.action_name || '').toLowerCase().includes(query) ||
      (b.display_name || '').toLowerCase().includes(query) ||
      (b.current_input || '').toLowerCase().includes(query) ||
      (deviceName || '').toLowerCase().includes(query) ||
      (inputDisplay || '').toLowerCase().includes(query) ||
      (b.current_input || '').toLowerCase().replace(/_/g, ' ').includes(query) ||
      (b.category || '').toLowerCase().includes(query) ||
      (b.category_label || '').toLowerCase().includes(query)
    );
  });

  if (filteredItems.length === 0 && query) return '';

  return `
    <div class="binding-category-block ${isExpanded ? 'expanded' : ''}" data-category="${escapeHtml(categoryKey)}">
      <div class="binding-category-header" onclick="this.parentElement.classList.toggle('expanded'); if(this.parentElement.classList.contains('expanded')){window.expandedBindingCategories.add('${categoryKey}');}else{window.expandedBindingCategories.delete('${categoryKey}');}">
        <span class="category-title">${escapeHtml(label)}</span>
        <span class="category-count">${items.length} Actions</span>
        <svg class="chevron" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <polyline points="6 9 12 15 18 9"></polyline>
        </svg>
      </div>
      <div class="binding-category-content">
        <table class="bindings-table">
          <thead>
            <tr>
              <th style="width: 35%">Action</th>
              <th style="width: 45%">Binding</th>
              <th style="width: 20%">Actions</th>
            </tr>
          </thead>
          <tbody>
            ${items.map(b => {
              // Build searchable text for filter (includes device name and input display)
              const deviceType = resolveDeviceType(b.current_input);
              const deviceName = resolveDeviceLabel(b.current_input);
              const inputDisplay = useHumanReadable ? formatInputDisplayText(b.current_input) : b.current_input;
              const searchText = [
                b.action_name,
                b.display_name,
                b.current_input,
                deviceName,
                inputDisplay,
                b.category,
                b.category_label
              ].filter(Boolean).join(' ').toLowerCase();

              const matches = !query || searchText.includes(query);

              return `
                <tr class="binding-row"
                    style="display: ${matches ? '' : 'none'}"
                    data-action-name="${escapeHtml(b.action_name)}"
                    data-display-name="${escapeHtml(b.display_name)}"
                    data-input="${escapeHtml(b.current_input)}"
                    data-device-name="${escapeHtml(deviceName)}"
                    data-input-display="${escapeHtml(inputDisplay)}">
                  <td>
                    <div class="binding-action-name">${escapeHtml(b.display_name || b.action_name)}</div>
                    <div class="binding-action-key">${escapeHtml(b.action_name)}</div>
                  </td>
                  <td class="binding-inputs-cell">
                    ${b.current_input
                      ? `
                            <div class="binding-input-row ${b.is_custom ? 'custom-binding' : ''}">
                              <span class="binding-device-tag ${deviceType}">${getDeviceIconSvg(deviceType)}${escapeHtml(deviceName)}</span>
                              <code class="binding-input">${escapeHtml(inputDisplay)}</code>
                              ${b.is_custom ? '<span class="custom-badge" title="Modified by you">Custom</span>' : ''}
                            </div>
                          `
                      : `<span class="binding-unbound">Unbound</span>`
                    }
                  </td>
                  <td class="binding-actions-cell">
                    <div class="action-buttons-flex">
                      <button class="btn btn-xs btn-primary" 
                              data-action="edit-binding"
                              data-action-name="${escapeHtml(b.action_name)}" 
                              data-category="${escapeHtml(categoryKey)}"
                              data-input="${escapeHtml(b.current_input)}">Edit</button>
                      ${b.current_input ? `
                        <button class="btn btn-xs btn-danger-sm"
                                data-action="remove-binding-direct"
                                data-action-name="${escapeHtml(b.action_name)}"
                                data-category="${escapeHtml(categoryKey)}"
                                data-input="${escapeHtml(b.current_input)}">
                          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
                        </button>
                      ` : ''}
                    </div>
                  </td>
                </tr>
              `;
            }).join('')}
          </tbody>
        </table>
      </div>
    </div>
  `;
}

function resolveDeviceType(input) {
  if (!input) return 'none';
  if (input.startsWith('kb')) return 'keyboard';
  if (input.startsWith('mo')) return 'mouse';
  if (input.startsWith('xi') || input.startsWith('gp')) return 'gamepad';
  if (input.startsWith('js')) return 'joystick';
  return 'unknown';
}

function formatDeviceType(deviceType) {
  const labels = {
    keyboard: 'Keyboard',
    mouse: 'Mouse',
    gamepad: 'Gamepad',
    joystick: 'Joystick',
    none: 'None',
    unknown: 'Unknown',
  };
  return labels[deviceType] || deviceType;
}

/**
 * Resolve a concrete device name from the active profile's device_map.
 * E.g. "js1_button12" → "VKB Gladiator NXT" (via device_map sc_instance=1)
 * Falls back to generic type name if no device_map match.
 */
function resolveDeviceLabel(input) {
  if (!input) return 'Unbound';
  const deviceType = resolveDeviceType(input);

  // Extract instance number from input prefix (js1_, js2_, etc.)
  const instanceMatch = input.match(/^js(\d+)_/);
  if (instanceMatch) {
    const scInstance = parseInt(instanceMatch[1], 10);
    const activeBackup = lastRestoredBackupId ? backups.find(b => b.id === lastRestoredBackupId) : null;
    const deviceMap = activeBackup?.device_map || [];
    const dm = deviceMap.find(d => d.sc_instance === scInstance && d.device_type === 'joystick');
    if (dm) {
      return dm.alias || dm.product_name;
    }
    // Fallback: show instance number
    return `Joystick ${scInstance}`;
  }

  return formatDeviceType(deviceType);
}

/**
 * Generate SVG icon for device type
 */
function getDeviceIconSvg(deviceType) {
  const icons = {
    keyboard: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="20" height="16" rx="2"/><line x1="6" y1="8" x2="6" y2="8"/><line x1="10" y1="8" x2="10" y2="8"/><line x1="14" y1="8" x2="14" y2="8"/><line x1="18" y1="8" x2="18" y2="8"/><line x1="6" y1="12" x2="6" y2="12"/><line x1="10" y1="12" x2="10" y2="12"/><line x1="14" y1="12" x2="14" y2="12"/><line x1="18" y1="12" x2="18" y2="12"/><line x1="8" y1="16" x2="16" y2="16"/></svg>`,
    mouse: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="6" y="2" width="12" height="20" rx="6"/><line x1="12" y1="6" x2="12" y2="10"/></svg>`,
    joystick: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="3"/><line x1="12" y1="3" x2="12" y2="6"/><line x1="12" y1="18" x2="12" y2="21"/><line x1="3" y1="12" x2="6" y2="12"/><line x1="18" y1="12" x2="21" y2="12"/></svg>`,
    gamepad: `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="6" width="20" height="12" rx="4"/><circle cx="6" cy="12" r="2"/><circle cx="10" cy="9" r="1"/><circle cx="14" cy="9" r="1"/><circle cx="18" cy="12" r="2"/></svg>`,
  };
  const icon = icons[deviceType] || icons.joystick;
  return `<span class="device-icon">${icon}</span>`;
}

function formatInputDisplayText(input) {
  if (!input) return '';

  // button26 -> Button #26
  const btnMatch = input.match(/button(\d+)/i);
  if (btnMatch) return `Button #${btnMatch[1]}`;

  // POV/Hat without direction - treat as POV
  const povMatch = input.match(/pov(\d+)/i);
  if (povMatch) return `POV #${povMatch[1]}`;

  // Axes with optional neg/pos suffix
  const axisMap = {
    x: 'X-Axis', y: 'Y-Axis', z: 'Z-Axis',
    rotx: 'Rot X', roty: 'Rot Y', rotz: 'Rot Z',
    slider1: 'Slider 1', slider2: 'Slider 2'
  };
  const axisMatch = input.match(/(x|y|z|rotx|roty|rotz|slider1|slider2)(neg|pos)?/i);
  if (axisMatch) {
    const baseAxis = axisMap[axisMatch[1].toLowerCase()];
    if (baseAxis) {
      const suffix = axisMatch[2] ? (axisMatch[2].toLowerCase() === 'neg' ? ' (-)' : ' (+)') : '';
      return baseAxis + suffix;
    }
  }

  // Hats mit Richtung: hat1_up, hat1_down, hat1_left, hat1_right
  const hatDirMatch = input.match(/hat(\d+)_(up|down|left|right)/i);
  if (hatDirMatch) {
    const hatNum = hatDirMatch[1];
    const dir = hatDirMatch[2].toLowerCase();
    const dirMap = { up: '↑ Up', down: '↓ Down', left: '← Left', right: '→ Right' };
    return `Hat #${hatNum} ${dirMap[dir] || dir}`;
  }

  // Hats ohne Richtung
  const hatMatch = input.match(/hat(\d+)/i);
  if (hatMatch) return `Hat #${hatMatch[1]}`;

  return input;
}

// ==================== Binding Editor ====================

/** Strip device prefix for display (e.g. "js2_x" → "x", "kb1_w" → "w", "mo1_button2" → "button2") */
function stripDevicePrefix(input) {
  if (!input) return input;
  return input.replace(/^(js|kb|mo)\d+_/, '');
}

function openBindingEditor(actionName, category, currentInput) {
  console.log('[EDITOR] Opening...', { actionName, category, currentInput });
  bindingEditorAction = { actionName, category, currentInput };
  bindingEditorDevice = resolveDeviceType(currentInput) || 'keyboard';

  // Create modal
  const modal = document.createElement('div');
  modal.className = 'modal-overlay';
  modal.id = 'binding-editor-modal';

  const isEdit = currentInput && currentInput.length > 0;
  const title = isEdit ? 'Edit Binding' : 'Add Binding';

  modal.innerHTML = `
    <div class="modal-content binding-editor-modal">
      <div class="modal-header">
        <h3>${title}</h3>
        <button class="modal-close" data-action="close-binding-editor">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>
      <div class="modal-body">
        <div class="binding-editor-context">
          <span class="binding-editor-context-action">${escapeHtml(actionName)}</span>
          <span class="binding-editor-context-sep">/</span>
          <span class="binding-editor-context-category">${escapeHtml(category)}</span>
        </div>
        <div class="capture-zone" id="capture-container">
          <label class="capture-zone-label">Press a key, button, or move an axis</label>
          <div class="capture-zone-input-wrap">
            <input type="text" class="capture-input" id="binding-input-field"
                   value="${stripDevicePrefix(currentInput) || ''}"
                   placeholder="Waiting for input..." readonly
                   aria-label="Captured input">
          </div>
        </div>
        <div class="binding-editor-device">
          <label>Device</label>
          <select id="capture-device-select" class="capture-device-select" aria-label="Device">
            <option value="">Loading devices...</option>
          </select>
        </div>
      </div>
      <div class="modal-footer">
        ${isEdit ? '<button class="btn btn-danger" id="btn-delete-binding">Delete</button>' : '<span></span>'}
        <div class="modal-footer-actions">
          <button class="btn btn-secondary" data-action="close-binding-editor">Cancel</button>
          <button class="btn btn-primary" id="btn-save-binding">Save</button>
        </div>
      </div>
    </div>
  `;

  document.body.appendChild(modal);

  // Get active profile's device map for Linux→SC instance remapping
  const activeBackup = lastRestoredBackupId ? backups.find(b => b.id === lastRestoredBackupId) : null;
  const profileDeviceMap = activeBackup?.device_map || [];

  // Load connected devices and show alongside profile device map
  const deviceSelect = modal.querySelector('#capture-device-select');
  let connectedDevices = [];

  invoke('list_connected_devices').then(devices => {
    connectedDevices = devices || [];
    console.log('[EDITOR] Connected devices:', connectedDevices);
    if (profileDeviceMap.length > 0) {
      // Show profile's SC devices with connection status
      deviceSelect.innerHTML = `
        <option value="">Auto-detect device</option>
        ${profileDeviceMap.map(dm => {
          const connected = connectedDevices.find(cd =>
            cd.product_name.toLowerCase() === dm.product_name.toLowerCase()
          );
          const alias = dm.alias || dm.product_name;
          const status = connected ? '●' : '○';
          return `
            <option value="${escapeHtml(dm.product_name)}" data-sc-instance="${dm.sc_instance}">
              ${status} ${escapeHtml(alias)} (SC js${dm.sc_instance})
            </option>
          `;
        }).join('')}
      `;
    } else if (connectedDevices.length > 0) {
      deviceSelect.innerHTML = `
        <option value="">Auto-detect device</option>
        ${connectedDevices.map(d => `
          <option value="${escapeHtml(d.product_name)}" data-instance="${d.instance}">
            ${escapeHtml(d.product_name)} (js${d.instance})
          </option>
        `).join('')}
      `;
    } else {
      deviceSelect.innerHTML = '<option value="">No joystick/gamepad detected</option>';
    }
  }).catch(err => {
    console.error('[EDITOR] Failed to list devices:', err);
    deviceSelect.innerHTML = '<option value="">Error loading devices</option>';
  });

  /**
   * Remap a captured joystick input from Linux gilrs instance to SC instance.
   * Uses the profile's device_map to find the SC instance for a given product_name.
   * @param {string} rawInput - e.g. "js2_button5"
   * @param {string} productName - device product name from gilrs
   * @returns {string} remapped input e.g. "js1_button5"
   */
  function remapToScInstance(rawInput, productName) {
    if (!productName || profileDeviceMap.length === 0) return rawInput;

    const match = profileDeviceMap.find(dm =>
      dm.product_name.toLowerCase() === productName.toLowerCase()
    );
    if (!match) return rawInput;

    // Replace js{N}_ prefix with SC instance
    return rawInput.replace(/^js\d+_/, `js${match.sc_instance}_`);
  }

  const inputField = modal.querySelector('#binding-input-field');
  let inputCapturedUnlisten = null;
  let isLocked = false; // Jitter protection lock
  let capturedDeviceUuid = '';
  let capturedDeviceName = '';
  let capturedRawCode = currentInput || ''; // Raw internal code for saving (with js{N}_ prefix)

  const setCapturedInput = (captureData) => {
    if (isLocked) return;
    isLocked = true;

    // Handle both old string format (keyboard/mouse) and new object format (joystick)
    let code, deviceUuid, deviceName;
    if (typeof captureData === 'string') {
      // Keyboard or mouse input (legacy format)
      code = captureData;
      deviceUuid = '';
      deviceName = '';
      inputField.value = stripDevicePrefix(code);
    } else if (typeof captureData === 'object' && captureData !== null) {
      // Joystick input (new format with device info)
      const rawCode = captureData.input || '';
      deviceUuid = captureData.linux_uuid || '';
      deviceName = captureData.product_name || '';
      capturedDeviceUuid = deviceUuid;
      capturedDeviceName = deviceName;

      // Remap Linux gilrs instance to SC instance using profile's device_map
      code = remapToScInstance(rawCode, deviceName);

      // Show human-readable input (strip js{N}_ prefix for display)
      const displayCode = stripDevicePrefix(code);
      const dmEntry = profileDeviceMap.find(dm =>
        dm.product_name.toLowerCase() === (deviceName || '').toLowerCase()
      );
      const displayName = dmEntry?.alias || deviceName;
      if (displayName) {
        inputField.value = `${displayCode} (${displayName})`;
      } else {
        inputField.value = displayCode;
      }
    } else {
      code = String(captureData || '');
      deviceUuid = '';
      deviceName = '';
    }

    // Store raw code for saving (with SC js{N}_ prefix after remapping)
    capturedRawCode = code;

    inputField.classList.add('captured-pulse');

    // Lock for 1 second after capture to prevent axis jitter from overwriting buttons
    setTimeout(() => {
      inputField.classList.remove('captured-pulse');
      isLocked = false;
    }, 1000);
  };

  // 1. Listen for Backend Hardware Events (Joysticks via Rust)
  listen('input-captured', (event) => {
    console.log('[EDITOR] Hardware event received from Rust:', event.payload);
    setCapturedInput(event.payload);
  }).then(unlisten => {
    inputCapturedUnlisten = unlisten;
  });

  // 2. Local Keyboard Capture
  const handleKeyDownCapture = (e) => {
    e.preventDefault();
    if (['Control', 'Alt', 'Shift', 'Meta', 'AltGraph'].includes(e.key)) return;

    const parts = [];
    if (e.ctrlKey) parts.push('lctrl');
    if (e.altKey || e.key === 'AltGraph') parts.push('lalt');
    if (e.shiftKey) parts.push('lshift');

    let key = e.key.toLowerCase();
    const keyMap = { 'arrowup': 'up', 'arrowdown': 'down', 'arrowleft': 'left', 'arrowright': 'right', ' ': 'space', 'escape': 'esc', 'enter': 'return', 'backspace': 'backspace', 'tab': 'tab', 'insert': 'insert', 'delete': 'delete', 'home': 'home', 'end': 'end', 'pageup': 'pgup', 'pagedown': 'pgdn' };
    if (keyMap[key]) key = keyMap[key];

    if (key.length === 1 || key.startsWith('f') || keyMap[e.key.toLowerCase()]) {
        parts.push(key);
        setCapturedInput(`kb1_${parts.join('+')}`);
    }
  };

  // 3. Local Mouse Button Capture
  const handleMouseDownCapture = (e) => {
    // Ignore Left Mouse Button (0) to allow clicking UI buttons like "Save"
    if (e.button === 0) return;

    const btnMap = { 1: 'button3', 2: 'button2', 3: 'button4', 4: 'button5' };
    const btn = btnMap[e.button];
    if (btn) {
        setCapturedInput(`mo1_${btn}`);
    }
  };

  window.addEventListener('keydown', handleKeyDownCapture);
  window.addEventListener('mousedown', handleMouseDownCapture);

  // Start Hardware Capture in Backend
  invoke('start_input_capture').catch(err => console.error('[EDITOR] Backend capture start failed:', err));

  const cleanupAndClose = () => {
    console.log('[EDITOR] Cleaning up and closing...');
    invoke('stop_input_capture');
    if (inputCapturedUnlisten) inputCapturedUnlisten();
    window.removeEventListener('keydown', handleKeyDownCapture);
    window.removeEventListener('mousedown', handleMouseDownCapture);
    modal.remove();
    bindingEditorAction = null;
  };

  modal.querySelectorAll('[data-action="close-binding-editor"]').forEach(btn => {
    btn.addEventListener('click', cleanupAndClose);
  });

  modal.addEventListener('click', (e) => { if (e.target === modal) cleanupAndClose(); });

  modal.querySelector('#btn-delete-binding')?.addEventListener('click', async () => {
    if (!lastRestoredBackupId) {
      showNotification('No profile loaded.', 'error');
      return;
    }

    try {
      await invoke('remove_profile_binding', {
        v: activeScVersion,
        profileId: lastRestoredBackupId,
        actionMap: category,
        actionName: actionName,
      });

      showNotification('Binding removed from profile', 'success');
      if (bindingEditorAction?.category) {
        window.expandedBindingCategories.add(bindingEditorAction.category);
      }
      cleanupAndClose();
      await loadBackups(); // refresh dirty flag
      await loadCompleteBindingList();
      refreshBindingsInPlace();
    } catch (err) {
      console.error('[EDITOR] Delete failed:', err);
      showNotification('Delete Error: ' + err, 'error');
    }
  });

  modal.querySelector('#btn-save-binding').addEventListener('click', async () => {
    // Use the raw captured code (preserves js{N}_ prefix needed internally)
    const newInput = capturedRawCode.trim();

    if (!newInput) {
      showNotification('No input captured. Press a key, button, or move an axis first.', 'error');
      return;
    }

    // Check for binding conflicts — compare by input identifier + same device (product name)
    if (completeBindingList.length > 0) {
      const newInputBare = stripDevicePrefix(newInput);
      const captureDev = (capturedDeviceName || '').trim().toLowerCase();
      const conflicting = completeBindingList.find(b => {
        if (b.action_name === actionName || !b.current_input) return false;
        const existingBare = stripDevicePrefix(b.current_input);
        if (existingBare !== newInputBare) return false;
        // Only conflict if same device (by product name)
        const existingDev = (b.device_type || '').trim().toLowerCase();
        return captureDev && existingDev && (captureDev.includes(existingDev) || existingDev.includes(captureDev));
      });
      if (conflicting) {
        const displayInput = stripDevicePrefix(newInput);
        const proceed = await confirm(
          `"${displayInput}" is already assigned to "${conflicting.display_name || conflicting.action_name}". Assign anyway?`,
          { title: 'Binding Conflict', kind: 'warning' }
        );
        if (!proceed) return;
      }
    }

    if (!lastRestoredBackupId) {
      showNotification('No profile loaded. Save your current SC settings as a profile first.', 'error');
      return;
    }

    try {
      await invoke('assign_profile_binding', {
        v: activeScVersion,
        profileId: lastRestoredBackupId,
        actionMap: category,
        actionName: actionName,
        newInput: newInput,
        oldInput: currentInput || null,
      });

      showNotification('Binding saved to profile', 'success');
      if (bindingEditorAction?.category) {
        window.expandedBindingCategories.add(bindingEditorAction.category);
      }
      cleanupAndClose();
      await loadBackups(); // refresh dirty flag
      await loadCompleteBindingList();
      refreshBindingsInPlace();
    } catch (err) {
      console.error('[EDITOR] Save failed. Error:', err);
      showNotification('Save Error: ' + err, 'error');
    }
  });
}

// Make globally accessible
window.openBindingEditor = openBindingEditor;


function formatCategoryName(name) {
  return name
    .replace(/^spaceship_/, '')
    .replace(/^vehicle_/, 'veh_')
    .replace(/^player_/, '')
    .replace(/_/g, ' ')
    .replace(/\b\w/g, c => c.toUpperCase());
}

// ==================== Changes Panel ====================

function renderChangesPanel(files) {
  const statusOrder = { modified: 0, new: 1, deleted: 2, unchanged: 3 };
  const sorted = [...files].sort((a, b) => (statusOrder[a.status] ?? 4) - (statusOrder[b.status] ?? 4));

  return `
    <div class="profile-changes-panel">
      ${sorted.map(f => `
        <div class="profile-file-status">
          <span class="file-name">${escapeHtml(f.file)}</span>
          <span class="status-badge status-${f.status}">${f.status}</span>
        </div>
      `).join('')}
    </div>`;
}

// ==================== Backups Section ====================

function formatBackupFiles(files) {
  let profiles = 0, mappings = 0, characters = 0;
  for (const f of files) {
    if (f.startsWith('controls_mappings/')) mappings++;
    else if (f.startsWith('custom_characters/')) characters++;
    else profiles++;
  }
  const parts = [];
  if (profiles > 0) parts.push(`${profiles} profile${profiles !== 1 ? 's' : ''}`);
  if (mappings > 0) parts.push(`${mappings} mapping${mappings !== 1 ? 's' : ''}`);
  if (characters > 0) parts.push(`${characters} character${characters !== 1 ? 's' : ''}`);
  return parts.join(' + ') || '0 files';
}

function formatProfileTypeBadge(backupType) {
  const map = {
    'manual': 'saved',
    'pre-import': 'pre-import',
    // Legacy types from older versions
    'auto': 'auto-save',
    'auto-pre-restore': 'auto-save',
    'auto-pre-import': 'pre-import',
    'auto-post-import': 'imported',
  };
  return map[backupType] || backupType;
}

// ==================== USER.cfg UI ====================

function getChangedSettingsCount() {
  let count = 0;
  for (const [key, setting] of Object.entries(DEFAULT_SETTINGS)) {
    const currentValue = userCfgSettings[key] !== undefined ? userCfgSettings[key] : setting.value;
    if (currentValue !== setting.value) count++;
  }
  return count;
}

function renderUserCfgUI() {
  const essentialCategory = { key: 'essential', label: 'Essential Settings' };
  const advancedCategories = [
    { key: 'performance', label: 'Performance' },
    { key: 'quality', label: 'Graphics Quality' },
    { key: 'shaders', label: 'Shader Quality' },
    { key: 'textures', label: 'Textures' },
    { key: 'effects', label: 'Visual Effects' },
    { key: 'clarity', label: 'Visual Clarity' },
    { key: 'lod', label: 'View Distance' },
  ];

  const changedCount = getChangedSettingsCount();

  return `
    <div class="sc-section usercfg-section">
      <div class="sc-section-header">
        <h3>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"></path>
            <circle cx="12" cy="12" r="3"></circle>
          </svg>
          USER.cfg Settings - ${escapeHtml(activeScVersion)}
        </h3>
        <div class="sc-section-actions">
          <span class="usercfg-unsaved" id="usercfg-unsaved" style="display:none">Unsaved changes</span>
          <button class="btn btn-sm btn-primary" id="btn-apply-usercfg">Apply</button>
          <button class="btn btn-sm btn-secondary" id="btn-reset-usercfg">Reset</button>
        </div>
      </div>
      <div class="usercfg-body">
        <div class="usercfg-header-info">
          <span class="usercfg-header-icon">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"></path>
              <polyline points="17 21 17 13 7 13 7 21"></polyline>
              <polyline points="7 3 7 8 15 8"></polyline>
            </svg>
          </span>
          <span>Only changed values are saved to USER.cfg. </span>
          <span class="usercfg-header-count">${changedCount > 0 ? `${changedCount} changed` : 'All defaults'}</span>
        </div>
        <div class="usercfg-categories">
          ${renderCategorySettings(essentialCategory, false)}
          ${advancedCategories.map(cat => renderCategorySettings(cat, true)).join('')}
        </div>
      </div>
    </div>
  `;
}

function renderCategorySettings(category, collapsible) {
  const settings = Object.entries(DEFAULT_SETTINGS)
    .filter(([_, s]) => s.category === category.key)
    .map(([key, s]) => ({ key, ...s }));

  if (settings.length === 0) return '';

  const isCollapsed = collapsible && collapsedCategories.has(category.key);
  const changedInCategory = settings.filter(s => {
    const val = userCfgSettings[s.key] !== undefined ? userCfgSettings[s.key] : s.value;
    return val !== s.value;
  }).length;

  return `
    <div class="usercfg-category">
      <div class="usercfg-category-header ${collapsible ? 'collapsible' : ''}"
           ${collapsible ? `data-category-key="${escapeHtml(category.key)}"` : ''}>
        <span class="usercfg-category-label">${category.label}</span>
        ${changedInCategory > 0 ? `<span class="usercfg-category-badge">${changedInCategory} changed</span>` : ''}
        ${collapsible ? `
          <svg class="usercfg-category-toggle ${isCollapsed ? 'collapsed' : ''}" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="6 9 12 15 18 9"></polyline>
          </svg>
        ` : ''}
      </div>
      <div class="usercfg-settings ${isCollapsed ? 'collapsed' : ''}">
        ${settings.map(s => renderSettingControl(s.key, s)).join('')}
      </div>
    </div>
  `;
}

function renderSettingControl(key, setting) {
  const value = userCfgSettings[key] !== undefined ? userCfgSettings[key] : setting.value;
  const isChanged = value !== setting.value;
  const changedClass = isChanged ? 'usercfg-changed' : '';

  const resetBtn = isChanged
    ? `<button class="usercfg-reset" data-key="${key}" title="Reset to default">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="1 4 1 10 7 10"></polyline><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"></path></svg>
      </button>`
    : '';

  if (setting.type === 'toggle') {
    const defaultLabel = setting.value ? 'On' : 'Off';
    return `
      <div class="usercfg-row ${changedClass}" data-tooltip="${escapeHtml(setting.desc)}" data-tooltip-pos="left">
        <span class="usercfg-label">${setting.label}${isChanged ? ` <span class="usercfg-default">(Default: ${defaultLabel})</span>` : ''}</span>
        <div class="usercfg-control-wrap">
          <label class="toggle-switch">
            <input type="checkbox" class="usercfg-input" data-key="${key}" ${value ? 'checked' : ''} />
            <span class="toggle-slider"></span>
          </label>
          ${resetBtn}
        </div>
      </div>
    `;
  }

  if (setting.type === 'number') {
    return `
      <div class="usercfg-row ${changedClass}" data-tooltip="${escapeHtml(setting.desc)}" data-tooltip-pos="left">
        <span class="usercfg-label">${setting.label}${isChanged ? ` <span class="usercfg-default">(Default: ${setting.value})</span>` : ''}</span>
        <div class="usercfg-control-wrap">
          <input type="number" class="usercfg-number-input" data-key="${key}"
                 min="${setting.min}" max="${setting.max}" step="${setting.step}" value="${value}"
                 aria-label="${escapeHtml(setting.label)}" />
          ${resetBtn}
        </div>
      </div>
    `;
  }

  let displayValue = value;
  if (setting.labels) displayValue = setting.labels[value] || value;
  else if (setting.category === 'quality') displayValue = QUALITY_LEVELS[value] || value;
  else if (setting.category === 'shaders') displayValue = SHADER_LEVELS[value] || value;

  let defaultDisplay = setting.value;
  if (setting.labels) defaultDisplay = setting.labels[setting.value] || setting.value;
  else if (setting.category === 'quality') defaultDisplay = QUALITY_LEVELS[setting.value] || setting.value;
  else if (setting.category === 'shaders') defaultDisplay = SHADER_LEVELS[setting.value] || setting.value;

  return `
    <div class="usercfg-row ${changedClass}" title="${escapeHtml(setting.desc)}">
      <span class="usercfg-label">${setting.label}${isChanged ? ` <span class="usercfg-default">(Default: ${defaultDisplay})</span>` : ''}</span>
      <div class="usercfg-control-wrap">
        <div class="usercfg-slider-wrap">
          <input type="range" class="usercfg-slider" data-key="${key}"
                 min="${setting.min}" max="${setting.max}" step="${setting.step}" value="${value}"
                 aria-label="${escapeHtml(setting.label)}" />
          <span class="usercfg-value">${displayValue}</span>
        </div>
        ${resetBtn}
      </div>
    </div>
  `;
}

// ==================== Actions ====================

async function applyUserCfg() {
  if (!config?.install_path || !activeScVersion) return;

  document.querySelectorAll('.usercfg-slider').forEach(slider => {
    const val = parseFloat(slider.value);
    if (!isNaN(val)) userCfgSettings[slider.dataset.key] = val;
  });
  document.querySelectorAll('.usercfg-number-input').forEach(input => {
    const val = parseFloat(input.value);
    if (!isNaN(val)) userCfgSettings[input.dataset.key] = val;
  });
  document.querySelectorAll('.usercfg-input[type="checkbox"]').forEach(checkbox => {
    userCfgSettings[checkbox.dataset.key] = checkbox.checked ? 1 : 0;
  });

  const content = generateUserCfg();

  try {
    await invoke('write_user_cfg', { gp: config.install_path, v: activeScVersion, c: content });
    savedUserCfgSnapshot = { ...userCfgSettings };
    showNotification('USER.cfg saved. Restart Star Citizen to apply changes.', 'success');
    updateChangedCounts();
  } catch (e) {
    showNotification('Failed to write USER.cfg', 'error');
  }
}

async function resetUserCfg() {
  if (!config?.install_path || !activeScVersion) return;
  const confirmed = await confirm('Reset all settings to defaults?', { title: 'Reset USER.cfg', kind: 'warning' });
  if (!confirmed) return;
  userCfgSettings = {};
  try {
    await invoke('write_user_cfg', { gp: config.install_path, v: activeScVersion, c: '' });
    showNotification('USER.cfg reset', 'success');
    renderEnvironments(document.getElementById('content'));
  } catch (e) {
    showNotification('Failed to reset USER.cfg', 'error');
  }
}

function generateUserCfg() {
  const lines = [
    '; Star Citizen USER.cfg Configuration',
    '; Generated by Star Control',
    '; Only non-default values are stored',
    '',
  ];

  const categoryOrder = ['essential', 'performance', 'quality', 'shaders', 'textures', 'effects', 'clarity', 'lod'];

  for (const cat of categoryOrder) {
    const catSettings = Object.entries(DEFAULT_SETTINGS).filter(([_, s]) => s.category === cat);
    const changedSettings = [];

    for (const [key, setting] of catSettings) {
      const currentValue = userCfgSettings[key] !== undefined ? userCfgSettings[key] : setting.value;
      if (currentValue !== setting.value) {
        const defaultStr = setting.type === 'toggle' ? (setting.value ? '1' : '0') : String(setting.value);
        changedSettings.push({ key, setting, value: currentValue, defaultValue: defaultStr });
      }
    }

    if (changedSettings.length > 0) {
      lines.push(`;--- ${cat.charAt(0).toUpperCase() + cat.slice(1)} ---`);
      for (const { key, setting, value, defaultValue } of changedSettings) {
        if (setting.type === 'toggle') {
          lines.push(`${key} = ${value ? 1 : 0}  ; default: ${defaultValue}`);
        } else {
          lines.push(`${key} = ${value}  ; default: ${defaultValue}`);
        }
      }
      lines.push('');
    }
  }

  // Preserve extra keys not managed by DEFAULT_SETTINGS (e.g. g_language, custom CVars)
  const managedKeys = new Set(Object.keys(DEFAULT_SETTINGS));
  const extraKeys = Object.keys(userCfgSettings).filter(k => !managedKeys.has(k));
  if (extraKeys.length > 0) {
    lines.push(';--- Other ---');
    for (const key of extraKeys) {
      const value = userCfgSettings[key];
      if (value !== undefined && value !== '') {
        lines.push(`${key} = ${value}`);
      }
    }
    lines.push('');
  }

  return lines.join('\n');
}

async function saveProfile() {
  if (!config?.install_path || !activeScVersion) return;

  // Show inline label input
  const btn = document.getElementById('btn-save-current') || document.getElementById('btn-save-first-profile');
  if (!btn) return;
  const header = btn.closest('.sc-section-header') || btn.parentElement;
  if (!header) return;

  // Check if input already shown
  if (header.querySelector('.backup-label-input-wrap')) return;

  const wrap = document.createElement('div');
  wrap.className = 'backup-label-input-wrap';
  wrap.innerHTML = `
    <input type="text" class="input backup-label-input" placeholder="Profile name (optional)" maxlength="60" aria-label="Profile name" />
    <button class="btn btn-sm btn-primary" id="btn-backup-confirm">Save</button>
    <button class="btn btn-sm" id="btn-backup-cancel">Cancel</button>
  `;
  header.after(wrap);
  const input = wrap.querySelector('.backup-label-input');
  input.focus();

  async function doCreate() {
    const label = input.value.trim();
    wrap.remove();
    try {
      await invoke('backup_profile', {
        gp: config.install_path,
        v: activeScVersion,
        bt: 'manual',
        l: label || '',
      });
      showNotification('Profile saved', 'success');
      await loadBackups();
      await loadProfileStatus();
      renderEnvironments(document.getElementById('content'));
    } catch (e) {
      showNotification(`Save failed: ${e}`, 'error');
    }
  }

  wrap.querySelector('#btn-backup-confirm').addEventListener('click', doCreate);
  wrap.querySelector('#btn-backup-cancel').addEventListener('click', () => wrap.remove());
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') doCreate();
    if (e.key === 'Escape') wrap.remove();
  });
}

async function loadProfile(backupId) {
  if (!config?.install_path || !activeScVersion) return;
  const backup = backups.find(b => b.id === backupId);
  const displayName = backup?.label || backupId;
  const filesInfo = backup ? formatBackupFiles(backup.files) : '';
  const confirmLoad = await confirm(
    `Load "${displayName}" into Star Citizen (${activeScVersion})?\n\nYour current SC settings will be replaced.\n\nIncludes: ${filesInfo}`,
    { title: 'Load Profile', kind: 'warning' }
  );
  if (!confirmLoad) return;
  try {
    await invoke('restore_profile', {
      gp: config.install_path,
      v: activeScVersion,
      bid: backupId,
    });
    lastRestoredBackupId = backupId;
    lastRestoredPerVersion[activeScVersion] = backupId;
    invoke('save_active_profile', { v: activeScVersion, bid: backupId }).catch(() => {});
    showNotification('Profile loaded', 'success');
    await Promise.all([loadActionDefinitions(), loadDevicesAndBindings(), loadCompleteBindingList(), loadBackups(), loadUserCfgSettings()]);
    await loadProfileStatus();
    renderEnvironments(document.getElementById('content'));
  } catch (e) {
    showNotification(`Load failed: ${e}`, 'error');
  }
}

async function deleteProfile(backupId) {
  const backup = backups.find(b => b.id === backupId);
  const displayName = backup?.label || 'Unnamed profile';
  const confirmDelete = await confirm(`Delete profile "${displayName}"?`, { title: 'Delete Profile', kind: 'warning' });
  if (!confirmDelete) return;
  try {
    await invoke('delete_backup', { v: activeScVersion, bid: backupId });
    if (lastRestoredBackupId === backupId) {
      lastRestoredBackupId = null;
      delete lastRestoredPerVersion[activeScVersion];
      activeProfileStatus = null;
      invoke('save_active_profile', { v: activeScVersion, bid: '' }).catch(() => {});
    }
    showNotification('Profile deleted', 'success');
    await loadBackups();
    await loadProfileStatus();
    renderEnvironments(document.getElementById('content'));
  } catch (e) {
    showNotification(`Delete failed: ${e}`, 'error');
  }
}

async function deleteScVersion(version) {
  if (!version) return;

  const confirmed = await confirm(`Are you absolutely sure you want to delete the environment ${version}? This will permanently remove the folder and all game data.`, {
    title: 'Delete Environment',
    kind: 'warning',
  });

  if (!confirmed) return;
  if (!config?.install_path) {
    showNotification('No installation path configured.', 'error');
    return;
  }

  try {
    showNotification(`Deleting environment ${version}...`, 'info');
    await invoke('delete_sc_version', { gp: config.install_path, version });
    
    // Clear active version if we just deleted it
    if (activeScVersion === version) {
      activeScVersion = null;
      lastRestoredBackupId = null;
      activeProfileTab = 'profile'; 
    }
    
    showNotification(`Environment ${version} deleted successfully.`, 'success');
    
    // Reload environments
    renderEnvironments(document.getElementById('content'));
  } catch (err) {
    showNotification(`Failed to delete environment: ${err}`, 'error');
  }
}

async function handleDeviceDrop(sourceInstance, targetInstance, sourceDeviceType = 'joystick', targetDeviceType = 'joystick') {
  if (sourceInstance === targetInstance) return;
  if (!config?.install_path || !activeScVersion) return;

  const newOrder = [
    { old_instance: sourceInstance, new_instance: targetInstance, device_type: sourceDeviceType },
    { old_instance: targetInstance, new_instance: sourceInstance, device_type: targetDeviceType },
  ];

  try {
    await invoke('reorder_devices', {
      gamePath: config.install_path,
      version: activeScVersion,
      newOrder,
    });
    showNotification(`Swapped ${sourceDeviceType.toUpperCase()}${sourceInstance} and ${targetDeviceType.toUpperCase()}${targetInstance}`, 'success');
    await loadDevicesAndBindings();
    await loadCompleteBindingList();
    renderEnvironments(document.getElementById('content'));
  } catch (e) {
    showNotification(`Reorder failed: ${e}`, 'error');
  }
}

// ==================== Import from Version ====================

async function showImportVersionDialog() {
  if (!config?.install_path || !activeScVersion) return;

  // Check if dialog already open
  if (document.getElementById('import-version-dialog')) return;

  try {
    const versions = await invoke('list_importable_versions', {
      gp: config.install_path,
      targetVersion: activeScVersion,
    });

    if (versions.length === 0) {
      showNotification('No other versions with importable data found.', 'info');
      return;
    }

    // Build dialog HTML
    const dialog = document.createElement('div');
    dialog.id = 'import-version-dialog';
    dialog.className = 'import-version-dialog';
    dialog.innerHTML = `
      <div class="import-version-dialog-header">
        <h4>Import from Version</h4>
      </div>
      <div class="import-version-dialog-body">
        <label class="import-version-label">Source version:</label>
        <select class="input import-version-select" id="import-source-select">
          ${versions.map(v => `<option value="${escapeHtml(v.version)}" data-info='${escapeHtml(JSON.stringify(v))}'>${escapeHtml(v.version)}</option>`).join('')}
        </select>
        <label class="import-version-label" style="margin-top: 8px;">Source:</label>
        <select class="input import-version-select" id="import-profile-select">
          <option value="__current__">Current SC files</option>
        </select>
        <div class="import-version-summary" id="import-version-summary"></div>
      </div>
      <div class="import-version-dialog-footer">
        <button class="btn btn-sm" id="btn-import-cancel">Cancel</button>
        <button class="btn btn-sm btn-primary" id="btn-import-confirm">Import</button>
      </div>
    `;

    // Insert dialog after section header
    const section = document.querySelector('.sc-section');
    if (section) {
      section.parentElement.insertBefore(dialog, section);
    } else {
      document.querySelector('.profile-tab-content')?.prepend(dialog);
    }

    // Load saved profiles for the selected source version
    async function loadSourceProfiles(sourceVersion) {
      const profileSelect = document.getElementById('import-profile-select');
      if (!profileSelect) return;
      profileSelect.innerHTML = '<option value="__current__">Current SC files</option>';
      try {
        const backups = await invoke('list_backups', { v: sourceVersion });
        for (const b of backups) {
          const label = b.label || b.id;
          const date = b.created_at ? ` (${b.created_at})` : '';
          const opt = document.createElement('option');
          opt.value = b.id;
          opt.textContent = `Saved: ${label}${date}`;
          profileSelect.appendChild(opt);
        }
      } catch (e) {
        debugLog('profiles', 'warn', `Failed to load backups for ${sourceVersion}: ${e}`);
      }
    }

    // Update summary for selected version
    function updateSummary() {
      const sel = document.getElementById('import-source-select');
      const opt = sel?.selectedOptions[0];
      const summaryEl = document.getElementById('import-version-summary');
      if (!opt || !summaryEl) return;
      const profileSel = document.getElementById('import-profile-select');
      const selectedProfile = profileSel?.value;
      if (selectedProfile && selectedProfile !== '__current__') {
        summaryEl.textContent = `Will create a new saved profile in ${activeScVersion}`;
        return;
      }
      try {
        const info = JSON.parse(opt.dataset.info);
        const parts = [];
        if (info.profile_file_count > 0) parts.push(`${info.profile_file_count} profile file${info.profile_file_count !== 1 ? 's' : ''}`);
        if (info.controls_file_count > 0) parts.push(`${info.controls_file_count} control mapping${info.controls_file_count !== 1 ? 's' : ''}`);
        if (info.character_file_count > 0) parts.push(`${info.character_file_count} character preset${info.character_file_count !== 1 ? 's' : ''}`);
        summaryEl.textContent = parts.length > 0
          ? `Will save as new profile: ${parts.join(', ')}`
          : 'No files found';
      } catch { summaryEl.textContent = ''; }
    }

    // Initial load
    await loadSourceProfiles(versions[0].version);
    updateSummary();

    document.getElementById('import-source-select')?.addEventListener('change', async (e) => {
      await loadSourceProfiles(e.target.value);
      updateSummary();
    });

    document.getElementById('import-profile-select')?.addEventListener('change', updateSummary);

    document.getElementById('btn-import-cancel')?.addEventListener('click', () => dialog.remove());

    document.getElementById('btn-import-confirm')?.addEventListener('click', async () => {
      const sourceVersion = document.getElementById('import-source-select')?.value;
      if (!sourceVersion) return;
      const selectedProfile = document.getElementById('import-profile-select')?.value;
      const isProfile = selectedProfile && selectedProfile !== '__current__';

      dialog.remove();
      try {
        const result = await invoke('import_version_as_profile', {
          gp: config.install_path,
          sourceVersion,
          targetVersion: activeScVersion,
          bid: isProfile ? selectedProfile : null,
          label: null,
        });
        showNotification(`Profile "${escapeHtml(result.label)}" created from ${sourceVersion}. Load it to apply.`, 'success');

        // Reload backups to show the new profile
        await loadBackups();
        renderEnvironments(document.getElementById('content'));
      } catch (e) {
        showNotification(`Import failed: ${e}`, 'error');
      }
    });

  } catch (e) {
    showNotification(`Failed to load importable versions: ${e}`, 'error');
  }
}

// ==================== Data.p4k Copy Dropdown ====================

async function showDataP4kCopyDropdown(targetVersion, event) {
  event.stopPropagation();

  // Remove any existing dropdown
  document.querySelector('.data-p4k-dropdown')?.remove();

  // Find versions with Data.p4k
  const sourceVersions = scVersions.filter(v => v.has_data_p4k && v.version !== targetVersion);

  if (sourceVersions.length === 0) {
    showNotification('Keine Version mit Data.p4k zum Kopieren gefunden', 'info');
    return;
  }

  // Build dropdown
  const dropdown = document.createElement('div');
  dropdown.className = 'data-p4k-dropdown';
  dropdown.innerHTML = `
    <div class="data-p4k-dropdown-header">Data.p4k kopieren von:</div>
    ${sourceVersions.map(v => `
      <button class="data-p4k-dropdown-item" data-source="${escapeHtml(v.version)}">
        ${escapeHtml(v.version)}
      </button>
    `).join('')}
  `;

  // Position dropdown
  const btn = event.target.closest('.version-copy-btn') || event.target;
  const rect = btn.getBoundingClientRect();
  dropdown.style.position = 'fixed';
  dropdown.style.left = `${rect.right + 5}px`;
  dropdown.style.top = `${rect.top}px`;

  document.body.appendChild(dropdown);

  // Handle clicks
  dropdown.querySelectorAll('.data-p4k-dropdown-item').forEach(item => {
    item.addEventListener('click', async () => {
      const sourceVersion = item.dataset.source;
      dropdown.remove();

      // Show progress modal instead of starting copy immediately
      await showDataP4kCopyProgressModal(sourceVersion, targetVersion);
    });
  });

  // Close on outside click
  setTimeout(() => {
    document.addEventListener('click', function closeDropdown() {
      dropdown.remove();
      document.removeEventListener('click', closeDropdown);
    });
  }, 0);
}

// ==================== Data.p4k Copy Progress Modal ====================

async function showDataP4kCopyProgressModal(sourceVersion, targetVersion) {
  // Remove existing modal
  document.querySelector('#data-p4k-copy-modal')?.remove();

  // Get file size
  let sizeBytes = 0;
  try {
    sizeBytes = await invoke('get_data_p4k_size', {
      gp: config.install_path,
      version: sourceVersion
    });
  } catch (e) {
    showNotification(`Fehler: ${e}`, 'error');
    return;
  }

  const modal = document.createElement('div');
  modal.id = 'data-p4k-copy-modal';
  modal.className = 'modal-overlay';
  modal.innerHTML = `
    <div class="modal-content data-p4k-copy-modal">
      <div class="modal-header">
        <h3>Data.p4k kopieren</h3>
        <button class="modal-close" id="btn-modal-close">×</button>
      </div>
      <div class="modal-body">
        <div class="copy-progress-info">
          <p>Von <strong>${escapeHtml(sourceVersion)}</strong> nach <strong>${escapeHtml(targetVersion)}</strong></p>
          <p>Größe: <strong>${formatFileSize(sizeBytes)}</strong></p>
        </div>
        <div class="progress-bar-container" style="display: none;">
          <div class="progress-bar" id="copy-progress-bar">
            <span class="progress-bar-text" id="copy-progress-percent">0%</span>
          </div>
        </div>
        <div class="progress-stats" style="display: none;">
          <div class="speed">
            <div class="label">Geschwindigkeit</div>
            <div class="value" id="copy-speed">-</div>
          </div>
          <div class="eta">
            <div class="label">Verbleibend</div>
            <div class="value" id="copy-eta">-</div>
          </div>
        </div>
        <p class="progress-text" id="copy-progress-text" style="display: none;">
          <span id="copied-bytes">0</span> von <span id="total-bytes">${formatFileSize(sizeBytes)}</span> kopiert
        </p>
      </div>
      <div class="modal-footer">
        <button class="btn btn-secondary" id="btn-copy-cancel">Abbrechen</button>
        <button class="btn btn-primary" id="btn-copy-start">Start</button>
      </div>
    </div>
  `;

  document.body.appendChild(modal);

  const progressBar = modal.querySelector('#copy-progress-bar');
  const progressPercent = modal.querySelector('#copy-progress-percent');
  const progressText = modal.querySelector('.progress-text');
  const progressStats = modal.querySelector('.progress-stats');
  const progressContainer = modal.querySelector('.progress-bar-container');
  const speedEl = modal.querySelector('#copy-speed');
  const etaEl = modal.querySelector('#copy-eta');
  const copiedBytesEl = modal.querySelector('#copied-bytes');
  const totalBytesEl = modal.querySelector('#total-bytes');

  // State
  let unlisten = null;

  // Helper to calculate ETA
  function calculateEta(speedBps, currentCopied) {
    if (speedBps < 1024 * 1024) return '-'; // Less than 1 MB/s
    const remaining = sizeBytes - currentCopied;
    const seconds = remaining / speedBps;
    if (seconds < 60) return '< 1 Min';
    const mins = Math.ceil(seconds / 60);
    if (mins < 60) return `~${mins} Min`;
    const hours = Math.floor(mins / 60);
    const remainingMins = mins % 60;
    return `~${hours}h ${remainingMins}m`;
  }

  let currentCopied = 0;

  // Close handlers
  const closeModal = async () => {
    if (unlisten) {
      unlisten();
      unlisten = null;
    }
    // If copying, send cancel
    try {
      await invoke('abort_copy_data_p4k', {
        gp: config.install_path,
        version: targetVersion
      });
    } catch (e) { /* ignore */ }
    modal.remove();
    copyingVersion = null;
    // Reload to reset state
    scVersions = await invoke('detect_sc_versions', { gp: config.install_path });
    renderEnvironments(document.getElementById('content'));
  };

  modal.querySelector('#btn-modal-close').addEventListener('click', closeModal);
  modal.querySelector('#btn-copy-cancel').addEventListener('click', closeModal);

  // Start button
  modal.querySelector('#btn-copy-start').addEventListener('click', async () => {
    // Switch to progress mode
    modal.querySelector('#btn-copy-start').style.display = 'none';
    modal.querySelector('#btn-copy-cancel').textContent = 'Abbrechen';
    progressContainer.style.display = 'block';
    progressText.style.display = 'block';
    progressStats.style.display = 'flex';

    // Setup progress listener
    unlisten = await listen('data-p4k-progress', (event) => {
      console.log('[DEBUG] Progress event received:', event.payload);
      const { version, percent, copied_bytes, total_bytes, speed_bps } = event.payload;

      // Only handle our target version
      if (version !== targetVersion) return;

      currentCopied = copied_bytes;

      progressBar.style.width = percent + '%';
      progressPercent.textContent = percent + '%';
      copiedBytesEl.textContent = formatFileSize(copied_bytes);

      if (speed_bps > 0) {
        const speedMB = (speed_bps / (1024 * 1024)).toFixed(1);
        speedEl.textContent = speedMB + ' MB/s';
        etaEl.textContent = calculateEta(speed_bps, copied_bytes);
      }
    });

    // Set copying state
    copyingVersion = { version: targetVersion, startTime: Date.now() };

    // Reload to show yellow "copying" state
    renderEnvironments(document.getElementById('content'));

    try {
      await invoke('copy_data_p4k', {
        gp: config.install_path,
        sourceVersion,
        targetVersion
      });

      // Success
      showNotification(`Data.p4k erfolgreich kopiert!`, 'success');
      if (unlisten) {
        unlisten();
        unlisten = null;
      }
      modal.remove();

      // Reload versions
      scVersions = await invoke('detect_sc_versions', { gp: config.install_path });
      renderEnvironments(document.getElementById('content'));

    } catch (e) {
      if (e.includes('cancelled') || e.includes('abgebrochen')) {
        showNotification('Kopieren abgebrochen', 'info');
      } else {
        showNotification(`Fehler: ${e}`, 'error');
      }
      if (unlisten) {
        unlisten();
        unlisten = null;
      }
      modal.remove();

      // Reload versions
      scVersions = await invoke('detect_sc_versions', { gp: config.install_path });
      renderEnvironments(document.getElementById('content'));
    }

    copyingVersion = null;
  });
}

// ==================== Event Listeners ====================

function attachProfilesEventListeners() {
  // Tab navigation
  document.querySelectorAll('.profile-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      activeProfileTab = tab.dataset.tab;
      renderEnvironments(document.getElementById('content'));
    });
  });

  // Dismiss hint buttons
  document.querySelectorAll('[data-action="dismiss-hint"]').forEach(btn => {
    btn.addEventListener('click', () => dismissHint(btn.dataset.hintId));
  });

  // Collapsible panel toggles
  document.querySelectorAll('.collapsible-header').forEach(header => {
    header.addEventListener('click', () => {
      const panel = header.dataset.panel;
      window.expandedPanels[panel] = !window.expandedPanels[panel];
      const toggle = header.querySelector('.collapsible-toggle');
      const content = header.nextElementSibling;
      if (toggle) toggle.classList.toggle('collapsed');
      if (content) content.classList.toggle('collapsed');
    });
  });

  // Version Cards
  document.querySelectorAll('.sc-version-card').forEach(card => {
    card.addEventListener('click', async () => {
      // Save active profile for current version before switching
      if (activeScVersion && lastRestoredBackupId) {
        lastRestoredPerVersion[activeScVersion] = lastRestoredBackupId;
      }
      activeScVersion = card.dataset.version;
      // Restore active profile for the new version
      lastRestoredBackupId = lastRestoredPerVersion[activeScVersion] || null;
      selectedBindingSource = null;
      bindingFilter = '';
      bindingCategory = 'all';
      await Promise.all([
        loadDevicesAndBindings(),
        loadCompleteBindingList(),
        loadExportedLayouts(),
        loadBackups(),
        loadUserCfgSettings(),
        loadLocalizationData(),
      ]);
      await loadProfileStatus();
      renderEnvironments(document.getElementById('content'));
    });
  });

  // Empty Version State actions
  document.getElementById('btn-create-version')?.addEventListener('click', async (e) => {
    const version = e.target.dataset.version;
    await createScVersion(version);
  });
  
  document.getElementById('btn-link-p4k')?.addEventListener('click', async (e) => {
    const version = e.target.dataset.version;
    const source = document.getElementById('data-source-select').value;
    await linkDataP4k(source, version);
  });
  
  document.getElementById('btn-copy-p4k')?.addEventListener('click', async (e) => {
    const version = e.target.dataset.version;
    const source = document.getElementById('data-source-select').value;
    startCopyDataP4k(source, version);
  });

  // Device drag-and-drop (pointer events - works in WebKitGTK)
  document.querySelectorAll('.device-drag-handle').forEach(handle => {
    handle.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      const card = handle.closest('.device-card');
      if (!card || !card.dataset.instance) return;

      const sourceInstance = parseInt(card.dataset.instance, 10);
      const sourceDeviceType = card.dataset.deviceType || 'joystick';
      const rect = card.getBoundingClientRect();
      const offsetY = e.clientY - rect.top;

      // Create floating clone
      const clone = card.cloneNode(true);
      clone.classList.add('drag-clone');
      clone.style.cssText = `position:fixed;width:${rect.width}px;top:${e.clientY - offsetY}px;left:${rect.left}px;z-index:1000;pointer-events:none;`;
      document.body.appendChild(clone);
      card.classList.add('dragging');

      function onMove(ev) {
        clone.style.top = (ev.clientY - offsetY) + 'px';
        // Highlight drop target
        document.querySelectorAll('.device-card.draggable').forEach(c => {
          if (c === card) return;
          const r = c.getBoundingClientRect();
          if (ev.clientY >= r.top && ev.clientY <= r.bottom) {
            c.classList.add('drag-over');
          } else {
            c.classList.remove('drag-over');
          }
        });
      }

      function onUp(ev) {
        document.removeEventListener('pointermove', onMove);
        document.removeEventListener('pointerup', onUp);
        clone.remove();
        card.classList.remove('dragging');

        // Find drop target
        let targetInstance = null;
        let targetDeviceType = 'joystick';
        document.querySelectorAll('.device-card.draggable').forEach(c => {
          if (c.classList.contains('drag-over')) {
            targetInstance = parseInt(c.dataset.instance, 10);
            targetDeviceType = c.dataset.deviceType || 'joystick';
            c.classList.remove('drag-over');
          }
        });

        if (targetInstance !== null && targetInstance !== sourceInstance) {
          handleDeviceDrop(sourceInstance, targetInstance, sourceDeviceType, targetDeviceType);
        }
      }

      document.addEventListener('pointermove', onMove);
      document.addEventListener('pointerup', onUp);
    });
  });

  // Reload profile from disk
  document.getElementById('btn-reload-profile')?.addEventListener('click', async () => {
    await Promise.all([loadDevicesAndBindings(), loadCompleteBindingList(), loadExportedLayouts()]);
    renderEnvironments(document.getElementById('content'));
    showNotification('Profile reloaded from disk', 'success');
  });

  document.getElementById('btn-update-profile')?.addEventListener('click', async () => {
    if (lastRestoredBackupId) {
      const confirmed = await confirm('Overwrite this profile with your current game settings? This cannot be undone.', {
        title: 'Update Profile',
        kind: 'warning',
      });
      if (confirmed) await updateProfileFromSc(lastRestoredBackupId);
    }
  });

  document.getElementById('btn-revert-changes')?.addEventListener('click', async () => {
    if (lastRestoredBackupId) {
      const confirmed = await confirm('Discard all local game changes and revert to the saved profile state?', {
        title: 'Revert Changes',
        kind: 'warning',
      });
      if (confirmed) await restoreProfile(lastRestoredBackupId);
    }
  });

  // Binding source select
  document.getElementById('binding-source-select')?.addEventListener('change', async (e) => {
    selectedBindingSource = e.target.value || null;
    await loadDevicesAndBindings();
    await loadCompleteBindingList();
    renderEnvironments(document.getElementById('content'));
  });

  // Binding search - searches across all fields
  document.getElementById('binding-search')?.addEventListener('input', (e) => {
    const term = e.target.value.toLowerCase().trim();
    bindingFilter = term;

    const categoryBlocks = document.querySelectorAll('.binding-category-block');
    categoryBlocks.forEach(block => {
      const rows = block.querySelectorAll('.binding-row');
      let hasVisibleRow = false;

      rows.forEach(row => {
        // Search in all data attributes including device name and input display
        const searchIn = [
          row.dataset.actionName,
          row.dataset.displayName,
          row.dataset.input,
          row.dataset.deviceName,
          row.dataset.inputDisplay
        ].filter(Boolean).join(' ').toLowerCase();

        const matches = !term || searchIn.includes(term);
        row.style.display = matches ? '' : 'none';
        if (matches) hasVisibleRow = true;
      });
      
      block.style.display = hasVisibleRow ? '' : 'none';
      if (term.length > 1 && hasVisibleRow) {
        block.classList.add('expanded');
      }
    });
  });

  // Category pills
  document.querySelectorAll('.binding-category-pills .source-tab').forEach(pill => {
    pill.addEventListener('click', () => {
      bindingCategory = pill.dataset.category;
      renderEnvironments(document.getElementById('content'));
    });
  });

  // Category more dropdown
  document.getElementById('binding-category-more')?.addEventListener('change', (e) => {
    if (e.target.value) {
      bindingCategory = e.target.value;
      renderEnvironments(document.getElementById('content'));
    }
  });

  // Add binding button
  document.querySelectorAll('[data-action="add-binding"]').forEach(btn => {
    btn.addEventListener('click', () => {
      const actionName = btn.dataset.actionName;
      const category = btn.dataset.category;
      openBindingEditor(actionName, category, null);
    });
  });

  // Edit binding button
  document.querySelectorAll('[data-action="edit-binding"]').forEach(btn => {
    btn.addEventListener('click', () => {
      const actionName = btn.dataset.actionName;
      const category = btn.dataset.category;
      const currentInput = btn.dataset.input || '';
      openBindingEditor(actionName, category, currentInput);
    });
  });

  // Remove binding button - removes from profile's actionmaps.xml
  document.querySelectorAll('[data-action="remove-binding"], [data-action="remove-binding-direct"]').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const actionName = btn.dataset.actionName;
      const category = btn.dataset.category;

      if (!lastRestoredBackupId) {
        showNotification('No profile loaded.', 'error');
        return;
      }

      const confirmed = await confirm(`Remove binding for "${actionName}" from your profile?`, {
        title: 'Remove Binding',
        kind: 'warning',
      });
      if (confirmed) {
        try {
          await invoke('remove_profile_binding', {
            v: activeScVersion,
            profileId: lastRestoredBackupId,
            actionMap: category,
            actionName: actionName,
          });

          showNotification('Binding removed from profile', 'success');
          await loadBackups();
          await loadCompleteBindingList();
          refreshBindingsInPlace();
        } catch (e) {
          showNotification(`Failed to remove binding: ${e}`, 'error');
        }
      }
    });
  });

  // Save / Load / Delete profiles
  document.getElementById('btn-save-current')?.addEventListener('click', saveProfile);
  document.getElementById('btn-save-first-profile')?.addEventListener('click', saveProfile);

  document.querySelectorAll('[data-action="load-profile"]').forEach(btn => {
    btn.addEventListener('click', () => loadProfile(btn.dataset.backupId));
  });

  document.querySelectorAll('[data-action="delete-saved-profile"]').forEach(btn => {
    btn.addEventListener('click', (e) => { e.stopPropagation(); deleteProfile(btn.dataset.backupId); });
  });

  // Storage tab actions
  document.getElementById('btn-delete-version')?.addEventListener('click', async (e) => {
    const version = e.target.closest('button').dataset.version;
    await deleteScVersion(version);
  });

  // Toggle changes detail panel
  document.getElementById('btn-toggle-changes')?.addEventListener('click', () => {
    showChangesPanel = !showChangesPanel;
    renderEnvironments(document.getElementById('content'));
  });

  // Apply to SC button
  document.getElementById('btn-apply-to-sc')?.addEventListener('click', async () => {
    if (!config?.install_path || !activeScVersion || !lastRestoredBackupId) return;
    try {
      await invoke('apply_profile_to_sc', {
        gp: config.install_path,
        v: activeScVersion,
        profileId: lastRestoredBackupId,
      });
      showNotification('Profile applied to Star Citizen', 'success');
      await loadBackups();
      await loadProfileStatus();
      renderEnvironments(document.getElementById('content'));
    } catch (e) {
      showNotification(`Apply failed: ${e}`, 'error');
    }
  });

  // Customized only toggle
  document.getElementById('customized-only-toggle')?.addEventListener('change', (e) => {
    customizedOnly = e.target.checked;
    refreshBindingsInPlace();
  });

  // Human-readable toggle
  document.getElementById('use-human-readable')?.addEventListener('change', (e) => {
    useHumanReadable = e.target.checked;
    refreshBindingsInPlace();
  });

  // Device alias buttons
  document.querySelectorAll('.device-map-alias-btn').forEach(btn => {
    btn.addEventListener('click', async () => {
      const productName = btn.dataset.product;
      const currentAlias = btn.dataset.alias || '';
      const newAlias = prompt(`Alias for "${productName}":`, currentAlias);
      if (newAlias === null) return; // cancelled
      try {
        await invoke('set_profile_device_alias', {
          v: activeScVersion,
          profileId: lastRestoredBackupId,
          productName,
          alias: newAlias,
        });
        await loadBackups();
        renderEnvironments(document.getElementById('content'));
      } catch (e) {
        showNotification(`Failed to set alias: ${e}`, 'error');
      }
    });
  });

  // Rename saved profile — click edit icon to show inline input
  document.querySelectorAll('[data-action="rename-saved-profile"]').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation(); // prevent card click
      const backupId = btn.dataset.backupId;
      const wrap = btn.closest('.profile-card-header') || btn.closest('.backup-main');
      if (!wrap || wrap.querySelector('.backup-rename-input')) return;

      const labelEl = wrap.querySelector('.profile-card-name') || wrap.querySelector('.backup-label-display');
      const backup = backups.find(b => b.id === backupId);
      const currentLabel = backup?.label || '';

      // Hide label and icon, show input
      labelEl.style.display = 'none';
      btn.style.display = 'none';

      const input = document.createElement('input');
      input.type = 'text';
      input.className = 'input backup-rename-input';
      input.value = currentLabel;
      input.placeholder = 'Profile name';
      input.maxLength = 60;
      wrap.appendChild(input);
      input.focus();
      input.select();

      async function saveRename() {
        const newLabel = input.value.trim();
        input.remove();
        labelEl.style.display = '';
        btn.style.display = '';
        labelEl.textContent = newLabel || 'Unnamed profile';

        if (backup) backup.label = newLabel;

        try {
          await invoke('update_backup_label', {
            v: activeScVersion,
            bid: backupId,
            l: newLabel,
          });
        } catch (e) {
          showNotification(`Failed to rename: ${e}`, 'error');
        }
        // Update header if this is the active profile
        if (lastRestoredBackupId === backupId) {
          renderEnvironments(document.getElementById('content'));
        }
      }

      input.addEventListener('blur', saveRename);
      input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') { e.preventDefault(); input.blur(); }
        if (e.key === 'Escape') { input.value = currentLabel; input.blur(); }
      });
    });
  });

  // Import from version
  document.getElementById('btn-import-version')?.addEventListener('click', showImportVersionDialog);
  document.getElementById('btn-import-banner')?.addEventListener('click', showImportVersionDialog);
  document.getElementById('btn-import-banner-dismiss')?.addEventListener('click', () => {
    document.getElementById('import-banner')?.remove();
  });

  // Data.p4k Copy Button
  document.querySelectorAll('.version-copy-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const targetVersion = btn.dataset.version;
      showDataP4kCopyDropdown(targetVersion, e);
    });
  });

  // USER.cfg
  document.getElementById('btn-apply-usercfg')?.addEventListener('click', applyUserCfg);
  document.getElementById('btn-reset-usercfg')?.addEventListener('click', resetUserCfg);

  // Accordion toggle for collapsible categories
  document.querySelectorAll('.usercfg-category-header.collapsible').forEach(header => {
    header.addEventListener('click', () => {
      const catKey = header.dataset.categoryKey;
      if (collapsedCategories.has(catKey)) {
        collapsedCategories.delete(catKey);
      } else {
        collapsedCategories.add(catKey);
      }
      const settingsDiv = header.nextElementSibling;
      const toggleIcon = header.querySelector('.usercfg-category-toggle');
      if (settingsDiv) settingsDiv.classList.toggle('collapsed');
      if (toggleIcon) toggleIcon.classList.toggle('collapsed');
    });
  });

  // Slider changes
  document.querySelectorAll('.usercfg-slider').forEach(slider => {
    slider.addEventListener('input', (e) => {
      const key = e.target.dataset.key;
      const value = parseFloat(e.target.value);
      const setting = DEFAULT_SETTINGS[key];

      let displayValue = value;
      if (setting.labels) displayValue = setting.labels[value] || value;
      else if (setting.category === 'quality') displayValue = QUALITY_LEVELS[value] || value;
      else if (setting.category === 'shaders') displayValue = SHADER_LEVELS[value] || value;

      e.target.parentElement.querySelector('.usercfg-value').textContent = displayValue;
      userCfgSettings[key] = value;
      updateSettingHighlight(e.target.closest('.usercfg-row'), key, setting, value);
    });
  });

  // Number input changes
  document.querySelectorAll('.usercfg-number-input').forEach(input => {
    input.addEventListener('change', (e) => {
      const key = e.target.dataset.key;
      const value = parseFloat(e.target.value);
      const setting = DEFAULT_SETTINGS[key];
      userCfgSettings[key] = value;
      updateSettingHighlight(e.target.closest('.usercfg-row'), key, setting, value);
    });
  });

  // Checkbox changes
  document.querySelectorAll('.usercfg-input[type="checkbox"]').forEach(checkbox => {
    checkbox.addEventListener('change', (e) => {
      const key = e.target.dataset.key;
      const value = e.target.checked ? 1 : 0;
      const setting = DEFAULT_SETTINGS[key];
      userCfgSettings[key] = value;
      updateSettingHighlight(e.target.closest('.usercfg-row'), key, setting, value);
    });
  });

  // Localization: install language buttons
  document.querySelectorAll('[data-action="install-lang"]').forEach(btn => {
    btn.addEventListener('click', () => {
      installLocalization(
        btn.dataset.langCode,
        btn.dataset.sourceRepo,
        btn.dataset.langName,
        btn.dataset.sourceLabel,
      );
    });
  });

  // Localization: update button
  document.getElementById('btn-update-localization')?.addEventListener('click', () => {
    const repo = resolveSourceRepo();
    if (repo && localizationStatus) {
      installLocalization(
        localizationStatus.language_code,
        repo,
        localizationStatus.language_name || localizationStatus.language_code,
        localizationStatus.source_label || '',
      );
    }
  });

  // Localization: remove button
  document.getElementById('btn-remove-localization')?.addEventListener('click', removeLocalization);

  // Reset individual setting to default (event delegation)
  document.querySelector('.usercfg-section')?.addEventListener('click', (e) => {
    const btn = e.target.closest('.usercfg-reset');
    if (!btn) return;

    const key = btn.dataset.key;
    const setting = DEFAULT_SETTINGS[key];
    if (!setting) return;

    delete userCfgSettings[key];

    const row = btn.closest('.usercfg-row');
    if (!row) return;

    // Update the control value in the DOM
    const slider = row.querySelector('.usercfg-slider');
    const numberInput = row.querySelector('.usercfg-number-input');
    const checkbox = row.querySelector('.usercfg-input[type="checkbox"]');

    if (slider) {
      slider.value = setting.value;
      const valueSpan = row.querySelector('.usercfg-value');
      if (valueSpan) {
        let display = setting.value;
        if (setting.labels) display = setting.labels[setting.value] || setting.value;
        else if (setting.category === 'quality') display = QUALITY_LEVELS[setting.value] || setting.value;
        else if (setting.category === 'shaders') display = SHADER_LEVELS[setting.value] || setting.value;
        valueSpan.textContent = display;
      }
    } else if (numberInput) {
      numberInput.value = setting.value;
    } else if (checkbox) {
      checkbox.checked = !!setting.value;
    }

    updateSettingHighlight(row, key, setting, setting.value);
  });

  // Data.p4k copy progress listener (listen is already imported at top)
  listen('data-p4k-progress', (event) => {
    const { version, percent, copied, total } = event.payload;
    // Update progress bar if this version is being copied
    const progressEl = document.querySelector(`.version-copy-progress[data-version="${version}"]`);
    if (progressEl) {
      progressEl.style.width = `${percent}%`;
      progressEl.textContent = `${percent}%`;
    }
  });

  listen('data-p4k-copy-complete', async (event) => {
    const { version, success } = event.payload;
    if (success) {
      showNotification(`Data.p4k für ${version} kopiert!`, 'success');
    }
    copyingVersion = null;
    // Reload versions
    scVersions = await invoke('detect_sc_versions', { gp: config.install_path });
    renderEnvironments(document.getElementById('content'));
  });
}

function updateSettingHighlight(row, key, setting, value) {
  const isChanged = value !== setting.value;
  if (!row) return;

  const controlWrap = row.querySelector('.usercfg-control-wrap');

  if (isChanged) {
    row.classList.add('usercfg-changed');
    const label = row.querySelector('.usercfg-label');
    const defaultLabel = setting.type === 'toggle'
      ? (setting.value ? 'On' : 'Off')
      : (setting.labels
        ? (setting.labels[setting.value] || setting.value)
        : (setting.category === 'quality'
          ? (QUALITY_LEVELS[setting.value] || setting.value)
          : (setting.category === 'shaders'
            ? (SHADER_LEVELS[setting.value] || setting.value)
            : setting.value)));
    if (!label.querySelector('.usercfg-default')) {
      label.innerHTML = `${setting.label} <span class="usercfg-default">(Default: ${defaultLabel})</span>`;
    }
    // Add reset button if not present
    if (controlWrap && !controlWrap.querySelector('.usercfg-reset')) {
      const btn = document.createElement('button');
      btn.className = 'usercfg-reset';
      btn.dataset.key = key;
      btn.title = 'Reset to default';
      btn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="1 4 1 10 7 10"></polyline><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"></path></svg>';
      controlWrap.appendChild(btn);
    }
  } else {
    row.classList.remove('usercfg-changed');
    const label = row.querySelector('.usercfg-label');
    const defaultSpan = label.querySelector('.usercfg-default');
    if (defaultSpan) defaultSpan.remove();
    // Remove reset button
    const resetBtn = controlWrap?.querySelector('.usercfg-reset');
    if (resetBtn) resetBtn.remove();
  }

  // Update category badge and header count
  updateChangedCounts();
}

function updateChangedCounts() {
  // Update each category badge
  document.querySelectorAll('.usercfg-category').forEach(cat => {
    const header = cat.querySelector('.usercfg-category-header');
    if (!header) return;
    const catKey = header.dataset?.categoryKey;
    const changedInCat = cat.querySelectorAll('.usercfg-row.usercfg-changed').length;
    let badge = header.querySelector('.usercfg-category-badge');
    if (changedInCat > 0) {
      if (!badge) {
        badge = document.createElement('span');
        badge.className = 'usercfg-category-badge';
        const label = header.querySelector('.usercfg-category-label');
        if (label) label.after(badge);
      }
      badge.textContent = `${changedInCat} changed`;
    } else if (badge) {
      badge.remove();
    }
  });

  // Update header total count
  const totalChanged = getChangedSettingsCount();
  const headerCount = document.querySelector('.usercfg-header-count');
  if (headerCount) {
    headerCount.textContent = totalChanged > 0 ? `${totalChanged} changed` : 'All defaults';
  }

  // Update unsaved indicator
  const unsavedEl = document.getElementById('usercfg-unsaved');
  if (unsavedEl) {
    const hasUnsaved = hasUnsavedChanges();
    unsavedEl.style.display = hasUnsaved ? '' : 'none';
  }
}

function hasUnsavedChanges() {
  // Compare current settings against saved snapshot
  const allKeys = new Set([
    ...Object.keys(userCfgSettings),
    ...Object.keys(savedUserCfgSnapshot),
  ]);
  for (const key of allKeys) {
    const current = userCfgSettings[key];
    const saved = savedUserCfgSnapshot[key];
    if (current !== saved) return true;
  }
  return false;
}

// ==================== Utilities ====================


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

// ==================== App Close Blocker ====================

async function initCloseBlocker() {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const appWindow = getCurrentWindow();

    appWindow.onCloseRequested(async (event) => {
      if (copyingVersion) {
        // Prevent close
        event.preventDefault();

        // Show confirmation dialog
        const confirmed = confirm(
          `Data.p4k wird noch kopiert für ${copyingVersion.version}.\n\n` +
          `Möchten Sie wirklich schließen? Die kopierten Daten werden gelöscht.`
        );

        if (confirmed) {
          // Abort copy and delete partial file
          try {
            await invoke('abort_copy_data_p4k', {
              gp: config.install_path,
              version: copyingVersion.version
            });
            showNotification('Kopieren abgebrochen und Datei gelöscht.', 'info');
          } catch (e) {
            console.error('Failed to abort copy:', e);
          }

          copyingVersion = null;

          // Reload and allow close
          scVersions = await invoke('detect_sc_versions', { gp: config.install_path });
          renderEnvironments(document.getElementById('content'));

          // Now close
          await appWindow.close();
        } else {
          // User clicked "Cancel" - just continue
          copyingVersion = null;
        }
      }
    });
  } catch (e) {
    console.warn('Close blocker not available:', e);
  }
}

// Initialize close blocker when profiles module loads
initCloseBlocker();

async function createScVersion(version) {
  if (!version || !config?.install_path) return;
  
  try {
    showNotification(`Creating folder for ${version}...`, 'info');
    await invoke('create_sc_version', { gp: config.install_path, version });
    showNotification(`Version ${version} folder created successfully.`, 'success');
    
    // Reload environments
    scVersions = await invoke('detect_sc_versions', { gp: config.install_path });
    renderEnvironments(document.getElementById('content'));
  } catch (err) {
    showNotification(`Failed to create version: ${err}`, 'error');
  }
}

async function linkDataP4k(sourceVersion, targetVersion) {
  if (!sourceVersion || !targetVersion || !config?.install_path) return;
  
  try {
    showNotification(`Symlinking Data.p4k from ${sourceVersion} to ${targetVersion}...`, 'info');
    await invoke('link_data_p4k', { gp: config.install_path, src_version: sourceVersion, dst_version: targetVersion });
    showNotification(`Data.p4k symlinked successfully.`, 'success');
    
    // Reload environments
    scVersions = await invoke('detect_sc_versions', { gp: config.install_path });
    renderEnvironments(document.getElementById('content'));
  } catch (err) {
    showNotification(`Failed to symlink Data.p4k: ${err}`, 'error');
  }
}

async function updateProfileFromSc(backupId) {
  if (!backupId || !activeScVersion || !config?.install_path) return;
  
  try {
    showNotification('Updating profile from current game files...', 'info');
    await invoke('update_backup_from_sc', { gp: config.install_path, v: activeScVersion, bid: backupId });
    showNotification('Profile updated successfully.', 'success');
    
    // Refresh UI
    await loadBackups();
    await loadProfileStatus();
    renderEnvironments(document.getElementById('content'));
  } catch (err) {
    showNotification(`Failed to update profile: ${err}`, 'error');
  }
}
