//! Star Control - Main entry point for the Tauri application.
//!
//! This is the main binary entry point for the Star Control application.
//! On Windows in release builds, it uses the windows_subsystem to hide the console window.
//!
//! The actual application logic is in the `star_control_lib` crate (lib.rs).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    star_control_lib::run()
}
