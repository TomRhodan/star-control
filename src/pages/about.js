/**
 * Star Control - About Page
 *
 * This module renders the about page, which displays the following information:
 * - Application version and description
 * - Author information
 * - Credits and inspiration sources
 * - License information
 * - Links to GitHub, Wiki, and other resources
 *
 * @module pages/about
 */

// Tauri plugin for opening external URLs in the default browser
import { openUrl } from '@tauri-apps/plugin-opener';
import { getVersion } from '@tauri-apps/api/app';
// Static image assets for logo and community badge
import logoUrl from '../assets/logos/StarControl-Transparent-Logo-Image.png';
import madeByCommunityUrl from '../assets/logos/MadeByTheCommunity_White.png';

/**
 * Renders the about page into the provided container.
 * Creates a hero banner with logo and version, as well as a grid
 * with cards for app info, imprint, credits, and license.
 * All external links are opened via the Tauri openUrl plugin,
 * so they appear in the system browser instead of the WebView.
 *
 * @param {HTMLElement} container - The container element to render into
 */
export async function renderAbout(container) {
  const appVersion = await getVersion();
  container.innerHTML = `
    <div class="about-hero">
      <div class="about-hero-glow"></div>
      <div class="about-hero-icon">
        <img src="${logoUrl}" alt="Star Control Logo" />
      </div>
      <h1 class="about-hero-title">Star Control</h1>
      <p class="about-hero-version">v${appVersion}</p>
      <p class="about-hero-tagline">Star Citizen Linux Manager</p>
    </div>

    <div class="about-grid">
      <!-- Card: General app information (name, version, description, source code link) -->
      <div class="about-card">
        <h3>App Info</h3>
        <div class="about-info-row">
          <span class="about-info-label">Name</span>
          <span class="about-info-value">Star Control</span>
        </div>
        <div class="about-info-row">
          <span class="about-info-label">Version</span>
          <span class="about-info-value">v${appVersion}</span>
        </div>
        <div class="about-info-row">
          <span class="about-info-label">Description</span>
          <span class="about-info-value">A Linux management tool for Star Citizen. Install, configure, and launch Star Citizen with Wine/Proton on Linux.</span>
        </div>
        <div class="about-info-row">
          <span class="about-info-label">Source</span>
          <span class="about-info-value">
            <a href="#" class="about-link" data-url="https://github.com/TomRhodan/star-control">github.com/TomRhodan/star-control</a>
          </span>
        </div>
      </div>

      <!-- Card: Imprint with author and contact details -->
      <div class="about-card">
        <h3>Impressum</h3>
        <div class="about-info-row">
          <span class="about-info-label">Author</span>
          <span class="about-info-value">TomRhodan</span>
        </div>
        <div class="about-info-row">
          <span class="about-info-label">Email</span>
          <span class="about-info-value">
            <a href="#" class="about-link" data-url="mailto:tomrhodan@gmail.com">tomrhodan@gmail.com</a>
          </span>
        </div>
      </div>

      <!-- Card: Credits and acknowledgments to community projects -->
      <div class="about-card">
        <h3>Credits & Inspirations</h3>
        <div class="about-credits-layout">
          <!-- Community badge: "Made by the Community" -->
          <div class="about-community-badge">
            <img src="${madeByCommunityUrl}" alt="Made by the Community" />
          </div>
          <!-- Links to the projects that inspired Star Control -->
          <div class="about-credits-links">
            <div class="about-credit-item">
              <a href="#" class="about-link" data-url="https://github.com/starcitizen-lug/lug-helper">LUG Helper</a>
              <span class="about-credit-desc">Star Citizen LUG Helper script</span>
            </div>
            <div class="about-credit-item">
              <a href="#" class="about-link" data-url="https://luftwerft.com">luftwerft.com</a>
              <span class="about-credit-desc">SC Launcher Configurator</span>
            </div>
            <div class="about-credit-item">
              <a href="#" class="about-link" data-url="https://wiki.starcitizen-lug.org/">Star Citizen LUG Wiki</a>
              <span class="about-credit-desc">Community knowledge base for Linux gaming</span>
            </div>
          </div>
        </div>
        <!-- Legal disclaimer: Star Control is not affiliated with CIG -->
        <div class="about-disclaimer">
          Star Citizen is a registered trademark of Cloud Imperium Games Corporation. Star Control is not affiliated with or endorsed by Cloud Imperium Games.
        </div>
      </div>

      <!-- Card: License information (GPL-3.0) -->
      <div class="about-card">
        <h3>License</h3>
        <div class="about-info-row">
          <span class="about-info-label">License</span>
          <span class="about-info-value">GPL-3.0-or-later</span>
        </div>
        <div class="about-info-row">
          <span class="about-info-label">Copyright</span>
          <span class="about-info-value">2024-2026 TomRhodan</span>
        </div>
        <div class="about-info-row">
          <span class="about-info-label">Details</span>
          <span class="about-info-value">
            <a href="#" class="about-link" data-url="https://www.gnu.org/licenses/gpl-3.0.html">GNU General Public License v3.0</a>
          </span>
        </div>
      </div>
    </div>
  `;

  // Intercept external links: Use openUrl instead of normal navigation,
  // so links open in the system browser (not in the Tauri WebView)
  container.querySelectorAll('.about-link[data-url]').forEach(link => {
    link.addEventListener('click', (e) => {
      e.preventDefault();
      openUrl(link.dataset.url);
    });
  });
}
