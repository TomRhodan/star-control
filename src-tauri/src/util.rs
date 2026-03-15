//! Shared utility functions used across multiple modules.

use std::io;
use std::path::Path;

/// Opens a URL in the default browser robustly.
/// 
/// This bypasses Tauri's default opener to fix a known issue where AppImage builds
/// fail to open links because they bundle their own `xdg-open` or `LD_LIBRARY_PATH`
/// which conflicts with the host system's browser.
#[tauri::command]
pub fn open_browser(url: String) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("mailto:") {
        return Err("Invalid URL. Only http://, https:// and mailto: are allowed.".into());
    }

    log::info!("Opening URL via XDG Portal (D-Bus): {}", url);

    #[cfg(target_os = "linux")]
    {
        // 1. Try XDG Desktop Portal via dbus-send
        // This is the modern standard for sandbox escape (Flatpak/AppImage).
        // Syntax: dbus-send [options] <path> <interface.method> <type:value> [type:value...]
        let mut command = std::process::Command::new("dbus-send");
        command.args([
            "--session",
            "--dest=org.freedesktop.portal.Desktop",
            "--type=method_call",
            "--print-reply",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.OpenURI.OpenURI",
            "string:", // parent_window (empty string)
            &format!("string:{}", url), // uri (with type prefix!)
            "array:dict:string:variant:handle_token,string:starcontrol" // options (a{sv})
        ]);

        // Preserve only the minimum environment required for D-Bus
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
            // We wait a tiny bit to see if dbus-send exits immediately with an error (like the type error before)
            std::thread::sleep(std::time::Duration::from_millis(100));
            if let Ok(Some(status)) = child.try_wait() {
                if status.success() {
                    return Ok(());
                }
                log::warn!("dbus-send exited with status: {}. Trying fallback...", status);
            } else {
                // Still running or couldn't wait, assume it's working (portal calls are async)
                return Ok(());
            }
        }

        // 2. Fallback: gio open (Modern GNOME/Arch standard, very robust)
        log::info!("Trying fallback: gio open");
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

        // 3. Last resort: xdg-open with cleaned environment
        log::info!("Trying last resort: xdg-open");
        let mut xdg_cmd = std::process::Command::new("xdg-open");
        xdg_cmd.arg(&url);
        xdg_cmd.env_remove("LD_LIBRARY_PATH");
        xdg_cmd.env_remove("LD_PRELOAD");
        xdg_cmd.env_remove("APPDIR");
        xdg_cmd.env_remove("APPIMAGE");
        xdg_cmd.env_remove("XDG_DATA_DIRS"); // Important: don't look for .desktop files inside the AppImage

        match xdg_cmd.spawn() {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("All browser opening methods failed: {}", e);
                Err(format!("Failed to open browser: {}", e))
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err("Robust open_browser is only implemented for Linux.".into())
    }
}

/// Replaces `~` at the beginning of a path with the actual home directory.
///
/// Uses `dirs::home_dir()` which is more robust than `std::env::var("HOME")`
/// as it also works when the HOME variable is not set.
///
/// # Examples
/// ```
/// // "~/Games/star-citizen" -> "/home/user/Games/star-citizen"
/// // "/absolute/path" -> "/absolute/path" (unchanged)
/// ```
pub(crate) fn expand_tilde(p: &str) -> String {
    if p.starts_with('~') {
        if let Some(h) = dirs::home_dir() {
            return p.replacen('~', &h.to_string_lossy(), 1);
        }
    }
    p.to_string()
}

/// Safely unpacks a tar archive, validating that no entry escapes the target directory.
///
/// This prevents path-traversal attacks where a malicious archive could contain
/// entries like `../../.bashrc` to overwrite files outside the intended directory.
pub(crate) fn safe_unpack<R: io::Read>(archive: &mut tar::Archive<R>, dst: &Path) -> io::Result<()> {
    let canonical_dst = dst.canonicalize()?;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        // Resolve the full target path
        let target = canonical_dst.join(&path);

        // Canonicalize parent to resolve any `..` components.
        // The file itself may not exist yet, so we canonicalize the parent.
        let parent = target.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Entry has no parent directory")
        })?;

        // Create parent directories so canonicalize can work
        std::fs::create_dir_all(parent)?;

        let canonical_target = parent.canonicalize()?.join(
            target.file_name().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Entry has no file name")
            })?
        );

        if !canonical_target.starts_with(&canonical_dst) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Path traversal detected: {}", path.display()),
            ));
        }

        entry.unpack(&canonical_target)?;
    }

    Ok(())
}

/// Validates a custom environment variable key.
///
/// Returns `Ok(())` if the key is valid, or an error message if not.
/// - Keys must only contain `[A-Za-z0-9_]`
/// - Certain security-sensitive keys are blocked to prevent abuse
pub(crate) fn validate_env_var_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("Environment variable key cannot be empty".to_string());
    }

    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!(
            "Environment variable key '{}' contains invalid characters (only A-Z, a-z, 0-9, _ allowed)",
            key
        ));
    }

    const BLOCKED_KEYS: &[&str] = &[
        "PATH", "LD_PRELOAD", "LD_LIBRARY_PATH", "HOME", "USER", "SHELL",
    ];

    if BLOCKED_KEYS.contains(&key) {
        return Err(format!(
            "Environment variable '{}' is blocked for security reasons",
            key
        ));
    }

    Ok(())
}
