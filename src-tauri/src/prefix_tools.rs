//! Wine prefix utility tools module.
//!
//! This module provides various Wine prefix management tools:
//! - winecfg: Wine configuration dialog
//! - Wine shell: Terminal with Wine environment
//! - DPI settings: Get and set DPI in Wine prefix
//! - PowerShell: Install PowerShell via winetricks
//!
//! These tools help configure the Wine prefix for optimal Star Citizen performance.

use std::io::{ BufRead, BufReader };
use std::path::Path;
use std::process::{ Command, Stdio };
use tauri::{ AppHandle, Emitter };

/// Expands tilde (~) in a path to the user's home directory.
fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen('~', &home, 1);
        }
    }
    path.to_string()
}

/// Gets the paths for Wine binary, runner bin directory, and prefix.
/// Returns (prefix_path, wine_binary, runner_bin_dir).
fn get_wine_paths(
    base_path: &str,
    runner_name: &str
) -> Result<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf), String> {
    let expanded = expand_tilde(base_path);
    let prefix = Path::new(&expanded);
    let runner_bin = prefix.join("runners").join(runner_name).join("bin");
    let wine = runner_bin.join("wine");

    if !wine.exists() {
        return Err(format!("Wine binary not found: {}", wine.display()));
    }

    Ok((prefix.to_path_buf(), wine, runner_bin))
}

#[tauri::command]
pub async fn run_winecfg(base_path: String, runner_name: String) -> Result<(), String> {
    let (prefix, wine, _) = get_wine_paths(&base_path, &runner_name)?;

    Command::new(wine.to_string_lossy().as_ref())
        .arg("winecfg")
        .env("WINEPREFIX", prefix.to_string_lossy().as_ref())
        .env("WINEDLLOVERRIDES", "winemenubuilder.exe=d;winedbg.exe=d")
        .env("WINEDEBUG", "-all")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to launch winecfg: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn launch_wine_shell(base_path: String, runner_name: String) -> Result<(), String> {
    let (prefix, _wine, wine_bin_dir) = get_wine_paths(&base_path, &runner_name)?;

    let prefix_str = prefix.to_string_lossy().to_string();
    let wine_bin_str = wine_bin_dir.to_string_lossy().to_string();

    // Try to find a terminal emulator
    let terminal = if
        Command::new("which")
            .arg("konsole")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    {
        "konsole"
    } else if
        Command::new("which")
            .arg("gnome-terminal")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    {
        "gnome-terminal"
    } else if
        Command::new("which")
            .arg("xfce4-terminal")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    {
        "xfce4-terminal"
    } else if
        Command::new("which")
            .arg("xterm")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    {
        "xterm"
    } else {
        return Err(
            "No terminal emulator found (konsole, gnome-terminal, xfce4-terminal, xterm)".into()
        );
    };

    // Build a helper script that sets up the Wine environment.
    // Using a script file instead of shell string interpolation avoids
    // command injection risks from paths with special characters.
    let script_dir = std::path::Path::new(&prefix_str).join(".tmp");
    std::fs::create_dir_all(&script_dir).map_err(|e| format!("Failed to create tmp dir: {}", e))?;
    let script_path = script_dir.join("wine_shell.sh");
    let script_content = "#!/bin/bash\nexport WINEDEBUG=-all\nwine cmd\n";
    std::fs
        ::write(&script_path, script_content)
        .map_err(|e| format!("Failed to write shell script: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs
            ::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to chmod script: {}", e))?;
    }

    let script_path_str = script_path.to_string_lossy().to_string();

    // Build the terminal command with env vars passed safely via .env()
    let build_cmd = |name: &str, args: &[&str]| -> Result<(), String> {
        Command::new(name)
            .args(args)
            .env("WINEPREFIX", &prefix_str)
            .env("PATH", format!("{}:{}", wine_bin_str, std::env::var("PATH").unwrap_or_default()))
            .env("WINEDEBUG", "-all")
            .spawn()
            .map_err(|e| format!("Failed to launch {}: {}", name, e))?;
        Ok(())
    };

    // Launch terminal with wineconsole
    match terminal {
        "konsole" => build_cmd("konsole", &["--hold", "-e", "bash", &script_path_str])?,
        "gnome-terminal" => build_cmd("gnome-terminal", &["--", "bash", &script_path_str])?,
        "xfce4-terminal" =>
            build_cmd("xfce4-terminal", &["-e", &format!("bash {}", &script_path_str)])?,
        "xterm" => build_cmd("xterm", &["-e", "bash", &script_path_str])?,
        _ => {
            return Err("No terminal emulator found".into());
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn get_dpi(base_path: String, runner_name: String) -> Result<u32, String> {
    let (prefix, wine, _) = get_wine_paths(&base_path, &runner_name)?;

    tokio::task
        ::spawn_blocking(move || {
            let output = Command::new(wine.to_string_lossy().as_ref())
                .args(["reg", "query", "HKCU\\Control Panel\\Desktop", "/v", "LogPixels"])
                .env("WINEPREFIX", prefix.to_string_lossy().as_ref())
                .env("WINEDLLOVERRIDES", "winemenubuilder.exe=d;winedbg.exe=d")
                .env("WINEDEBUG", "-all")
                .output()
                .map_err(|e| format!("Failed to query DPI: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout);

            // Parse output: "    LogPixels    REG_DWORD    0x60"
            for line in stdout.lines() {
                if line.contains("LogPixels") {
                    // Find the hex value at the end
                    if let Some(hex_val) = line.split_whitespace().last() {
                        if let Some(stripped) = hex_val.strip_prefix("0x") {
                            if let Ok(val) = u32::from_str_radix(stripped, 16) {
                                return Ok(val);
                            }
                        }
                        // Try decimal
                        if let Ok(val) = hex_val.parse::<u32>() {
                            return Ok(val);
                        }
                    }
                }
            }

            // Default DPI
            Ok(96)
        }).await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn set_dpi(base_path: String, runner_name: String, dpi: u32) -> Result<(), String> {
    if !(96..=480).contains(&dpi) {
        return Err(format!("DPI must be between 96 and 480, got {}", dpi));
    }

    let (prefix, wine, _) = get_wine_paths(&base_path, &runner_name)?;

    tokio::task
        ::spawn_blocking(move || {
            let output = Command::new(wine.to_string_lossy().as_ref())
                .args([
                    "reg",
                    "add",
                    "HKCU\\Control Panel\\Desktop",
                    "/v",
                    "LogPixels",
                    "/t",
                    "REG_DWORD",
                    "/d",
                    &dpi.to_string(),
                    "/f",
                ])
                .env("WINEPREFIX", prefix.to_string_lossy().as_ref())
                .env("WINEDLLOVERRIDES", "winemenubuilder.exe=d;winedbg.exe=d")
                .env("WINEDEBUG", "-all")
                .output()
                .map_err(|e| format!("Failed to set DPI: {}", e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("Failed to set DPI: {}", stderr));
            }

            Ok(())
        }).await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn install_powershell(
    app: AppHandle,
    base_path: String,
    runner_name: String
) -> Result<(), String> {
    let (prefix, wine, wineserver) = get_wine_paths(&base_path, &runner_name)?;

    let emit_log = |line: &str| {
        let _ = app.emit("prefix-tool-log", line.to_string());
    };

    emit_log("Downloading winetricks...");

    // Download winetricks
    let tmp_dir = prefix.join(".tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("Failed to create tmp dir: {}", e))?;

    let winetricks_path = tmp_dir.join("winetricks");

    let client = reqwest::Client
        ::builder()
        .user_agent("star-control/0.1.5")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let wt_bytes = client
        .get("https://raw.githubusercontent.com/Winetricks/winetricks/master/src/winetricks")
        .send().await
        .map_err(|e| format!("Failed to download winetricks: {}", e))?
        .bytes().await
        .map_err(|e| format!("Failed to read winetricks: {}", e))?;

    std::fs
        ::write(&winetricks_path, &wt_bytes)
        .map_err(|e| format!("Failed to write winetricks: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs
            ::set_permissions(&winetricks_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to chmod winetricks: {}", e))?;
    }

    emit_log("Installing PowerShell via winetricks (this may take several minutes)...");

    // Create no_win64_warnings marker
    let marker = prefix.join("no_win64_warnings");
    let _ = std::fs::write(&marker, "");

    // Kill any lingering wineserver
    let _ = Command::new(wineserver.to_string_lossy().as_ref())
        .arg("-k")
        .env("WINEPREFIX", prefix.to_string_lossy().as_ref())
        .output();

    let mut child = Command::new(winetricks_path.to_string_lossy().as_ref())
        .args(["-q", "powershell"])
        .env("WINEPREFIX", prefix.to_string_lossy().as_ref())
        .env("WINE", wine.to_string_lossy().as_ref())
        .env("WINESERVER", wineserver.to_string_lossy().as_ref())
        .env("WINEDLLOVERRIDES", "winemenubuilder.exe=d;winedbg.exe=d")
        .env("WINEDEBUG", "-all")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run winetricks: {}", e))?;

    // Stream stderr on a separate thread
    let stderr_handle = child.stderr.take().map(|stderr| {
        let app_clone = app.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let _ = app_clone.emit("prefix-tool-log", line);
            }
        })
    });

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            emit_log(&line);
        }
    }

    if let Some(handle) = stderr_handle {
        let _ = handle.join();
    }

    let status = child.wait().map_err(|e| format!("Failed to wait for winetricks: {}", e))?;

    // Cleanup
    let _ = std::fs::remove_dir_all(&tmp_dir);

    if !status.success() {
        emit_log(&format!("winetricks powershell exited with code {:?}", status.code()));
        return Err(format!("winetricks powershell failed with exit code {:?}", status.code()));
    }

    // Create PowerShell marker file for detection
    let ps_marker = prefix.join(".powershell_installed");
    let _ = std::fs::write(&ps_marker, "1");

    emit_log("PowerShell installed successfully!");
    Ok(())
}

#[tauri::command]
pub async fn detect_powershell(base_path: String) -> Result<bool, String> {
    let result = tokio::task
        ::spawn_blocking(move || {
            let expanded = expand_tilde(&base_path);
            let prefix = Path::new(&expanded);

            // Check for marker file first (created after installation)
            let marker = prefix.join(".powershell_installed");
            if marker.exists() {
                return Ok::<bool, String>(true);
            }

            // Also check for actual PowerShell installation paths
            let ps_path1 = prefix
                .join("drive_c")
                .join("windows")
                .join("system32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe");

            let ps_path2 = prefix
                .join("drive_c")
                .join("Program Files")
                .join("PowerShell")
                .join("7")
                .join("pwsh.exe");

            Ok(ps_path1.exists() || ps_path2.exists())
        }).await
        .map_err(|e| format!("Task failed: {}", e))?;

    result.map_err(|e| format!("Task failed: {}", e))
}
