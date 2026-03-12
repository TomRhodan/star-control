# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1] - 2026-03-12

### Changed
- Version bump to v0.2.1

## [0.2.0] - 2026-03-11

### Added
- **Multi-Device Bindings** — New "+" button on keybindings to add bindings for additional devices (e.g., add a joystick binding alongside an existing keyboard binding)
- **Auto-Select New Profile** — Newly created profiles are now immediately set as the active profile

### Fixed
- **Binding Editor Dialog** — Fixed invisible binding editor modal (missing `.show` class for CSS opacity transition)
- **Remove Binding** — Removing a binding no longer deletes all bindings for that action; only the specific device binding is removed
- **Version Selector Highlight** — Active version card now uses a visible cyan accent instead of blending into the background

### Changed
- **Device Reordering** — Swap logic now operates on profile backups instead of live SC files, scoped by device type
- **Version Selector** — Improved empty state detection

## [0.1.9] - 2026-03-08

### Added
- **Environment Management** — Renamed "Profiles" page to "Environments" to better reflect its role in managing game versions, storage, and settings
- **Empty State UI** — New setup screen for missing versions with options to "Create Folder", "Symlink Data.p4k" (space-saving), or "Copy Data.p4k"
- **Git-style Profile Actions** — Added "Update Profile" (save current game files back to profile) and "Revert" (discard local game changes) buttons to the active profile header
- **Environment Deletion** — Safe deletion of Star Citizen version folders with safety whitelist

### Fixed
- **Profile Metadata Error** — Fixed path logic in profile update command
- **Character File Backup** — Profile updates now correctly include `.chf` character files
- **UI Consistency** — Segmented control styling for version selector and underlined tabs for better visual hierarchy

## [0.1.8] - 2026-03-07

### Changed
- **Data.p4k Copy Buffer** — Increased from 1MB to 8MB for faster copy speeds on SSDs

### Added
- **Data.p4k Copy Progress Modal** — Shows progress, speed, and ETA during copy operation

### Fixed
- Improved UI padding for profile cards and footer

## [0.1.7] - 2026-03-05

### Changed
- **Unified Binding System** — Merged Controller tab into Profiles; bindings are now managed directly within saved profiles
- **Import from Version** — Non-destructive import that creates a saved profile instead of overwriting SC files
- **Profile Cards** — Wider card layout (340px min) so profile names are fully readable
- **Contextual Hints** — Dismissible guidance hints for profiles, bindings, and devices sections

### Added
- Profile-scoped binding commands (get/assign/remove bindings per profile)
- Cross-version profile import with saved profile selection
- Device name resolution in binding list via device map

### Removed
- Separate Controller tab and binding_database system
- Direct SC file overwrite on cross-version import

### Fixed
- Launch page log output formatting (no longer collapsed into single line)

## [0.1.6] - 2026-03-03

### Changed
- **Launch Page** — Separated Wayland into a dedicated experimental section with clearer documentation

### Fixed
- **Device Identity** — Corrected device identity resolution to properly match controllers using product name instead of instance numbers
- **Binding UI** — Improved binding display and interaction on the Controllers page

## [0.1.5] - 2026-03-02

### Fixed
- **Binding Export** — Fixed v_pitch and other bindings not appearing in exported actionmaps.xml when they didn't exist in the original SC bindings
- **Device Instance Mapping** — Fixed bindings being exported to wrong joystick instances by properly matching devices via product name and GUID

### Added
- **Device Reconciliation** — New "Reconcile Devices" button on Profiles page that syncs device instances with current SC actionmaps.xml configuration. This handles cases where device order changes (e.g., input-remapper changes joystick assignments)

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

[0.2.1]: https://github.com/TomRhodan/star-control/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/TomRhodan/star-control/compare/v0.1.9...v0.2.0
[0.1.9]: https://github.com/TomRhodan/star-control/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/TomRhodan/star-control/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/TomRhodan/star-control/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/TomRhodan/star-control/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/TomRhodan/star-control/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/TomRhodan/star-control/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/TomRhodan/star-control/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/TomRhodan/star-control/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/TomRhodan/star-control/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/TomRhodan/star-control/releases/tag/v0.1.0
