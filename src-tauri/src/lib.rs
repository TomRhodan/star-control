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

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to Star Control.", name)
}

fn window_state_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|p| p.join("star-control").join("window-state.json"))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct WindowState {
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    maximized: bool,
}

fn load_window_state() -> Option<WindowState> {
    let path = window_state_path()?;
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_window_state_from(window: &tauri::WebviewWindow) {
    if let Some(path) = window_state_path() {
        let Ok(size) = window.inner_size() else { return };
        let Ok(pos) = window.outer_position() else { return };
        let maximized = window.is_maximized().unwrap_or(false);

        let state = WindowState {
            width: size.width,
            height: size.height,
            x: pos.x,
            y: pos.y,
            maximized,
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn restore_window_state(window: &tauri::WebviewWindow) {
    if let Some(state) = load_window_state() {
        let _ = window.set_size(tauri::PhysicalSize::new(state.width, state.height));
        let _ = window.set_position(tauri::PhysicalPosition::new(state.x, state.y));
        if state.maximized {
            let _ = window.maximize();
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                let handle2 = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    if let Some(window) = handle2.get_webview_window("main") {
                        restore_window_state(&window);
                    }
                });
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
            runners::fetch_available_runners,
            runners::install_runner,
            runners::cancel_runner_install,
            runners::delete_runner,
            dxvk::fetch_dxvk_releases,
            dxvk::detect_dxvk_version,
            dxvk::install_dxvk,
            prefix_tools::run_winecfg,
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
