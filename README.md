<p align="center">
  <img src="docs/star-control.de/assets/logo-hero.png" alt="Star Control" width="400">
</p>

<h1 align="center">Star Control</h1>

<p align="center">
  A Linux management tool for Star Citizen.<br>
  Install, configure, and launch Star Citizen with Wine/Proton - no terminal required.
</p>

<p align="center">
  Built with <a href="https://tauri.app/">Tauri 2</a> and vanilla JavaScript.
  <br>
  <strong>Version 0.2.4</strong>
</p>

---

## Screenshots

<p align="center">
  <img src="docs/star-control.de/assets/screenshots/dashboard.png" alt="Command Center" width="720"><br>
  <em>Command Center - RSI news, server status, and community stats at a glance</em>
</p>

<details>
<summary>More screenshots</summary>

<br>

<p align="center">
  <img src="docs/star-control.de/assets/screenshots/launch.png" alt="Launch" width="720"><br>
  <em>Launch - One-click start with performance options and experimental Wayland support</em>
</p>

<p align="center">
  <img src="docs/star-control.de/assets/screenshots/system-check.png" alt="System Check" width="720"><br>
  <em>Installation - Automated system compatibility check</em>
</p>

<p align="center">
  <img src="docs/star-control.de/assets/screenshots/configuration.png" alt="Configuration" width="720"><br>
  <em>Configuration - Install directory, Wine runner, and performance options</em>
</p>

<p align="center">
  <img src="docs/star-control.de/assets/screenshots/installation.png" alt="Installation" width="720"><br>
  <em>Installation - Automated Wine prefix setup and RSI Launcher download</em>
</p>

<p align="center">
  <img src="docs/star-control.de/assets/screenshots/wine-runners.png" alt="Wine Runners" width="720"><br>
  <em>Wine Runners - Download runners, manage DXVK, and access prefix tools</em>
</p>

<p align="center">
  <img src="docs/star-control.de/assets/screenshots/profiles.png" alt="Profiles" width="720"><br>
  <em>Profiles - Backup and restore Star Citizen profiles and USER.cfg settings</em>
</p>

<p align="center">
  <img src="docs/star-control.de/assets/screenshots/usercfg-editor.png" alt="USER.cfg Editor" width="720"><br>
  <em>USER.cfg Editor - Visual editor for graphics and performance settings</em>
</p>

<p align="center">
  <img src="docs/star-control.de/assets/screenshots/controllers.png" alt="Controllers" width="720"><br>
  <em>Controllers - Profile manager, connected devices, keybindings, and joystick reordering</em>
</p>

<p align="center">
  <img src="docs/star-control.de/assets/screenshots/localization.png" alt="Localization" width="720"><br>
  <em>Localization - Install community translations with one click</em>
</p>

<p align="center">
  <img src="docs/star-control.de/assets/screenshots/about.png" alt="About" width="720"><br>
  <em>About - Version info, links, and credits</em>
</p>

</details>

## Features

- **Quick Install** - Automatically detects existing RSI Launcher installations and offers Quick Install (skip RSI Launcher download) or Full Reinstall
- **Command Center** - Live dashboard with RSI news, server status, and community funding stats
- **Installation Wizard** - System compatibility check, automated Wine prefix setup, and RSI Launcher installation
- **Launch Manager** - One-click launch with configurable performance options (ESync, FSync, DXVK Async, DXVK HUD, HDR, FSR, MangoHUD) and experimental Wayland support
- **Wine Runner Management** - Download and manage Wine/Proton runners from multiple community sources (LUG, LUG Experimental, Kron4ek, RawFox, Mactan)
- **DXVK Management** - Install and update DXVK versions with automatic DLL deployment
- **Profile Management** - Backup and restore Star Citizen profiles (actionmaps.xml, attributes.xml, USER.cfg)
- **Controllers Page** - View connected devices, keybindings, reorder joystick instances, and manage binding profiles with import/export from Star Citizen
- **USER.cfg Editor** - Visual editor for all Star Citizen graphics, performance, and quality settings
- **Localization** - Install community translations with one click, with automatic update detection
- **Prefix Tools** - Winecfg, Wine Shell, DPI scaling, PowerShell installation via winetricks
- **Multi-version Support** - Manage LIVE, PTU, EPTU, and other Star Citizen channels

## Installation

### Download from Releases

Pre-built packages are available on the [GitHub Releases](https://github.com/TomRhodan/star-control/releases) page. Download the package that matches your distribution.

#### Debian / Ubuntu / Linux Mint (.deb)

```bash
# Download the .deb file from the latest release, then install it:
sudo dpkg -i star-control_*.deb

# If there are missing dependencies, fix them with:
sudo apt install -f
```

After installation, Star Control appears in your application menu, or you can launch it from the terminal:

```bash
star-control
```

To uninstall:

```bash
sudo dpkg -r star-control
```

#### AppImage (any distribution)

The AppImage is a portable executable that works on any Linux distribution without installation.

```bash
# Download the .AppImage file from the latest release, then make it executable:
chmod +x Star-Control_*.AppImage

# Run it:
./Star-Control_*.AppImage
```

You can move the AppImage anywhere you like, for example:

```bash
mv Star-Control_*.AppImage ~/Applications/
```

> **Tip:** Most desktop environments allow you to right-click the AppImage file, go to Properties > Permissions, and check "Allow executing file as program" instead of using the terminal.

### Build from Source

#### Prerequisites

- **Linux** (tested on Arch Linux, should work on most distributions)
- **Rust** 1.70+
- **Node.js** 18+
- **Tauri system dependencies** - see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

#### Steps

```bash
git clone https://github.com/TomRhodan/star-control.git
cd star-control
npm install
```

Development mode with hot-reload:

```bash
npm run tauri dev
```

Production build:

```bash
npm run tauri build
```

The built binary will be in `src-tauri/target/release/` and packages in `src-tauri/target/release/bundle/`.

## Usage

### First Run

1. Launch Star Control
2. The **Installation** page runs a system compatibility check automatically
3. Set your install directory and select a Wine runner
4. Start the installation to set up the Wine prefix and RSI Launcher

### Launching Star Citizen

- Navigate to the **Launch** page
- Configure performance options as needed (ESync, FSync, DXVK, etc.)
- Click the launch button

### Profiles & Settings

- Use the **Profiles** page to manage backups and USER.cfg settings
- Use the **Controllers** page to manage connected devices, keybindings, and binding profiles
- Create backups before making changes
- Use the visual USER.cfg editor to tune graphics settings

### Wine Runners & DXVK

- Go to **Wine Runners** to download, install, and switch between runners
- Manage DXVK versions and use prefix tools (Winecfg, DPI, PowerShell)

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for a detailed list of changes.

## Documentation

The Rust backend is fully documented with `///` doc comments. You can generate and browse the API documentation locally:

```bash
cargo doc --no-deps --open
```

## Testing

Star Control currently relies on manual testing. When contributing, please verify your changes by running `cargo tauri dev` and testing the affected pages. Automated tests are a welcome contribution -- see [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) for details on the development setup, code style, and pull request process. This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md).

We use [GitHub Issue Templates](.github/ISSUE_TEMPLATE/) for bug reports and feature requests.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Commit your changes (`git commit -m 'Add my feature'`)
4. Push to the branch (`git push origin feature/my-feature`)
5. Open a Pull Request

## Credits

- [LUG Helper](https://github.com/starcitizen-lug/lug-helper) - Star Citizen LUG Helper script
- [luftwerft.com](https://luftwerft.com) - SC Launcher Configurator
- [Star Citizen LUG Wiki](https://wiki.starcitizen-lug.org/) - Community knowledge base

Star Citizen is a registered trademark of Cloud Imperium Games Corporation. Star Control is not affiliated with or endorsed by Cloud Imperium Games.

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).

Copyright (C) 2024-2026 TomRhodan <tomrhodan@gmail.com>
