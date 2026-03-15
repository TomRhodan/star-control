/**
 * Star Control - Main Entry Point
 *
 * This is the central JavaScript entry point of the Star Control application.
 * It sets up window management, router initialization, and global UI components
 * (e.g. tooltips, title bar buttons).
 *
 * @module main
 */

// Tauri APIs for window management, external URL opening, and app info
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { router } from './router.js';

// Reference to the current Tauri window for minimize/maximize/close
const appWindow = getCurrentWindow();

/**
 * Automatic window size adjustment on startup.
 * Adjusts the window size to fit the screen so that the window does not
 * extend beyond the edges on small monitors. Target size: 1280x900, but
 * at most 85% of the screen size. Only runs if the screen is smaller
 * than the default size.
 */
(async () => {
  try {
    // Account for device pixel ratio (e.g. HiDPI/Retina displays)
    const factor = window.devicePixelRatio || 1;
    const screenWidth = window.screen.width * factor;
    const screenHeight = window.screen.height * factor;

    // Maximum window size: 85% of screen
    const maxWidth = Math.floor(screenWidth * 0.85);
    const maxHeight = Math.floor(screenHeight * 0.85);

    // Target size: the minimum of default size and maximum allowed size
    const targetWidth = Math.min(1280, maxWidth);
    const targetHeight = Math.min(900, maxHeight);

    // Only resize if the screen is smaller than the default size
    if (screenWidth < 1280 || screenHeight < 900) {
      await appWindow.setSize({ type: 'Logical', width: targetWidth, height: targetHeight });
    }
  } catch (e) {
    // Errors during size adjustment are non-critical and can be ignored
  }
})();

// === Title bar buttons (custom window controls) ===
// Since Tauri uses a frameless window, minimize/maximize/close are handled
// via custom HTML buttons in the title bar.

document.getElementById('btn-minimize').addEventListener('click', () => {
  appWindow.minimize();
});

document.getElementById('btn-maximize').addEventListener('click', () => {
  appWindow.toggleMaximize();
});

document.getElementById('btn-close').addEventListener('click', () => {
  appWindow.close();
});

// Wiki link: Opens the Star Citizen LUG Wiki page in the default browser
document.getElementById('link-wiki').addEventListener('click', (e) => {
  e.preventDefault();
  invoke('open_browser', { url: 'https://wiki.starcitizen-lug.org/' }).catch(err => console.error(err));
});

/**
 * Global Tooltip System
 *
 * Displays tooltips for all elements with the `data-tooltip` attribute.
 * Tooltips are positioned within the visible viewport area so they are not
 * clipped. Uses event delegation on the entire document so that dynamically
 * created elements are also supported.
 */
(function initTooltips() {
  // Currently displayed tooltip element and the associated source element
  let tip = null;
  let tipSource = null;

  /** Removes the current tooltip from the DOM */
  function removeTip() {
    if (tip) { tip.remove(); tip = null; }
    tipSource = null;
  }

  /**
   * Creates and positions a tooltip below the source element.
   * If there is no room below, it is displayed above instead.
   * Horizontally centered, but kept within the viewport bounds.
   */
  function showTip(el) {
    const text = el.getAttribute('data-tooltip');
    if (!text) return;

    tip = document.createElement('div');
    tip.className = 'tooltip-popup';
    tip.textContent = text;
    document.body.appendChild(tip);
    tipSource = el;

    // Determine position of the source element
    const rect = el.getBoundingClientRect();
    const tw = tip.offsetWidth;
    const th = tip.offsetHeight;

    // Vertical position: prefer below, fall back to above if not enough space
    let top = rect.bottom + 8;
    if (top + th > window.innerHeight - 8) {
      top = rect.top - th - 8;
    }

    // Horizontal position: centered below the element, clamped to viewport
    let left = rect.left + rect.width / 2 - tw / 2;
    left = Math.max(8, Math.min(left, window.innerWidth - tw - 8));

    tip.style.top = top + 'px';
    tip.style.left = left + 'px';
  }

  // Event delegation: responds to mouseover for all data-tooltip elements
  document.addEventListener('mouseover', (e) => {
    const el = e.target.closest('[data-tooltip]');

    // If the mouse has left the current tooltip source element, remove tooltip
    if (tipSource && tipSource !== el) {
      removeTip();
    }

    // Show new tooltip when hovering over a tooltip element and none is active
    if (el && !tip) {
      showTip(el);
    }
  });
})();

// Populate version from tauri.conf.json (single source of truth)
getVersion().then(v => {
  const el = document.querySelector('.version');
  if (el) el.textContent = 'v' + v;
});

// Initialize router - checks setup status and loads the first page
router.init();
