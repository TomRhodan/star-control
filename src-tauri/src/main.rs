//! Star Control -- Main entry point of the Tauri application.
//!
//! This is the binary entry point for the Star Control application.
//! In release builds on Windows, the console window is hidden via
//! `windows_subsystem`.
//!
//! The actual application logic resides in the `star_control_lib` crate (lib.rs).

// Suppress the Windows console window in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Entry point of the application.
/// Delegates all initialization and execution to the library.
fn main() {
    star_control_lib::run()
}
