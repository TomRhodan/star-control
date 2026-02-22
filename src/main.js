/**
 * Star Control - Main Entry Point
 *
 * This is the main JavaScript entry point for the Star Control application.
 * It handles window management, initializes the router, and sets up global UI components.
 *
 * @module main
 */

import { getCurrentWindow } from '@tauri-apps/api/window';
import { openUrl } from '@tauri-apps/plugin-opener';
import { router } from './router.js';

const appWindow = getCurrentWindow();

// Adjust window size to fit screen (max 80% of screen, min 900x700)
(async () => {
  try {
    const factor = window.devicePixelRatio || 1;
    const screenWidth = window.screen.width * factor;
    const screenHeight = window.screen.height * factor;

    const maxWidth = Math.floor(screenWidth * 0.85);
    const maxHeight = Math.floor(screenHeight * 0.85);

    const targetWidth = Math.min(1280, maxWidth);
    const targetHeight = Math.min(900, maxHeight);

    // Only resize if needed (screen is smaller than default)
    if (screenWidth < 1280 || screenHeight < 900) {
      await appWindow.setSize({ type: 'Logical', width: targetWidth, height: targetHeight });
    }
  } catch (e) {
    // Ignore resize errors (not critical)
  }
})();

document.getElementById('btn-minimize').addEventListener('click', () => {
  appWindow.minimize();
});

document.getElementById('btn-maximize').addEventListener('click', () => {
  appWindow.toggleMaximize();
});

document.getElementById('btn-close').addEventListener('click', () => {
  appWindow.close();
});

document.getElementById('link-wiki').addEventListener('click', (e) => {
  e.preventDefault();
  openUrl('https://wiki.starcitizen-lug.org/');
});

// Global tooltip system — positions tooltips within viewport
(function initTooltips() {
  let tip = null;
  let tipSource = null;

  function removeTip() {
    if (tip) { tip.remove(); tip = null; }
    tipSource = null;
  }

  function showTip(el) {
    const text = el.getAttribute('data-tooltip');
    if (!text) return;

    tip = document.createElement('div');
    tip.className = 'tooltip-popup';
    tip.textContent = text;
    document.body.appendChild(tip);
    tipSource = el;

    const rect = el.getBoundingClientRect();
    const tw = tip.offsetWidth;
    const th = tip.offsetHeight;

    let top = rect.bottom + 8;
    if (top + th > window.innerHeight - 8) {
      top = rect.top - th - 8;
    }

    let left = rect.left + rect.width / 2 - tw / 2;
    left = Math.max(8, Math.min(left, window.innerWidth - tw - 8));

    tip.style.top = top + 'px';
    tip.style.left = left + 'px';
  }

  document.addEventListener('mouseover', (e) => {
    const el = e.target.closest('[data-tooltip]');

    // Moved away from tooltip source or to a different one
    if (tipSource && tipSource !== el) {
      removeTip();
    }

    // Show new tooltip if hovering a tooltip element and none is active
    if (el && !tip) {
      showTip(el);
    }
  });
})();

router.init();
