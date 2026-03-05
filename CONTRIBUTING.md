# Contributing to Star Control

Thank you for your interest in contributing to Star Control, a Linux desktop application for managing Star Citizen installations. This guide will help you get started.

## Prerequisites

- **Rust toolchain** (stable): Install via [rustup](https://rustup.rs/)
- **Node.js** (v18+) and npm
- **Tauri system dependencies** for Linux:

```bash
# Debian/Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev

# Fedora
sudo dnf install webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel patchelf gtk3-devel libsoup3-devel javascriptcoregtk4.1-devel

# Arch Linux
sudo pacman -S webkit2gtk-4.1 libappindicator-gtk3 librsvg patchelf
```

- **Tauri CLI**: `cargo install tauri-cli`

## Development Setup

1. Clone the repository:

   ```bash
   git clone https://github.com/TomRhodan/star-control.git
   cd star-control
   ```

2. Install frontend dependencies:

   ```bash
   npm install
   ```

3. Run in development mode:

   ```bash
   cargo tauri dev
   ```

   This starts both the Vite dev server (frontend) and the Rust backend with hot-reload.

## Project Structure

```
star-control/
  src/                    # Frontend (JS/HTML)
    pages/                # Page-specific JavaScript
    styles/               # CSS stylesheets
  src-tauri/
    src/                  # Rust backend
      main.rs             # Application entry point
      lib.rs              # Library root, Tauri command definitions
    Cargo.toml            # Rust dependencies
    tauri.conf.json       # Tauri configuration
  package.json            # Node.js dependencies & scripts
```

## Code Style

### Rust

- Run `cargo fmt` before committing.
- Run `cargo clippy` and resolve all warnings.
- Avoid `unwrap()` in error paths -- use proper error handling (`?`, `Result`, `.unwrap_or_default()`, etc.).

### JavaScript / HTML / CSS

- Use 2-space indentation.
- Keep frontend code in the appropriate `src/pages/` or `src/styles/` directory.

## Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/) format:

- `feat:` -- new feature
- `fix:` -- bug fix
- `docs:` -- documentation changes
- `chore:` -- maintenance tasks (deps, CI, etc.)
- `refactor:` -- code restructuring without behavior change
- `style:` -- formatting, whitespace (no logic change)
- `test:` -- adding or updating tests

Example: `feat: add Wine version selection dropdown`

## Pull Request Process

1. Create a feature branch from `main`.
2. Make your changes and ensure they compile without warnings (`cargo clippy`).
3. **Test manually** by running `cargo tauri dev` and verifying the following areas still work:
   - Dashboard (status overview, launch)
   - SC Versions (install/manage Star Citizen versions)
   - Bindings (import, view, and export controller bindings)
   - Settings (configuration options)
4. Write a clear PR description explaining what changed and why.
5. Submit the PR against `main`.

## License

By contributing to Star Control, you agree that your contributions will be licensed under the [GPL-3.0-or-later](LICENSE) license.
