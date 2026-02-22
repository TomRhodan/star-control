//! Build script for the Star Control Tauri application.
//!
//! This file configures the Tauri build process. It uses tauri-build to generate
//! the necessary platform-specific code for the desktop application.

fn main() {
    tauri_build::build()
}
