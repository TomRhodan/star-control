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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
