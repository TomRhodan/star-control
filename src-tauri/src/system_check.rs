use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CheckResult {
    id: String,
    name: String,
    status: CheckStatus,
    detail: String,
    fixable: bool,
}

#[derive(Serialize, Deserialize)]
pub struct SystemCheckResult {
    checks: Vec<CheckResult>,
    all_passed: bool,
    has_warnings: bool,
}

#[derive(Serialize, Deserialize)]
pub struct FixResult {
    success: bool,
    message: String,
}

// --- Individual checks ---

fn check_memory() -> CheckResult {
    let mut ram_kb: u64 = 0;
    let mut swap_kb: u64 = 0;

    if let Ok(contents) = fs::read_to_string("/proc/meminfo") {
        for line in contents.lines() {
            if line.starts_with("MemTotal:") {
                ram_kb = parse_meminfo_value(line);
            } else if line.starts_with("SwapTotal:") {
                swap_kb = parse_meminfo_value(line);
            }
        }
    }

    let ram_gib = ram_kb as f64 / 1_048_576.0;
    let swap_gib = swap_kb as f64 / 1_048_576.0;
    let combined_gib = ram_gib + swap_gib;

    let status = if ram_gib < 16.0 {
        CheckStatus::Fail
    } else if combined_gib < 40.0 {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    };

    let detail = format!(
        "{:.0} GiB RAM + {:.0} GiB Swap = {:.0} GiB total",
        ram_gib, swap_gib, combined_gib
    );

    CheckResult {
        id: "memory".into(),
        name: "Memory".into(),
        status,
        detail,
        fixable: false,
    }
}

fn parse_meminfo_value(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
}

fn check_avx() -> CheckResult {
    let has_avx = fs::read_to_string("/proc/cpuinfo")
        .map(|contents| {
            contents
                .lines()
                .any(|line| line.starts_with("flags") && line.contains(" avx"))
        })
        .unwrap_or(false);

    CheckResult {
        id: "avx".into(),
        name: "AVX Support".into(),
        status: if has_avx {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        detail: if has_avx {
            "CPU supports AVX instructions".into()
        } else {
            "CPU does not support AVX — required by Star Citizen".into()
        },
        fixable: false,
    }
}

fn check_mapcount() -> CheckResult {
    let required: u64 = 16_777_216;

    let current = fs::read_to_string("/proc/sys/vm/max_map_count")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);

    let status = if current >= required {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };

    let detail = format!(
        "Current: {} (required: {})",
        format_number(current),
        format_number(required)
    );

    let fixable = status == CheckStatus::Fail;

    CheckResult {
        id: "mapcount".into(),
        name: "vm.max_map_count".into(),
        status,
        detail,
        fixable,
    }
}

fn check_filelimit() -> CheckResult {
    let required: u64 = 524_288;

    let mut rlim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };

    let hard_limit = unsafe {
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut rlim) == 0 {
            rlim.rlim_max
        } else {
            0
        }
    };

    let status = if hard_limit >= required {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };

    let detail = format!(
        "Hard limit: {} (required: {})",
        format_number(hard_limit),
        format_number(required)
    );

    let fixable = status == CheckStatus::Fail;

    CheckResult {
        id: "filelimit".into(),
        name: "File Descriptor Limit".into(),
        status,
        detail,
        fixable,
    }
}

fn check_vulkan() -> CheckResult {
    let has_vulkaninfo = Path::new("/usr/bin/vulkaninfo").exists();
    let has_libvulkan = Path::new("/usr/lib/libvulkan.so.1").exists()
        || Path::new("/usr/lib64/libvulkan.so.1").exists()
        || Path::new("/usr/lib/x86_64-linux-gnu/libvulkan.so.1").exists();

    let detected = has_vulkaninfo || has_libvulkan;

    CheckResult {
        id: "vulkan".into(),
        name: "Vulkan Support".into(),
        status: if detected {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        detail: if has_vulkaninfo {
            "vulkaninfo found".into()
        } else if has_libvulkan {
            "libvulkan.so.1 found (vulkaninfo not installed)".into()
        } else {
            "No Vulkan runtime detected — install your GPU's Vulkan driver".into()
        },
        fixable: false,
    }
}

fn check_disk_space(install_path: &str) -> CheckResult {
    let required_gb: u64 = 100;

    let path = if install_path.is_empty() {
        get_default_install_path_inner()
    } else {
        install_path.to_string()
    };

    // Walk up the path to find an existing directory for statvfs
    let check_path = find_existing_parent(&path);

    let free_gb = get_free_space_gb(&check_path);

    let status = if free_gb >= required_gb {
        CheckStatus::Pass
    } else {
        CheckStatus::Fail
    };

    let detail = format!(
        "{} GB free at {} (required: {} GB)",
        free_gb, check_path, required_gb
    );

    CheckResult {
        id: "diskspace".into(),
        name: "Disk Space".into(),
        status,
        detail,
        fixable: false,
    }
}

fn find_existing_parent(path: &str) -> String {
    let mut p = Path::new(path);
    while !p.exists() {
        match p.parent() {
            Some(parent) => p = parent,
            None => return "/".into(),
        }
    }
    p.to_string_lossy().into_owned()
}

fn get_free_space_gb(path: &str) -> u64 {
    use std::ffi::CString;

    let c_path = match CString::new(path) {
        Ok(p) => p,
        Err(_) => return 0,
    };

    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) == 0 {
            let free_bytes = stat.f_bavail as u64 * stat.f_frsize as u64;
            free_bytes / (1024 * 1024 * 1024)
        } else {
            0
        }
    }
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

fn get_default_install_path_inner() -> String {
    if let Ok(home) = std::env::var("HOME") {
        format!("{}/Games/star-citizen", home)
    } else {
        "/tmp/star-citizen".into()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MonitorInfo {
    pub name: String,
    pub resolution: String,
    pub primary: bool,
    pub scale: Option<f64>,
}

// --- Tauri Commands ---

#[tauri::command]
pub async fn detect_monitors() -> Result<Vec<MonitorInfo>, String> {
    tokio::task::spawn_blocking(move || {
        let is_wayland = std::env::var("XDG_SESSION_TYPE")
            .map(|v| v == "wayland")
            .unwrap_or(false);

        if is_wayland {
            // Try Wayland-native detection (KDE → GNOME → wlroots → xrandr)
            if let Some(monitors) = detect_monitors_kscreen() {
                if !monitors.is_empty() {
                    return monitors;
                }
            }
            if let Some(monitors) = detect_monitors_gnome() {
                if !monitors.is_empty() {
                    return monitors;
                }
            }
            if let Some(monitors) = detect_monitors_wlr_randr() {
                if !monitors.is_empty() {
                    return monitors;
                }
            }
        }

        // Fallback: xrandr (works on X11 and XWayland)
        detect_monitors_xrandr()
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))
}

/// KDE Plasma Wayland: `kscreen-doctor --outputs`
fn detect_monitors_kscreen() -> Option<Vec<MonitorInfo>> {
    let output = Command::new("kscreen-doctor")
        .arg("--outputs")
        .env("LANG", "C")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    // Strip ANSI escape codes
    let raw = String::from_utf8_lossy(&output.stdout);
    let stdout = strip_ansi(&raw);
    let lines: Vec<&str> = stdout.lines().collect();
    let mut monitors = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        // "Output: 1 DP-1 <uuid>"
        if line.starts_with("Output:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let name = parts[2].to_string();
                let mut enabled = false;
                let mut connected = false;
                let mut primary = false;
                let mut resolution = String::new();
                let mut scale: Option<f64> = None;

                // Parse subsequent indented lines for this output
                i += 1;
                while i < lines.len() {
                    let sub = lines[i].trim();
                    if sub.starts_with("Output:") {
                        break; // Next output block
                    }
                    if sub == "enabled" {
                        enabled = true;
                    } else if sub == "connected" {
                        connected = true;
                    } else if sub.starts_with("priority") {
                        if let Some(val) = sub.split_whitespace().nth(1) {
                            primary = val == "1";
                        }
                    } else if sub.starts_with("Geometry:") {
                        // "Geometry: 2560,0 2560x1440"
                        if let Some(geom) = sub.split_whitespace().nth(2) {
                            resolution = geom.to_string();
                        }
                    } else if sub.starts_with("Scale:") {
                        if let Some(val) = sub.split_whitespace().nth(1) {
                            scale = val.parse().ok();
                        }
                    }
                    i += 1;
                }

                if enabled && connected {
                    monitors.push(MonitorInfo {
                        name,
                        resolution,
                        primary,
                        scale,
                    });
                }
                continue; // Don't increment i again
            }
        }
        i += 1;
    }

    Some(monitors)
}

/// GNOME Wayland: `gnome-monitor-config list`
fn detect_monitors_gnome() -> Option<Vec<MonitorInfo>> {
    let output = Command::new("gnome-monitor-config")
        .arg("list")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    let mut monitors = Vec::new();

    // gnome-monitor-config list output format:
    // Logical monitor 0: x=0 y=0 scale=1 transform=normal PRIMARY
    //   DP-1 []: 2560x1440@143.91 ...
    // Logical monitor 1: x=2560 y=0 scale=1.5 transform=normal
    //   DP-2 []: 3840x2160@60.00 ...

    let mut current_primary;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("Logical monitor") {
            current_primary = line.contains("PRIMARY");
            // Next indented line(s) are the physical outputs
            i += 1;
            while i < lines.len() {
                let sub = lines[i].trim();
                if sub.is_empty() || lines[i].starts_with("Logical monitor") {
                    break;
                }
                // "DP-1 [LG Electronics ...]: 2560x1440@143.91 ..."
                let parts: Vec<&str> = sub.splitn(2, ' ').collect();
                if !parts.is_empty() {
                    let name = parts[0].to_string();
                    // Extract resolution: find "WxH@rate" pattern
                    let mut resolution = String::new();
                    if let Some(rest) = parts.get(1) {
                        for token in rest.split_whitespace() {
                            if token.contains('x') && token.contains('@') {
                                // "2560x1440@143.91" → "2560x1440"
                                if let Some(res) = token.split('@').next() {
                                    resolution = res.to_string();
                                }
                                break;
                            }
                        }
                    }
                    monitors.push(MonitorInfo {
                        name,
                        resolution,
                        primary: current_primary,
                        scale: None,
                    });
                }
                i += 1;
            }
            continue;
        }
        i += 1;
    }

    Some(monitors)
}

/// wlroots compositors (Sway, Hyprland): `wlr-randr`
fn detect_monitors_wlr_randr() -> Option<Vec<MonitorInfo>> {
    let output = Command::new("wlr-randr").output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    let mut monitors = Vec::new();

    let mut current_name = String::new();
    let mut current_resolution = String::new();

    for line in &lines {
        let trimmed = line.trim();
        // Output line: "DP-1 ..." (not indented)
        if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            // Save previous monitor
            if !current_name.is_empty() {
                monitors.push(MonitorInfo {
                    name: current_name.clone(),
                    resolution: current_resolution.clone(),
                    primary: monitors.is_empty(),
                    scale: None,
                });
            }
            current_name = trimmed.split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            current_resolution = String::new();
        } else if current_resolution.is_empty() && trimmed.contains("current") {
            // "  2560x1440 px, 59.951 Hz (current)"
            if let Some(res) = trimmed.split_whitespace().next() {
                current_resolution = res.to_string();
            }
        }
    }

    // Last monitor
    if !current_name.is_empty() {
        monitors.push(MonitorInfo {
            name: current_name,
            resolution: current_resolution,
            primary: monitors.is_empty(),
            scale: None,
        });
    }

    Some(monitors)
}

/// X11 / XWayland fallback: `xrandr --query`
fn detect_monitors_xrandr() -> Vec<MonitorInfo> {
    let output = match Command::new("xrandr").arg("--query").output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    if !output.status.success() {
        return Vec::new();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    let mut monitors = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if !line.contains(" connected") {
            continue;
        }

        let name = match line.split_whitespace().next() {
            Some(n) => n.to_string(),
            None => continue,
        };

        let primary = line.contains(" primary ");

        let mut resolution = String::new();
        for j in (i + 1)..lines.len() {
            let mode_line = lines[j].trim();
            if mode_line.contains(" connected") || mode_line.contains(" disconnected") {
                break;
            }
            if mode_line.contains('*') {
                if let Some(res) = mode_line.split_whitespace().next() {
                    resolution = res.to_string();
                }
                break;
            }
        }

        monitors.push(MonitorInfo {
            name,
            resolution,
            primary,
            scale: None,
        });
    }

    monitors
}

/// Remove ANSI escape sequences from a string
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip until we hit a letter (end of escape sequence)
            while let Some(&next) = chars.peek() {
                chars.next();
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[tauri::command]
pub async fn run_system_check(install_path: String) -> Result<SystemCheckResult, String> {
    tokio::task::spawn_blocking(move || {
        let checks = vec![
            check_memory(),
            check_avx(),
            check_mapcount(),
            check_filelimit(),
            check_vulkan(),
            check_disk_space(&install_path),
        ];

        let all_passed = checks.iter().all(|c| c.status != CheckStatus::Fail);
        let has_warnings = checks.iter().any(|c| c.status == CheckStatus::Warn);

        SystemCheckResult {
            checks,
            all_passed,
            has_warnings,
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn fix_mapcount() -> Result<FixResult, String> {
    tokio::task::spawn_blocking(move || {
        // Check if pkexec is available
        if !Path::new("/usr/bin/pkexec").exists() {
            return FixResult {
                success: false,
                message: "pkexec not found. Manually run: sudo sysctl -w vm.max_map_count=16777216 && echo 'vm.max_map_count = 16777216' | sudo tee /etc/sysctl.d/99-starcitizen-max_map_count.conf".into(),
            };
        }

        let result = Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg("printf 'vm.max_map_count = 16777216\\n' > /etc/sysctl.d/99-starcitizen-max_map_count.conf && sysctl --quiet --system")
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    FixResult {
                        success: true,
                        message: "vm.max_map_count set to 16,777,216 (persistent)".into(),
                    }
                } else {
                    let code = output.status.code().unwrap_or(-1);
                    if code == 126 || code == 127 {
                        FixResult {
                            success: false,
                            message: "Authentication cancelled".into(),
                        }
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                        FixResult {
                            success: false,
                            message: format!("Failed (exit {}): {}", code, stderr.trim()),
                        }
                    }
                }
            }
            Err(e) => FixResult {
                success: false,
                message: format!("Failed to execute pkexec: {}", e),
            },
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn fix_filelimit() -> Result<FixResult, String> {
    tokio::task::spawn_blocking(move || {
        if !Path::new("/usr/bin/pkexec").exists() {
            return FixResult {
                success: false,
                message: "pkexec not found. Manually create /etc/systemd/system.conf.d/99-starcitizen-filelimit.conf with:\n[Manager]\nDefaultLimitNOFILE=524288".into(),
            };
        }

        let result = Command::new("pkexec")
            .arg("sh")
            .arg("-c")
            .arg("mkdir -p /etc/systemd/system.conf.d && printf '[Manager]\\nDefaultLimitNOFILE=524288\\n' > /etc/systemd/system.conf.d/99-starcitizen-filelimit.conf && systemctl daemon-reexec")
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    FixResult {
                        success: true,
                        message: "File descriptor limit set to 524,288 (persistent, effective after re-login)".into(),
                    }
                } else {
                    let code = output.status.code().unwrap_or(-1);
                    if code == 126 || code == 127 {
                        FixResult {
                            success: false,
                            message: "Authentication cancelled".into(),
                        }
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                        FixResult {
                            success: false,
                            message: format!("Failed (exit {}): {}", code, stderr.trim()),
                        }
                    }
                }
            }
            Err(e) => FixResult {
                success: false,
                message: format!("Failed to execute pkexec: {}", e),
            },
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn get_default_install_path() -> String {
    get_default_install_path_inner()
}
