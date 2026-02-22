use crate::config::{AppConfig, PerformanceSettings};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

static INSTALL_CANCEL: AtomicBool = AtomicBool::new(false);

/// Stores the PID of the running game process and the install path (for wineserver cleanup).
static GAME_PID: Mutex<Option<(u32, String)>> = Mutex::new(None);

#[derive(Serialize, Deserialize, Clone)]
pub struct InstallProgress {
    pub phase: String,
    pub step: String,
    pub percent: f64,
    pub log_line: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct InstallationStatus {
    pub installed: bool,
    pub has_runner: bool,
    pub runner_name: Option<String>,
    pub install_path: String,
    pub launcher_exe_exists: bool,
    pub message: String,
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen('~', &home, 1);
        }
    }
    path.to_string()
}

fn emit_progress(app: &AppHandle, phase: &str, step: &str, percent: f64, log_line: &str) {
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            phase: phase.to_string(),
            step: step.to_string(),
            percent,
            log_line: log_line.to_string(),
        },
    );
}

fn is_cancelled() -> bool {
    INSTALL_CANCEL.load(Ordering::Relaxed)
}

fn stream_command_output(app: &AppHandle, phase: &str, step: &str, percent: f64, child: &mut std::process::Child) {
    // Read stdout and stderr on separate threads to avoid pipe deadlocks.
    // A deadlock occurs when the child fills the stderr buffer while we block on stdout.
    let stderr_handle = child.stderr.take().map(|stderr| {
        let app = app.clone();
        let phase = phase.to_string();
        let step = step.to_string();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    emit_progress(&app, &phase, &step, percent, &line);
                }
            }
        })
    });

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(line) = line {
                emit_progress(app, phase, step, percent, &line);
            }
        }
    }

    if let Some(handle) = stderr_handle {
        let _ = handle.join();
    }
}

fn configure_wine_env(cmd: &mut Command, install_path: &str, perf: &PerformanceSettings, log_level: &str) -> Vec<(String, String)> {
    let mut vars: Vec<(String, String)> = Vec::new();

    // Core Wine env
    vars.push(("WINEPREFIX".into(), install_path.into()));
    vars.push(("WINEDLLOVERRIDES".into(), "winemenubuilder.exe=d;winedbg.exe=d".into()));

    // WINEDEBUG: show more output in debug mode
    let winedebug = match log_level {
        "debug" => "+waylanddrv,+explorer,err+all",
        _ => "-all",
    };
    vars.push(("WINEDEBUG".into(), winedebug.into()));

    // Performance flags
    if perf.esync {
        vars.push(("WINEESYNC".into(), "1".into()));
    }
    if perf.fsync {
        vars.push(("WINEFSYNC".into(), "1".into()));
    }
    if perf.dxvk_async {
        vars.push(("DXVK_ASYNC".into(), "1".into()));
    }

    // Display — Wayland
    if perf.wayland {
        vars.push(("PROTON_ENABLE_WAYLAND".into(), "1".into())); // Proton runners
        // Plain Wine runners: fully remove DISPLAY so Wine's X11 driver
        // initialization fails and the Wayland driver takes over.
        // Setting DISPLAY="" is not enough — getenv("DISPLAY") still returns
        // a non-NULL pointer and Wine attempts X11 anyway.
        vars.push(("DISPLAY".into(), "(removed)".to_string())); // logged only
    }
    if perf.hdr {
        vars.push(("PROTON_ENABLE_HDR".into(), "1".into()));
        vars.push(("DXVK_HDR".into(), "1".into()));
    }
    if perf.fsr {
        vars.push(("PROTON_FSR4_UPGRADE".into(), "1".into()));
    }
    if let Some(ref monitor) = perf.primary_monitor {
        vars.push(("WAYLANDDRV_PRIMARY_MONITOR".into(), monitor.clone()));
    }

    // Overlays
    if perf.mangohud {
        vars.push(("MANGOHUD".into(), "1".into()));
    }
    if perf.dxvk_hud {
        vars.push(("DXVK_HUD".into(), "fps,compiler".into()));
    }

    // Shader caches
    vars.push(("__GL_SHADER_DISK_CACHE".into(), "1".into()));
    vars.push(("__GL_SHADER_DISK_CACHE_SIZE".into(), "10737418240".into()));
    vars.push(("__GL_SHADER_DISK_CACHE_PATH".into(), install_path.into()));
    vars.push(("__GL_SHADER_DISK_CACHE_SKIP_CLEANUP".into(), "1".into()));
    vars.push(("MESA_SHADER_CACHE_DIR".into(), install_path.into()));
    vars.push(("MESA_SHADER_CACHE_MAX_SIZE".into(), "10G".into()));

    // Apply all env vars to the command
    for (key, val) in &vars {
        if key == "DISPLAY" {
            // Remove DISPLAY entirely — don't set it to empty
            cmd.env_remove("DISPLAY");
        } else {
            cmd.env(key, val);
        }
    }

    vars
}

#[tauri::command]
pub fn check_installation(config: AppConfig) -> InstallationStatus {
    let install_path = expand_tilde(&config.install_path);

    let runner_name = config.selected_runner.clone();
    let has_runner = runner_name.as_ref().map_or(false, |name| {
        let wine = Path::new(&install_path)
            .join("runners")
            .join(name)
            .join("bin")
            .join("wine");
        wine.exists()
    });

    let launcher_exe = Path::new(&install_path)
        .join("drive_c")
        .join("Program Files")
        .join("Roberts Space Industries")
        .join("RSI Launcher")
        .join("RSI Launcher.exe");
    let launcher_exe_exists = launcher_exe.exists();

    let message = if runner_name.is_none() {
        "No runner selected".to_string()
    } else if !has_runner {
        format!("Runner '{}' not found", runner_name.as_deref().unwrap_or(""))
    } else if !launcher_exe_exists {
        "RSI Launcher not found — please run installation first".to_string()
    } else {
        "Ready to launch".to_string()
    };

    let installed = has_runner && launcher_exe_exists;

    InstallationStatus {
        installed,
        has_runner,
        runner_name,
        install_path,
        launcher_exe_exists,
        message,
    }
}

#[tauri::command]
pub async fn launch_game(app: AppHandle, config: AppConfig) -> Result<(), String> {
    let install_path = expand_tilde(&config.install_path);
    let runner_name = config
        .selected_runner
        .as_deref()
        .ok_or("No runner selected")?;
    let log_level = config.log_level.as_str();
    let is_debug = log_level == "debug";

    let runner_bin = Path::new(&install_path)
        .join("runners")
        .join(runner_name)
        .join("bin");
    let wine = runner_bin.join("wine");
    let wineserver = runner_bin.join("wineserver");

    if !wine.exists() {
        return Err(format!("Wine binary not found: {}", wine.to_string_lossy()));
    }

    let launcher_exe = Path::new(&install_path)
        .join("drive_c")
        .join("Program Files")
        .join("Roberts Space Industries")
        .join("RSI Launcher")
        .join("RSI Launcher.exe");

    if !launcher_exe.exists() {
        return Err("RSI Launcher not found — please run installation first".to_string());
    }

    // --- Log: Header ---
    let _ = app.emit("launch-log", "────────────────────────────────────────");
    let _ = app.emit("launch-log", "  Star Control — Launch");
    let _ = app.emit("launch-log", "────────────────────────────────────────");

    // --- Log: Runner & paths ---
    let _ = app.emit("launch-log", &format!("Runner:     {}", runner_name));
    let _ = app.emit("launch-log", &format!("Wine:       {}", wine.to_string_lossy()));
    let _ = app.emit("launch-log", &format!("Prefix:     {}", install_path));

    // --- Log: Wine version ---
    match Command::new(wine.to_string_lossy().as_ref())
        .arg("--version")
        .env("WINEPREFIX", &install_path)
        .output()
    {
        Ok(out) => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !version.is_empty() {
                let _ = app.emit("launch-log", &format!("Version:    {}", version));
            }
        }
        Err(_) => {}
    }

    let _ = app.emit("launch-log", "");

    // --- Kill lingering wineserver ---
    let _ = app.emit("launch-log", "> Killing old wineserver processes...");
    let _ = Command::new(wineserver.to_string_lossy().as_ref())
        .arg("-k")
        .env("WINEPREFIX", &install_path)
        .output();

    // --- Build command ---
    let mut cmd = Command::new(wine.to_string_lossy().as_ref());
    cmd.arg("C:\\Program Files\\Roberts Space Industries\\RSI Launcher\\RSI Launcher.exe");

    let env_log = configure_wine_env(&mut cmd, &install_path, &config.performance, log_level);

    // --- Log: Environment variables ---
    let _ = app.emit("launch-log", "");
    let _ = app.emit("launch-log", "> Environment variables:");
    for (key, val) in &env_log {
        if val.is_empty() {
            let _ = app.emit("launch-log", &format!("  {}=\"\" (cleared — forcing Wayland driver)", key));
        } else {
            let _ = app.emit("launch-log", &format!("  {}={}", key, val));
        }
    }

    // --- Log: Performance settings summary ---
    let perf = &config.performance;
    let _ = app.emit("launch-log", "");
    let _ = app.emit("launch-log", "> Performance settings:");
    let _ = app.emit("launch-log", &format!("  ESync={}, FSync={}, DXVK Async={}", perf.esync, perf.fsync, perf.dxvk_async));
    let _ = app.emit("launch-log", &format!("  Wayland={}, HDR={}, FSR={}", perf.wayland, perf.hdr, perf.fsr));
    let _ = app.emit("launch-log", &format!("  MangoHUD={}, DXVK HUD={}", perf.mangohud, perf.dxvk_hud));
    if let Some(ref monitor) = perf.primary_monitor {
        let _ = app.emit("launch-log", &format!("  Primary Monitor={}", monitor));
    }

    // --- Log: Full command (debug only) ---
    if is_debug {
        let _ = app.emit("launch-log", "");
        let _ = app.emit("launch-log", &format!("> Command: {} \"C:\\Program Files\\Roberts Space Industries\\RSI Launcher\\RSI Launcher.exe\"", wine.to_string_lossy()));
    }

    // RSI Launcher is an Electron app — file redirection keeps child handles
    // open and prevents exit detection. Use null handles instead.
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null());

    let _ = app.emit("launch-log", "");
    let _ = app.emit("launch-log", "> Starting RSI Launcher...");

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to launch RSI Launcher: {}", e))?;

    let pid = child.id();
    let _ = app.emit("launch-log", &format!("> RSI Launcher started (PID: {})", pid));
    let _ = app.emit("launch-started", "RSI Launcher process started");

    // Store PID + install path for stop_game
    if let Ok(mut guard) = GAME_PID.lock() {
        *guard = Some((pid, install_path.clone()));
    }

    // Monitor child process in background
    std::thread::spawn(move || {
        let status = child.wait();
        let code = status.ok().and_then(|s| s.code());

        // Clear stored PID
        if let Ok(mut guard) = GAME_PID.lock() {
            *guard = None;
        }

        let _ = app.emit("launch-log", "");
        let _ = app.emit("launch-log", &format!("> RSI Launcher exited (code: {:?})", code));
        let _ = app.emit("launch-exited", code.unwrap_or(-1));
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_game(app: AppHandle) -> Result<(), String> {
    let (pid, install_path) = {
        let guard = GAME_PID.lock().map_err(|e| format!("Lock error: {}", e))?;
        guard.clone().ok_or("No game process is running")?
    };

    let _ = app.emit("launch-log", "");
    let _ = app.emit("launch-log", &format!("> Stopping game process (PID: {})...", pid));

    // Kill the process tree
    let _ = Command::new("kill").arg("-TERM").arg(pid.to_string()).output();

    // Give it a moment, then force-kill if still alive
    std::thread::sleep(std::time::Duration::from_secs(2));
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();

    // Kill wineserver to clean up all Wine processes
    let runner_dirs = std::fs::read_dir(Path::new(&install_path).join("runners")).ok();
    if let Some(dirs) = runner_dirs {
        for entry in dirs.flatten() {
            let wineserver = entry.path().join("bin").join("wineserver");
            if wineserver.exists() {
                let _ = app.emit("launch-log", "> Killing wineserver...");
                let _ = Command::new(wineserver.to_string_lossy().as_ref())
                    .arg("-k")
                    .env("WINEPREFIX", &install_path)
                    .output();
            }
        }
    }

    // Clear stored PID
    if let Ok(mut guard) = GAME_PID.lock() {
        *guard = None;
    }

    let _ = app.emit("launch-log", "> Game stopped.");
    let _ = app.emit("launch-exited", -1);

    Ok(())
}

#[tauri::command]
pub async fn run_installation(app: AppHandle, config: AppConfig) -> Result<(), String> {
    INSTALL_CANCEL.store(false, Ordering::SeqCst);

    // Frontend may not pass github_token or install_mode — load from saved config if missing
    let config = if config.github_token.is_none() || config.install_mode.is_empty() {
        if let Ok(Some(saved)) = crate::config::load_config().await {
            AppConfig {
                github_token: if config.github_token.is_none() {
                    saved.github_token
                } else {
                    config.github_token
                },
                install_mode: if config.install_mode.is_empty() {
                    saved.install_mode
                } else {
                    config.install_mode
                },
                ..config
            }
        } else {
            config
        }
    } else {
        config
    };

    let install_path = expand_tilde(&config.install_path);
    let skip_launcher = config.install_mode == "quick";

    // In quick mode, verify that RSI Launcher actually exists
    if skip_launcher {
        let launcher_exe = Path::new(&install_path)
            .join("drive_c")
            .join("Program Files")
            .join("Roberts Space Industries")
            .join("RSI Launcher")
            .join("RSI Launcher.exe");
        if !launcher_exe.exists() {
            return Err("Quick Install selected but RSI Launcher not found. Please use Full Installation.".into());
        }
    }

    let runner_name = config
        .selected_runner
        .as_deref()
        .ok_or("No runner selected")?;

    let runner_bin = Path::new(&install_path)
        .join("runners")
        .join(runner_name)
        .join("bin");
    let wine = runner_bin.join("wine");
    let wineserver = runner_bin.join("wineserver");

    if !wine.exists() {
        return Err(format!(
            "Wine binary not found: {}",
            wine.to_string_lossy()
        ));
    }

    // ── Phase 1: Prepare (0–5%) ──
    // Note: In quick mode, we run Phases 1-3 but skip Phases 4-5 (RSI Launcher)

    let tmp_dir = Path::new(&install_path).join(".tmp");
    let client = reqwest::Client::builder()
        .user_agent("star-control/0.1.3")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    emit_progress(&app, "prepare", "Preparing environment...", 0.0, "Starting installation...");

    let live_dir = Path::new(&install_path).join("drive_c");

    std::fs::create_dir_all(&install_path)
        .map_err(|e| format!("Failed to create install directory: {}", e))?;
    std::fs::create_dir_all(&live_dir)
        .map_err(|e| format!("Failed to create drive_c directory: {}", e))?;
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| format!("Failed to create .tmp directory: {}", e))?;

        // Create no_win64_warnings early so winetricks skips the 64-bit warning
        let marker = Path::new(&install_path).join("no_win64_warnings");
        let _ = std::fs::write(&marker, "");

        // Kill any lingering wineserver from previous attempts
        emit_progress(&app, "prepare", "Cleaning up old processes...", 1.0, "Killing any lingering wineserver...");
        let _ = Command::new(wineserver.to_string_lossy().as_ref())
            .arg("-k")
            .env("WINEPREFIX", &install_path)
            .output();

        emit_progress(&app, "prepare", "Downloading winetricks...", 2.0, "Downloading winetricks...");

    let winetricks_path = tmp_dir.join("winetricks");

    let wt_bytes = client
        .get("https://raw.githubusercontent.com/Winetricks/winetricks/master/src/winetricks")
        .send()
        .await
        .map_err(|e| format!("Failed to download winetricks: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read winetricks response: {}", e))?;

    std::fs::write(&winetricks_path, &wt_bytes)
        .map_err(|e| format!("Failed to write winetricks: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&winetricks_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to chmod winetricks: {}", e))?;
    }

    emit_progress(&app, "prepare", "Environment ready", 5.0, "Environment prepared successfully");

    if is_cancelled() {
        return Err("Installation cancelled".into());
    }

    // ── Phase 2: Winetricks (5–45%) ──
    // DXVK is NOT installed via winetricks (ships outdated versions).
    // It is installed separately in Phase 3 using our own DXVK module.
    let verbs = ["win11", "arial", "tahoma", "powershell"];
    let verb_count = verbs.len() as f64;

    for (i, verb) in verbs.iter().enumerate() {
        if is_cancelled() {
            return Err("Installation cancelled".into());
        }

        let step_label = format!("Installing {}...", verb);
        let base_percent = 5.0 + (i as f64 / verb_count) * 40.0;

        // Kill wineserver before each verb to prevent hangs from lingering processes
        let _ = Command::new(wineserver.to_string_lossy().as_ref())
            .arg("-k")
            .env("WINEPREFIX", &install_path)
            .output();

        emit_progress(&app, "winetricks", &step_label, base_percent, &format!("Running: winetricks -q {}", verb));

        let mut child = Command::new(winetricks_path.to_string_lossy().as_ref())
            .args(["-q", verb])
            .env("WINEPREFIX", &install_path)
            .env("WINE", wine.to_string_lossy().as_ref())
            .env("WINESERVER", wineserver.to_string_lossy().as_ref())
            .env("WINEDLLOVERRIDES", "winemenubuilder.exe=d;winedbg.exe=d")
            .env("WINEDEBUG", "-all")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to run winetricks {}: {}", verb, e))?;

        stream_command_output(&app, "winetricks", &step_label, base_percent, &mut child);

        let status = child
            .wait()
            .map_err(|e| format!("Failed to wait for winetricks {}: {}", verb, e))?;

        if !status.success() {
            emit_progress(&app, "error", &format!("winetricks {} failed", verb), base_percent, &format!("winetricks {} exited with code {:?}", verb, status.code()));
            return Err(format!("winetricks {} failed with exit code {:?}", verb, status.code()));
        }

        let done_percent = 5.0 + ((i + 1) as f64 / verb_count) * 40.0;
        emit_progress(&app, "winetricks", &format!("{} installed", verb), done_percent, &format!("Completed: {}", verb));
    }

    if is_cancelled() {
        return Err("Installation cancelled".into());
    }

    // ── Phase 3: DXVK (45–60%) ──
    emit_progress(&app, "dxvk", "Installing DXVK...", 45.0, "Fetching latest DXVK release from GitHub...");

    // Kill wineserver before DXVK install
    let _ = Command::new(wineserver.to_string_lossy().as_ref())
        .arg("-k")
        .env("WINEPREFIX", &install_path)
        .output();

    // Fetch latest DXVK release
    let dxvk_url = "https://api.github.com/repos/doitsujin/dxvk/releases/latest";
    let mut dxvk_request = client.get(dxvk_url);

    // Use GitHub token if available
    if let Some(ref token) = config.github_token {
        dxvk_request = dxvk_request.header("Authorization", format!("Bearer {}", token));
    }

    let dxvk_resp = dxvk_request
        .send()
        .await
        .map_err(|e| format!("Failed to fetch DXVK release: {}", e))?;

    if !dxvk_resp.status().is_success() {
        return Err(format!("GitHub API returned {} for DXVK", dxvk_resp.status()));
    }

    #[derive(Deserialize)]
    struct GhRelease { tag_name: String, assets: Vec<GhAsset> }
    #[derive(Deserialize)]
    struct GhAsset { name: String, browser_download_url: String }

    let release: GhRelease = dxvk_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse DXVK release: {}", e))?;

    let dxvk_asset = release.assets.iter()
        .find(|a| a.name.ends_with(".tar.gz"))
        .ok_or("No .tar.gz asset found in DXVK release")?;

    let dxvk_version = release.tag_name.clone();
    emit_progress(&app, "dxvk", "Downloading DXVK...", 47.0, &format!("Downloading DXVK {}...", dxvk_version));

    // Download DXVK archive
    let mut dxvk_dl_request = client.get(&dxvk_asset.browser_download_url);
    if let Some(ref token) = config.github_token {
        dxvk_dl_request = dxvk_dl_request.header("Authorization", format!("Bearer {}", token));
    }
    let dxvk_bytes = dxvk_dl_request
        .send()
        .await
        .map_err(|e| format!("Failed to download DXVK: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read DXVK download: {}", e))?;

    let dxvk_archive_path = tmp_dir.join("dxvk.tar.gz");
    std::fs::write(&dxvk_archive_path, &dxvk_bytes)
        .map_err(|e| format!("Failed to save DXVK archive: {}", e))?;

    emit_progress(&app, "dxvk", "Extracting DXVK...", 52.0, "Extracting DXVK archive...");

    // Extract
    let extract_dir = tmp_dir.join("dxvk-extract");
    std::fs::create_dir_all(&extract_dir)
        .map_err(|e| format!("Failed to create extract dir: {}", e))?;

    {
        let file = std::fs::File::open(&dxvk_archive_path)
            .map_err(|e| format!("Failed to open DXVK archive: {}", e))?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(&extract_dir)
            .map_err(|e| format!("Failed to extract DXVK: {}", e))?;
    }

    emit_progress(&app, "dxvk", "Installing DXVK DLLs...", 55.0, "Copying DLLs to Wine prefix...");

    // Find extracted dir (usually dxvk-X.Y.Z/)
    let dxvk_inner = std::fs::read_dir(&extract_dir)
        .map_err(|e| format!("Failed to read extract dir: {}", e))?
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .map(|e| e.path())
        .unwrap_or(extract_dir.clone());

    let dll_names = ["d3d9.dll", "d3d10core.dll", "d3d11.dll", "dxgi.dll"];

    // Copy x64 DLLs to system32
    let sys32 = Path::new(&install_path).join("drive_c").join("windows").join("system32");
    std::fs::create_dir_all(&sys32).ok();
    let x64_dir = dxvk_inner.join("x64");
    if x64_dir.is_dir() {
        for name in &dll_names {
            let src = x64_dir.join(name);
            if src.exists() {
                let _ = std::fs::copy(&src, sys32.join(name));
            }
        }
    }

    // Copy x32 DLLs to syswow64
    let syswow64 = Path::new(&install_path).join("drive_c").join("windows").join("syswow64");
    std::fs::create_dir_all(&syswow64).ok();
    let x32_dir = dxvk_inner.join("x32");
    if x32_dir.is_dir() {
        for name in &dll_names {
            let src = x32_dir.join(name);
            if src.exists() {
                let _ = std::fs::copy(&src, syswow64.join(name));
            }
        }
    }

    // Register DLL overrides in Wine registry so Wine uses native DXVK DLLs
    emit_progress(&app, "dxvk", "Registering DXVK DLLs...", 57.0, "Setting DLL overrides in registry...");
    for name in &["d3d9", "d3d10core", "d3d11", "dxgi"] {
        let _ = Command::new(wine.to_string_lossy().as_ref())
            .args([
                "reg", "add",
                "HKEY_CURRENT_USER\\Software\\Wine\\DllOverrides",
                "/v", name, "/d", "native", "/f",
            ])
            .env("WINEPREFIX", &install_path)
            .env("WINEDLLOVERRIDES", "winemenubuilder.exe=d;winedbg.exe=d")
            .env("WINEDEBUG", "-all")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    // Write version marker
    let dxvk_marker = Path::new(&install_path).join(".dxvk_version");
    let _ = std::fs::write(&dxvk_marker, &dxvk_version);

    // Cleanup DXVK temp files
    let _ = std::fs::remove_file(&dxvk_archive_path);
    let _ = std::fs::remove_dir_all(&extract_dir);

    emit_progress(&app, "dxvk", "DXVK installed", 60.0, &format!("DXVK {} installed successfully", dxvk_version));

    if is_cancelled() {
        return Err("Installation cancelled".into());
    }

    // ── Phase 4: Registry (60–65%) ──
    emit_progress(&app, "registry", "Configuring registry...", 60.0, "Setting registry keys...");

    let mut reg_child = Command::new(wine.to_string_lossy().as_ref())
        .args([
            "reg", "add",
            "HKEY_CURRENT_USER\\Software\\Wine\\FileOpenAssociations",
            "/v", "Enable", "/d", "N", "/f",
        ])
        .env("WINEPREFIX", &install_path)
        .env("WINEDLLOVERRIDES", "winemenubuilder.exe=d;winedbg.exe=d")
        .env("WINEDEBUG", "-all")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run wine reg: {}", e))?;

    stream_command_output(&app, "registry", "Setting registry keys...", 62.0, &mut reg_child);

    let reg_status = reg_child.wait()
        .map_err(|e| format!("Failed to wait for wine reg: {}", e))?;

    if !reg_status.success() {
        emit_progress(&app, "registry", "Registry warning", 63.0, "Registry key set returned non-zero (continuing anyway)");
    }

    emit_progress(&app, "registry", "Registry configured", 64.0, "Registry configuration complete");

    // Kill wineserver after registry to prevent lingering Wine processes
    // from holding pipe handles open during the async download phase
    let _ = Command::new(wineserver.to_string_lossy().as_ref())
        .arg("-k")
        .env("WINEPREFIX", &install_path)
        .output();
    std::thread::sleep(std::time::Duration::from_secs(1));

    emit_progress(&app, "registry", "Registry complete", 65.0, "Wine processes cleaned up");

    if is_cancelled() {
        return Err("Installation cancelled".into());
    }

    // ── Phase 4 & 5: RSI Launcher Download & Install (65–95%) ──
    // Skip if install_mode is "quick" (RSI Launcher already exists)
    if skip_launcher {
        emit_progress(&app, "launcher_skip", "Skipping RSI Launcher...", 65.0,
            "Quick Install: RSI Launcher already exists, skipping download and install");
    } else {
        // ── Phase 4: Download RSI Launcher (65–85%) ──
        emit_progress(&app, "download", "Fetching launcher info...", 65.0, "Downloading latest.yml...");

    let latest_yml = client
        .get("https://install.robertsspaceindustries.com/rel/2/latest.yml")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch latest.yml: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to read latest.yml: {}", e))?;

    // Extract filename from "path:" line (top-level field in latest.yml)
    let installer_filename = latest_yml
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            // Try top-level "path: filename.exe" first
            if let Some(val) = trimmed.strip_prefix("path:") {
                return Some(val.trim().to_string());
            }
            // Fallback: nested "- url: filename.exe"
            let stripped = trimmed.strip_prefix('-').map(|s| s.trim()).unwrap_or(trimmed);
            if let Some(val) = stripped.strip_prefix("url:") {
                return Some(val.trim().to_string());
            }
            None
        })
        .filter(|s| s.ends_with(".exe"))
        .ok_or("Could not find installer filename in latest.yml")?;

    let download_url = format!(
        "https://install.robertsspaceindustries.com/rel/2/{}",
        installer_filename
    );

    emit_progress(&app, "download", "Downloading RSI Launcher...", 67.0, &format!("Downloading {}", installer_filename));

    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download RSI Launcher: {}", e))?;

    let total_bytes = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let installer_path = tmp_dir.join(&installer_filename);

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::File::create(&installer_path)
        .await
        .map_err(|e| format!("Failed to create installer file: {}", e))?;

    let mut stream = response.bytes_stream();
    let mut last_emit = std::time::Instant::now();
    while let Some(chunk_result) = stream.next().await {
        if is_cancelled() {
            let _ = file.flush().await;
            drop(file);
            return Err("Installation cancelled".into());
        }

        match chunk_result {
            Ok(chunk) => {
                file.write_all(&chunk)
                    .await
                    .map_err(|e| format!("Failed to write installer chunk: {}", e))?;
                downloaded += chunk.len() as u64;

                // Throttle progress events to max once per 500ms to avoid flooding the UI
                let is_complete = total_bytes > 0 && downloaded >= total_bytes;
                if is_complete || last_emit.elapsed() >= std::time::Duration::from_millis(500) {
                    let dl_percent = if total_bytes > 0 {
                        65.0 + (downloaded as f64 / total_bytes as f64) * 20.0
                    } else {
                        75.0
                    };
                    let status_msg = if total_bytes > 0 {
                        format!(
                            "Downloading... {:.1} MB / {:.1} MB",
                            downloaded as f64 / 1_048_576.0,
                            total_bytes as f64 / 1_048_576.0
                        )
                    } else {
                        format!("Downloading... {:.1} MB", downloaded as f64 / 1_048_576.0)
                    };
                    emit_progress(&app, "download", "Downloading RSI Launcher...", dl_percent, &status_msg);
                    last_emit = std::time::Instant::now();
                }
            }
            Err(e) => {
                return Err(format!("Download stream error: {}", e));
            }
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush installer file: {}", e))?;
    drop(file);

    emit_progress(&app, "download", "Download complete", 85.0, "RSI Launcher downloaded successfully");

    if is_cancelled() {
        return Err("Installation cancelled".into());
    }

    // ── Phase 5: Install RSI Launcher (85–95%) ──
    emit_progress(&app, "install", "Installing RSI Launcher...", 85.0, &format!("Running: wine {} /S", installer_filename));

    // The NSIS installer with /S auto-launches RSI Launcher as a child process.
    // Use Stdio::null() to prevent pipe-handle inheritance.
    // Use try_wait() with timeout because the NSIS process may not exit until
    // the RSI Launcher it spawned exits (Wine keeps parent alive).
    let mut install_child = Command::new(wine.to_string_lossy().as_ref())
        .arg(installer_path.to_string_lossy().as_ref())
        .arg("/S")
        .env("WINEPREFIX", &install_path)
        .env("WINEDLLOVERRIDES", "winemenubuilder.exe=d;winedbg.exe=d")
        .env("WINEDEBUG", "-all")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to run RSI Launcher installer: {}", e))?;

    emit_progress(&app, "install", "Installing RSI Launcher...", 88.0, "Waiting for installer to finish...");

    // Poll with timeout - the NSIS installer may block if it waits for its child (RSI Launcher)
    let install_timeout = std::time::Duration::from_secs(120);
    let install_start = std::time::Instant::now();
    let mut timed_out = false;

    loop {
        match install_child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    emit_progress(&app, "install", "Install warning", 92.0,
                        &format!("Installer exited with code {:?} (may be normal)", status.code()));
                }
                break;
            }
            Ok(None) => {
                // Check if RSI Launcher exe was installed (installer is done, just hasn't exited)
                let launcher_exe = Path::new(&install_path)
                    .join("drive_c/Program Files/Roberts Space Industries/RSI Launcher/RSI Launcher.exe");
                if install_start.elapsed() > std::time::Duration::from_secs(15) && launcher_exe.exists() {
                    emit_progress(&app, "install", "Installing RSI Launcher...", 92.0,
                        "RSI Launcher installed, stopping installer process...");
                    let _ = install_child.kill();
                    let _ = install_child.wait();
                    timed_out = true;
                    break;
                }

                if install_start.elapsed() > install_timeout {
                    emit_progress(&app, "install", "Install timeout", 92.0,
                        "Installer timed out, killing process...");
                    let _ = install_child.kill();
                    let _ = install_child.wait();
                    timed_out = true;
                    break;
                }

                std::thread::sleep(std::time::Duration::from_secs(1));
                let elapsed = install_start.elapsed().as_secs();
                emit_progress(&app, "install", "Installing RSI Launcher...",
                    88.0 + (elapsed as f64 / 120.0) * 4.0,
                    &format!("Waiting for installer... ({}s)", elapsed));
            }
            Err(e) => {
                emit_progress(&app, "install", "Install warning", 92.0,
                    &format!("Could not check installer status: {}", e));
                break;
            }
        }
    }

    if timed_out {
        emit_progress(&app, "install", "Cleaning up...", 93.0, "Killing Wine processes from installer...");
    } else {
        emit_progress(&app, "install", "Cleaning up...", 93.0, "Stopping installer-spawned processes...");
    }

    // Kill wineserver to stop any processes the installer auto-launched
    let _ = Command::new(wineserver.to_string_lossy().as_ref())
        .arg("-k")
        .env("WINEPREFIX", &install_path)
        .output();

    // Give wineserver a moment to shut everything down
    std::thread::sleep(std::time::Duration::from_secs(2));

    emit_progress(&app, "install", "Installation complete", 95.0, "RSI Launcher installed successfully");
    } // End of else (non-quick mode for Phase 4/5)

    // Cleanup tmp_dir if it was created (in non-quick mode)
    if !skip_launcher {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    if is_cancelled() {
        return Err("Installation cancelled".into());
    }

    // ── Phase 6: Launch (95–100%) ──
    emit_progress(&app, "launch", "Launching RSI Launcher...", 95.0, "Preparing launch environment...");

    let mut cmd = Command::new(wine.to_string_lossy().as_ref());
    cmd.arg("C:\\Program Files\\Roberts Space Industries\\RSI Launcher\\RSI Launcher.exe");

    let _ = configure_wine_env(&mut cmd, &install_path, &config.performance, "info");

    cmd.stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to launch RSI Launcher: {}", e))?;

    let pid = child.id();

    // Store PID so the Launch page and stop_game know the process is running
    if let Ok(mut guard) = GAME_PID.lock() {
        *guard = Some((pid, install_path.clone()));
    }

    let _ = app.emit("launch-started", "RSI Launcher process started");
    let _ = app.emit("launch-log", &format!("> RSI Launcher started (PID: {})", pid));

    emit_progress(&app, "complete", "RSI Launcher started", 100.0, "RSI Launcher is now running");

    // Monitor child process in background — emit exit event when done
    let bg_app = app.clone();
    std::thread::spawn(move || {
        let status = child.wait();
        let code = status.ok().and_then(|s| s.code());

        if let Ok(mut guard) = GAME_PID.lock() {
            *guard = None;
        }

        let _ = bg_app.emit("launch-log", &format!("> RSI Launcher exited (code: {:?})", code));
        let _ = bg_app.emit("launch-exited", code.unwrap_or(-1));
    });

    Ok(())
}

#[tauri::command]
pub fn cancel_installation() -> bool {
    INSTALL_CANCEL.store(true, Ordering::SeqCst);
    true
}

#[tauri::command]
pub fn is_game_running() -> bool {
    GAME_PID
        .lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false)
}
