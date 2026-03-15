//! Shared utility functions used across multiple modules.

use std::io;
use std::path::Path;
use tauri::Window;
use base64::{Engine as _, engine::general_purpose};

/// Robustly captures the current active window to a file using system tools.
/// Automatically detects project root to avoid rebuild loops during development.
#[tauri::command]
pub async fn capture_app_window(window: Window, filename: String) -> Result<(), String> {
    let mut project_root = std::env::current_dir().map_err(|e| format!("Failed to get current dir: {}", e))?;
    
    // If we are in src-tauri, we must go up to reach the project root
    if project_root.ends_with("src-tauri") {
        project_root.pop();
    }
    
    let target_path = project_root
        .join("docs/star-control.de/assets/screenshots")
        .join(&filename);

    log::info!("Capturing window to: {:?}", target_path);

    // Ensure directory exists
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        // Focus the window before taking the screenshot to ensure it's on top
        let _ = window.set_focus();
        
        // Short delay to ensure UI has finished rendering and focus is set
        std::thread::sleep(std::time::Duration::from_millis(500));

        // KDE Spectacle: -a (active window), -b (background), -n (non-interactive), -o (output)
        let spectacle = std::process::Command::new("spectacle")
            .args(["-a", "-b", "-n", "-o", target_path.to_str().unwrap()])
            .status();
        if spectacle.is_ok() && spectacle.unwrap().success() { return Ok(()); }

        // Fallback: GNOME Screenshot
        let gnome = std::process::Command::new("gnome-screenshot")
            .args(["-w", "-f", target_path.to_str().unwrap()])
            .status();
        if gnome.is_ok() && gnome.unwrap().success() { return Ok(()); }

        // Fallback: Grim (Wayland Generic)
        let grim = std::process::Command::new("grim")
            .args([target_path.to_str().unwrap()])
            .status();
        if grim.is_ok() && grim.unwrap().success() { return Ok(()); }

        Err("No screenshot tool found. Please install spectacle or gnome-screenshot.".into())
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err("Only Linux supported.".into())
    }
}

/// Robustly saves a base64 encoded image (Fallback).
#[tauri::command]
pub async fn save_screenshot(base64_data: String, filename: String) -> Result<(), String> {
    let mut project_root = std::env::current_dir().map_err(|e| e.to_string())?;
    if project_root.ends_with("src-tauri") { project_root.pop(); }
    
    let target_path = project_root.join("docs/star-control.de/assets/screenshots").join(&filename);
    let data = base64_data.split(',').next_back().ok_or("Invalid image data")?;
    let decoded = general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    
    std::fs::write(target_path, decoded).map_err(|e| e.to_string())?;
    Ok(())
}

/// Opens a URL in the default browser robustly.
#[tauri::command]
pub fn open_browser(url: String) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("mailto:") {
        return Err("Invalid URL. Only http://, https:// and mailto: are allowed.".into());
    }

    log::info!("Opening URL via XDG Portal (D-Bus): {}", url);

    #[cfg(target_os = "linux")]
    {
        let mut command = std::process::Command::new("dbus-send");
        command.args([
            "--session",
            "--dest=org.freedesktop.portal.Desktop",
            "--type=method_call",
            "--print-reply",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.OpenURI.OpenURI",
            "string:",
            &format!("string:{}", url),
            "array:dict:string:variant:handle_token,string:starcontrol"
        ]);

        command.env_clear();
        if let Ok(dbus_addr) = std::env::var("DBUS_SESSION_BUS_ADDRESS") {
            command.env("DBUS_SESSION_BUS_ADDRESS", dbus_addr);
        }
        if let Ok(display) = std::env::var("DISPLAY") {
            command.env("DISPLAY", display);
        }
        if let Ok(w_display) = std::env::var("WAYLAND_DISPLAY") {
            command.env("WAYLAND_DISPLAY", w_display);
        }

        if let Ok(mut child) = command.spawn() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if let Ok(Some(status)) = child.try_wait() {
                if status.success() { return Ok(()); }
            } else {
                return Ok(());
            }
        }

        let mut gio_cmd = std::process::Command::new("gio");
        gio_cmd.arg("open").arg(&url);
        gio_cmd.env_remove("LD_LIBRARY_PATH");
        gio_cmd.env_remove("LD_PRELOAD");
        gio_cmd.env_remove("APPDIR");
        gio_cmd.env_remove("APPIMAGE");
        if let Ok(mut child) = gio_cmd.spawn() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if let Ok(Some(status)) = child.try_wait() {
                if status.success() { return Ok(()); }
            } else {
                return Ok(());
            }
        }

        let mut xdg_cmd = std::process::Command::new("xdg-open");
        xdg_cmd.arg(&url);
        xdg_cmd.env_remove("LD_LIBRARY_PATH");
        xdg_cmd.env_remove("LD_PRELOAD");
        xdg_cmd.env_remove("APPDIR");
        xdg_cmd.env_remove("APPIMAGE");
        xdg_cmd.env_remove("XDG_DATA_DIRS");

        match xdg_cmd.spawn() {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to open browser: {}", e))
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err("Robust open_browser is only implemented for Linux.".into())
    }
}

pub(crate) fn expand_tilde(p: &str) -> String {
    if p.starts_with('~') {
        if let Some(h) = dirs::home_dir() {
            return p.replacen('~', &h.to_string_lossy(), 1);
        }
    }
    p.to_string()
}

pub(crate) fn safe_unpack<R: io::Read>(archive: &mut tar::Archive<R>, dst: &Path) -> io::Result<()> {
    let canonical_dst = dst.canonicalize()?;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let target = canonical_dst.join(&path);
        let parent = target.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "No parent"))?;
        std::fs::create_dir_all(parent)?;
        let canonical_target = parent.canonicalize()?.join(target.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "No name"))?);
        if !canonical_target.starts_with(&canonical_dst) { return Err(io::Error::new(io::ErrorKind::InvalidInput, "Traversal")); }
        entry.unpack(&canonical_target)?;
    }
    Ok(())
}

pub(crate) fn validate_env_var_key(key: &str) -> Result<(), String> {
    if key.is_empty() { return Err("Empty".to_string()); }
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') { return Err("Invalid".to_string()); }
    const BLOCKED: &[&str] = &["PATH", "LD_PRELOAD", "LD_LIBRARY_PATH", "HOME", "USER", "SHELL"];
    if BLOCKED.contains(&key) { return Err("Blocked".to_string()); }
    Ok(())
}
