//! Star Control Library - Core Tauri application logic.
//!
//! This is the main library crate for Star Control, a desktop application
//! for managing Star Citizen on Linux with Wine/Proton.
//!
//! ## Modules
//!
//! - `config`: Application configuration management
//! - `dashboard`: RSI news, server status, and community stats
//! - `dxvk`: DXVK installation and detection
//! - `installer`: Game installation and launching
//! - `localization`: Language pack management
//! - `prefix_tools`: Wine prefix utilities (winecfg, DPI, PowerShell)
//! - `runners`: Wine runner (Wine/Proton) management
//! - `sc_config`: Star Citizen configuration and profile management
//! - `system_check`: System requirements checking
//!
//! ## Window State Management
//!
//! The application saves and restores window position, size, and scale
//! to provide a seamless user experience across sessions.

use tauri::Manager;

mod config;
mod dashboard;
mod dxvk;
mod installer;
mod localization;
mod prefix_tools;
mod runners;
mod sc_config;
mod system_check;

/// Simple greeting command for testing Tauri command infrastructure.
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to Star Control.", name)
}

/// Returns the path to the window state file.
/// The file is stored in the config directory under `star-control/window-state.json`.
fn window_state_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|p| p.join("star-control").join("window-state.json"))
}

/// Represents the saved window state including position, size, and scale.
#[derive(serde::Serialize, serde::Deserialize)]
struct WindowState {
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    maximized: bool,
    #[serde(default = "default_scale")]
    scale: f64,
}

/// Default scale factor for the window (1.0 = 100%).
fn default_scale() -> f64 {
    1.0
}

/// Loads the window state from the configuration file.
/// Returns None if the file doesn't exist or cannot be parsed.
fn load_window_state() -> Option<WindowState> {
    let path = window_state_path()?;
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Saves the current window state to the configuration file.
/// This is called when the window is closed.
/// Converts physical coordinates to logical coordinates for proper scaling.
fn save_window_state_from(window: &tauri::WebviewWindow) {
    if let Some(path) = window_state_path() {
        let Ok(size) = window.inner_size() else { return };
        let Ok(pos) = window.outer_position() else { return };
        let Ok(scale) = window.scale_factor() else { return };
        let maximized = window.is_maximized().unwrap_or(false);

        // Convert physical size to logical size for storage
        let logical_width = (size.width as f64 / scale) as u32;
        let logical_height = (size.height as f64 / scale) as u32;
        // Convert physical position to logical position
        let logical_x = (pos.x as f64 / scale) as i32;
        let logical_y = (pos.y as f64 / scale) as i32;

        let state = WindowState {
            width: logical_width,
            height: logical_height,
            x: logical_x,
            y: logical_y,
            maximized,
            scale,
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            let _ = std::fs::write(path, json);
        }
    }
}

/// Restores the window state from the configuration file.
/// This is called when the application starts.
/// Limits the window size to the monitor size to prevent issues.
fn restore_window_state(window: &tauri::WebviewWindow) {
    if let Some(state) = load_window_state() {
        // Use logical size directly
        let mut width = state.width;
        let mut height = state.height;

        // Limit to monitor size (in logical coordinates)
        if let Ok(Some(monitor)) = window.current_monitor() {
            let monitor_scale = monitor.scale_factor();
            let monitor_physical = monitor.size();
            let monitor_logical_w = (monitor_physical.width as f64 / monitor_scale) as u32;
            let monitor_logical_h = (monitor_physical.height as f64 / monitor_scale) as u32;

            width = width.min(monitor_logical_w.saturating_sub(50));
            height = height.min(monitor_logical_h.saturating_sub(50));
        }

        // Set size using LogicalSize (better for Wayland)
        let _ = window.set_size(tauri::LogicalSize::new(width, height));

        // Set position (on X11 only, Wayland ignores it)
        let current_scale = window.scale_factor().unwrap_or(1.0);
        let _ = window.set_position(tauri::PhysicalPosition::new(
            (state.x as f64 * current_scale) as i32,
            (state.y as f64 * current_scale) as i32,
        ));

        if state.maximized {
            let _ = window.maximize();
        }
    }
}

/// Main entry point for the Tauri application.
/// Initializes all plugins, commands, and sets up window event handlers.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(200));
                if let Some(window) = handle.get_webview_window("main") {
                    restore_window_state(&window);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            system_check::run_system_check,
            system_check::fix_mapcount,
            system_check::fix_filelimit,
            system_check::detect_monitors,
            system_check::get_default_install_path,
            config::check_needs_setup,
            config::create_install_directory,
            config::validate_install_path,
            config::scan_runners,
            config::save_config,
            config::load_config,
            config::load_runner_cache,
            config::save_runner_cache,
            config::load_dxvk_cache,
            config::save_dxvk_cache,
            config::reset_app,
            config::add_runner_source_from_github,
            config::import_lug_helper_sources,
            runners::fetch_available_runners,
            runners::install_runner,
            runners::cancel_runner_install,
            runners::delete_runner,
            dxvk::fetch_dxvk_releases,
            dxvk::detect_dxvk_version,
            dxvk::install_dxvk,
            prefix_tools::run_winecfg,
            prefix_tools::launch_wine_shell,
            prefix_tools::get_dpi,
            prefix_tools::set_dpi,
            prefix_tools::install_powershell,
            prefix_tools::detect_powershell,
            installer::run_installation,
            installer::cancel_installation,
            installer::check_installation,
            installer::is_game_running,
            installer::launch_game,
            installer::stop_game,
            sc_config::read_user_cfg,
            sc_config::write_user_cfg,
            sc_config::detect_sc_versions,
            sc_config::list_profiles,
            sc_config::export_profile,
            sc_config::import_profile,
            sc_config::read_attributes,
            sc_config::write_attributes,
            sc_config::parse_actionmaps,
            sc_config::reorder_devices,
            sc_config::backup_profile,
            sc_config::restore_profile,
            sc_config::list_backups,
            sc_config::delete_backup,
            sc_config::update_backup_label,
            sc_config::list_exported_layouts,
            localization::check_localization_update,
            localization::get_available_languages,
            localization::get_localization_status,
            localization::install_localization,
            localization::remove_localization,
            dashboard::fetch_rsi_news,
            dashboard::fetch_server_status,
            dashboard::fetch_community_stats,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Some(ww) = window.app_handle().get_webview_window(window.label()) {
                    save_window_state_from(&ww);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
