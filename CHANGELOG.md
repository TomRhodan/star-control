# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.4] - 2026-03-02

### Changed
- **Controllers Page** — Separated controller and binding management into a dedicated "Controllers" page accessible from the sidebar. The Profiles page now focuses on backups and USER.cfg settings only.

## [0.1.3] - 2026-02-22

### Added
- **Quick Install Detection** — Automatically detects existing RSI Launcher installations and offers Quick Install (skip RSI Launcher download) or Full Reinstall options
- **Launch Log Transfer** — Installation logs are now displayed on the Launch page when navigating from installation completion
- **Dynamic Runner Sources** — Installation page now dynamically loads runner sources from LUG-Helper and displays tabs based on configured sources
- **Loading State** — Added loading spinner while fetching available Wine runners

## [0.1.2] - 2026-02-21

### Added
- Version bump to v0.1.2

## [0.1.1] - 2026-02-21

### Added
- **Fractional Scaling Support** — Window size and position now adapt correctly when moving between monitors with different DPI scaling (e.g., 100% ↔ 150%)
- **Wine Runner Sources** — Added LUG Experimental to available Wine runner sources
- **Wine Shell** — New prefix tool to launch a Wine command prompt in a terminal
- **About Page** — New About page with version info, links, and credits

### Updated
- Screenshots for dashboard, launch, wine runners, profiles, and about pages
- Default Wine runner sources configuration

## [0.1.0] - 2026-02-20

### Added
- **Command Center** — Live dashboard with RSI news, server status, and community funding stats
- **Installation Wizard** — System compatibility check, automated Wine prefix setup, and RSI Launcher installation
- **Launch Manager** — One-click launch with configurable performance options (ESync, FSync, DXVK Async, Wayland, HDR, FSR, MangoHUD)
- **Wine Runner Management** — Download and manage Wine/Proton runners from multiple community sources (LUG, Kron4ek, RawFox, Mactan)
- **DXVK Management** — Install and update DXVK versions with automatic DLL deployment
- **Profile Management** — Backup and restore Star Citizen profiles (actionmaps.xml, attributes.xml, USER.cfg)
- **Controller Configuration** — View connected devices, keybindings, and reorder joystick instances
- **USER.cfg Editor** — Visual editor for all Star Citizen graphics, performance, and quality settings
- **Localization** — Install community translations with one click, with automatic update detection
- **Prefix Tools** — Winecfg, DPI scaling, PowerShell installation via winetricks
- **Multi-version Support** — Manage LIVE, PTU, EPTU, and other Star Citizen channels

[0.1.4]: https://github.com/TomRhodan/star-control/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/TomRhodan/star-control/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/TomRhodan/star-control/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/TomRhodan/star-control/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/TomRhodan/star-control/releases/tag/v0.1.0
