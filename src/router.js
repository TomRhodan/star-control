/**
 * Star Control - Router Module
 *
 * This module handles client-side routing for the application.
 * It manages page navigation, sidebar visibility based on installation state,
 * and the initial setup flow for first-time users.
 *
 * @module router
 */

import { invoke } from '@tauri-apps/api/core';
import { renderDashboard } from './pages/dashboard.js';
import { renderInstallation } from './pages/installation.js';
import { renderRunners } from './pages/runners.js';
import { renderLaunch } from './pages/launch.js';
import { renderProfiles } from './pages/profiles.js';
import { renderSettings } from './pages/settings.js';
import { renderAbout } from './pages/about.js';
import { renderSetup } from './pages/setup.js';

/**
 * Route mapping from page name to render function.
 * @constant {Object<string, function>}
 */
const routes = {
  dashboard: renderDashboard,
  installation: renderInstallation,
  runners: renderRunners,
  launch: renderLaunch,
  profiles: renderProfiles,
  settings: renderSettings,
  about: renderAbout,
};

// Pages visible when NO instance is installed
const PRE_INSTALL_PAGES = ['dashboard', 'installation', 'settings'];

// Pages visible when an instance IS installed
const POST_INSTALL_PAGES = ['dashboard', 'launch', 'runners', 'profiles', 'settings'];

let setupActive = false;
let installed = false;

/**
 * Navigates to a specific page.
 * @param {string} page - The page name to navigate to.
 */
function navigate(page) {
  if (setupActive) return;

  const content = document.getElementById('content');
  const renderFn = routes[page];
  if (!renderFn) return;

  content.innerHTML = '';
  renderFn(content);

  document.querySelectorAll('.nav-link').forEach((link) => {
    link.classList.toggle('active', link.dataset.page === page);
  });
}

/**
 * Updates the sidebar visibility based on installation state.
 * @param {boolean} isInstalled - Whether Star Citizen is installed.
 */
function updateSidebar(isInstalled) {
  installed = isInstalled;
  const visiblePages = isInstalled ? POST_INSTALL_PAGES : PRE_INSTALL_PAGES;

  // Nav links in main list
  document.querySelectorAll('.nav-links .nav-link').forEach((link) => {
    const page = link.dataset.page;
    if (!page) return;
    link.closest('li').style.display = visiblePages.includes(page) ? '' : 'none';
  });

  // Footer links (about) — always visible
  document.querySelectorAll('.sidebar-footer .nav-link').forEach((link) => {
    link.style.display = '';
  });
}

/**
 * Checks if Star Citizen is installed by loading config and checking installation status.
 * @returns {Promise<boolean>} True if installed, false otherwise.
 */
async function checkInstallationState() {
  try {
    const config = await invoke('load_config');
    if (config) {
      const status = await invoke('check_installation', { config });
      return status.installed;
    }
  } catch (e) {
    // no config or check failed
  }
  return false;
}

/**
 * Shows the initial setup wizard.
 * @param {string} defaultPath - Default installation path suggestion.
 */
function showSetup(defaultPath) {
  setupActive = true;

  const sidebar = document.getElementById('sidebar');
  const content = document.getElementById('content');

  sidebar.classList.add('sidebar-hidden');
  content.classList.add('content-setup');
  content.innerHTML = '';

  renderSetup(content, {
    defaultPath,
    onComplete: () => {
      setupActive = false;
      sidebar.classList.remove('sidebar-hidden');
      content.classList.remove('content-setup');
      updateSidebar(false);
      navigate('installation');
    },
  });
}

/**
 * Initializes the router and application state.
 * Sets up navigation handlers, checks for setup requirements, and loads the initial page.
 */
async function init() {
  document.querySelectorAll('.nav-link').forEach((link) => {
    link.addEventListener('click', (e) => {
      e.preventDefault();
      navigate(link.dataset.page);
    });
  });

  // Check if first-time setup is needed
  try {
    const check = await invoke('check_needs_setup');
    if (check.needs_setup) {
      showSetup(check.default_path);
      return;
    }
  } catch (err) {
    console.error('Setup check failed:', err);
  }

  // Determine installation state and update sidebar
  const isInstalled = await checkInstallationState();
  updateSidebar(isInstalled);

  navigate('dashboard');
}

export const router = { init, navigate, updateSidebar };
