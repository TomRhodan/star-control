# Star Control

A Linux management tool for Star Citizen. Install, configure, and launch Star Citizen with Wine/Proton on Linux.

Built with [Tauri 2.0](https://tauri.app/) and vanilla JavaScript.

## Screenshots

*Screenshots coming soon.*

## Features

- **Installation Wizard** -- System compatibility check, automated Wine prefix setup, and RSI Launcher installation
- **Launch Manager** -- One-click launch with configurable performance options (ESync, FSync, DXVK Async, Wayland, HDR, FSR, MangoHUD)
- **Wine Runner Management** -- Download and manage Wine/Proton runners from multiple community sources (LUG, Kron4ek, RawFox, Mactan)
- **DXVK Management** -- Install and update DXVK versions with automatic DLL deployment
- **Profile Management** -- Backup and restore Star Citizen profiles (actionmaps.xml, attributes.xml, USER.cfg)
- **Controller Configuration** -- View connected devices, keybindings, and reorder joystick instances
- **USER.cfg Editor** -- Visual editor for all Star Citizen graphics, performance, and quality settings
- **Localization** -- Install community translations with one click
- **Prefix Tools** -- Winecfg, DPI scaling, PowerShell installation via winetricks
- **Multi-version Support** -- Manage LIVE, PTU, EPTU, and other Star Citizen channels

## Prerequisites

- **Linux** (tested on Arch Linux, should work on most distributions)
- **Rust** 1.70+
- **Node.js** 18+
- **Tauri system dependencies** -- see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

## Build from Source

```bash
git clone https://github.com/TomRhodan/star-control.git
cd star-control
npm install
```

### Development

```bash
npm run tauri dev
```

### Production Build

```bash
npm run tauri build
```

The built application will be in `src-tauri/target/release/`.

## Download

Pre-built binaries will be available on the [GitHub Releases](https://github.com/TomRhodan/star-control/releases) page.

## Usage

### First Run

1. Launch Star Control
2. Go to **Installation** and run the system compatibility check
3. Set your install directory and select a Wine runner
4. Start the installation to set up the Wine prefix and RSI Launcher

### Launching

- Navigate to the **Launch** page
- Configure performance options as needed
- Click the launch button

### Profiles

- Use the **Profiles** page to manage backups, keybindings, and USER.cfg settings
- Create backups before making changes
- Use the visual USER.cfg editor to tune graphics settings

### Wine Runners

- Go to **Wine Runners** to download, install, and switch between runners
- Manage DXVK versions and use prefix tools

## Contributing

Contributions are welcome!

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Commit your changes (`git commit -m 'Add my feature'`)
4. Push to the branch (`git push origin feature/my-feature`)
5. Open a Pull Request

## Credits

- [LUG Helper](https://github.com/starcitizen-lug/lug-helper) -- Star Citizen LUG Helper script
- [luftwerft.com](https://luftwerft.com) -- SC Launcher Configurator
- [Star Citizen LUG Wiki](https://wiki.starcitizen-lug.org/) -- Community knowledge base

Star Citizen is a registered trademark of Cloud Imperium Games Corporation. Star Control is not affiliated with or endorsed by Cloud Imperium Games.

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).

Copyright (C) 2024-2026 TomRhodan <tomrhodan@gmail.com>
