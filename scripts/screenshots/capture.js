import { chromium } from 'playwright';
import { spawn } from 'child_process';
import path from 'path';
import fs from 'fs';

/**
 * Screenshot Automation for Star Control
 * 
 * This script starts the Vite dev server and uses Playwright to capture
 * high-quality screenshots of the application for the website.
 * 
 * It bypasses the "White Window" problem by connecting directly to the 
 * Vite dev server (frontend) and mocking/skipping Tauri-specific calls.
 */

const SCREENSHOT_DIR = path.join(process.cwd(), 'docs/star-control.de/assets/screenshots');
const VITE_PORT = 5173;
const BASE_URL = `http://localhost:${VITE_PORT}`;

// Map of page IDs to their target filenames
const PAGES = {
  'dashboard': 'dashboard.png',
  'runners': 'wine-runners.png',
  'launch': 'launch.png',
  'environments': 'configuration.png', // Webpage uses 'configuration' for environments
  'settings': 'usercfg-editor.png',    // Webpage uses 'usercfg-editor' for settings
  'about': 'about.png',
  'installation': 'installation.png'
};

async function run() {
  console.log('🚀 Starting Screenshot Automation...');

  // 1. Ensure screenshot directory exists
  if (!fs.existsSync(SCREENSHOT_DIR)) {
    fs.mkdirSync(SCREENSHOT_DIR, { recursive: true });
  }

  // 2. Start Vite Dev Server
  console.log('📦 Starting Vite dev server...');
  const vite = spawn('npm', ['run', 'dev'], { shell: true });
  
  // Wait for Vite to be ready
  await new Promise((resolve) => {
    vite.stdout.on('data', (data) => {
      if (data.toString().includes('built in')) resolve();
    });
    // Fallback timeout
    setTimeout(resolve, 5000);
  });

  // 3. Launch Playwright
  const browser = await chromium.launch();
  const context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
    deviceScaleFactor: 2, // HiDPI screenshots
  });

  const page = await context.newPage();

  // 4. Inject Tauri Mock
  // This prevents the "White Window" / "Tauri not found" errors
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {};
    window.__TAURI__ = {
      invoke: async (cmd, args) => {
        console.log(`[MOCK] Invoking ${cmd}`, args);
        // Return dummy data so the UI doesn't crash
        if (cmd === 'check_needs_setup') return { needs_setup: false };
        if (cmd === 'load_config') return { install_path: '/mock/path', selected_runner: 'GE-Proton9-20' };
        if (cmd === 'check_installation') return { installed: true, has_runner: true };
        if (cmd === 'get_localization_status') return { installed: true, language_name: 'Deutsch' };
        if (cmd === 'detect_sc_versions') return [{ version: 'LIVE' }];
        if (cmd === 'fetch_rsi_news') return { items: [] };
        if (cmd === 'fetch_server_status') return { components: [] };
        if (cmd === 'fetch_community_stats') return { stats: { funds: '$600M', fans: '5M', vehicles: '200' } };
        return {};
      },
      event: {
        listen: async () => ({ unlisten: () => {} })
      }
    };
    
    // Mock the specific @tauri-apps/api/core invoke
    window.__TAURI_INVOKE__ = window.__TAURI__.invoke;
  });

  console.log(`🌐 Navigating to ${BASE_URL}...`);
  await page.goto(BASE_URL);

  // 5. Capture Screenshots
  for (const [route, fileName] of Object.entries(PAGES)) {
    console.log(`📸 Capturing ${route} -> ${fileName}...`);
    
    // Use the router's internal navigation if possible, or trigger click on sidebar
    await page.evaluate((r) => {
      // Find the link in the sidebar and click it
      const link = document.querySelector(`.nav-link[data-page="${r}"]`);
      if (link) link.click();
    }, route);

    // Wait for animations/data to "load"
    await page.waitForTimeout(1000);

    const filePath = path.join(SCREENSHOT_DIR, fileName);
    await page.screenshot({ path: filePath });
    console.log(`✅ Saved to ${filePath}`);
  }

  // 6. Cleanup
  console.log('🧹 Cleaning up...');
  await browser.close();
  vite.kill();
  console.log('✨ All screenshots updated!');
  process.exit(0);
}

run().catch(err => {
  console.error('❌ Error during automation:', err);
  process.exit(1);
});
