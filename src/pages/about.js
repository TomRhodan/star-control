import { openUrl } from '@tauri-apps/plugin-opener';
import logoUrl from '../assets/logos/StarControl-Transparent-Logo-Image.png';
import madeByCommunityUrl from '../assets/logos/MadeByTheCommunity_White.png';

export function renderAbout(container) {
  container.innerHTML = `
    <div class="about-hero">
      <div class="about-hero-glow"></div>
      <div class="about-hero-icon">
        <img src="${logoUrl}" alt="Star Control Logo" />
      </div>
      <h1 class="about-hero-title">Star Control</h1>
      <p class="about-hero-version">v0.1.3</p>
      <p class="about-hero-tagline">Star Citizen Linux Manager</p>
    </div>

    <div class="about-grid">
      <div class="about-card">
        <h3>App Info</h3>
        <div class="about-info-row">
          <span class="about-info-label">Name</span>
          <span class="about-info-value">Star Control</span>
        </div>
        <div class="about-info-row">
          <span class="about-info-label">Version</span>
          <span class="about-info-value">v0.1.3</span>
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

      <div class="about-card">
        <h3>Credits & Inspirations</h3>
        <div class="about-credits-layout">
          <div class="about-community-badge">
            <img src="${madeByCommunityUrl}" alt="Made by the Community" />
          </div>
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
        <div class="about-disclaimer">
          Star Citizen is a registered trademark of Cloud Imperium Games Corporation. Star Control is not affiliated with or endorsed by Cloud Imperium Games.
        </div>
      </div>

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

  // Bind external link clicks
  container.querySelectorAll('.about-link[data-url]').forEach(link => {
    link.addEventListener('click', (e) => {
      e.preventDefault();
      openUrl(link.dataset.url);
    });
  });
}
