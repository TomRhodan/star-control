//! Configuration management module for Star Control.
//!
//! This module handles all application configuration including:
//! - Performance settings (esync, fsync, DXVK, etc.)
//! - Runner sources (Wine/Proton repositories)
//! - Installation settings
//! - Caching of runner and DXVK data
//!
//! Configuration is stored in `~/.config/star-control/config.json`
//! Cache is stored in `~/.config/star-control/cache.json`

use serde::{ Deserialize, Serialize };
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// Performance-related settings for Wine/Star Citizen execution.
/// These settings control various Wine features and overlays.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct PerformanceSettings {
    pub esync: bool,
    pub fsync: bool,
    pub dxvk_async: bool,
    pub mangohud: bool,
    pub dxvk_hud: bool,
    pub wayland: bool,
    pub hdr: bool,
    pub fsr: bool,
    pub primary_monitor: Option<String>,
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            esync: true,
            fsync: true,
            dxvk_async: true,
            mangohud: false,
            dxvk_hud: false,
            wayland: true,
            hdr: false,
            fsr: false,
            primary_monitor: None,
        }
    }
}

/// Configuration for a Wine/Proton runner source.
/// Runner sources are GitHub repositories that provide pre-built Wine builds.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct RunnerSourceConfig {
    pub name: String,
    pub api_url: String,
    pub filter: Option<String>, // "all", "kron4ek"
    pub enabled: bool,
}

impl Default for RunnerSourceConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            api_url: String::new(),
            filter: None,
            enabled: true,
        }
    }
}

/// Main application configuration structure.
/// Contains all settings needed to run Star Citizen.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub install_path: String,
    pub selected_runner: Option<String>,
    pub performance: PerformanceSettings,
    pub github_token: Option<String>,
    pub log_level: String,
    pub auto_backup_on_launch: Option<bool>,
    pub runner_sources: Vec<RunnerSourceConfig>,
    pub install_mode: String, // "full" | "quick"
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            install_path: String::new(),
            selected_runner: None,
            performance: PerformanceSettings::default(),
            github_token: None,
            log_level: "info".to_string(),
            auto_backup_on_launch: None,
            runner_sources: vec![],
            install_mode: "full".to_string(),
        }
    }
}

// --- Cached Data Structures ---

/// Cached information about an available runner.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct CachedRunner {
    pub name: String,
    pub source: String,
    pub version: String,
    pub download_url: String,
    pub file_name: String,
    pub size_bytes: u64,
}

/// Cached information about an available DXVK release.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct CachedDxvkRelease {
    pub version: String,
    pub download_url: String,
    pub file_name: String,
    pub size_bytes: u64,
}

/// Cache container for runner data.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct RunnerCache {
    pub runners: Vec<CachedRunner>,
    pub cached_at: u64, // Unix timestamp
}

/// Cache container for DXVK release data.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct DxvkCache {
    pub releases: Vec<CachedDxvkRelease>,
    pub cached_at: u64,
}

/// Combined application cache containing both runner and DXVK data.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct AppCache {
    pub runners: RunnerCache,
    pub dxvk: DxvkCache,
}

/// Returns the path to the cache file.
fn cache_file_path() -> Option<String> {
    dirs::config_dir().map(|p| {
        p.join("star-control").join("cache.json").to_string_lossy().into_owned()
    })
}

/// Result of validating an installation path.
#[derive(Serialize, Deserialize)]
pub struct PathValidation {
    pub valid: bool,
    pub writable: bool,
    pub free_space_gb: u64,
    pub space_sufficient: bool,
    pub message: String,
}

/// Information about a detected runner in the local installation directory.
#[derive(Serialize, Deserialize, Clone)]
pub struct DetectedRunner {
    pub name: String,
    pub bin_path: String,
    pub wine_executable: String,
}

/// Result of scanning for local runners.
#[derive(Serialize, Deserialize)]
pub struct ScanRunnersResult {
    pub runners: Vec<DetectedRunner>,
    pub runners_dir: String,
}

/// Expands tilde (~) in a path to the user's home directory.
fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen('~', &home, 1);
        }
    }
    path.to_string()
}

/// Finds the first existing parent directory in a path.
fn find_existing_parent(path: &str) -> String {
    let mut p = Path::new(path);
    while !p.exists() {
        match p.parent() {
            Some(parent) => {
                p = parent;
            }
            None => {
                return "/".into();
            }
        }
    }
    p.to_string_lossy().into_owned()
}

/// Gets the free disk space in gigabytes for a given path.
fn get_free_space_gb(path: &str) -> u64 {
    use std::ffi::CString;

    let c_path = match CString::new(path) {
        Ok(p) => p,
        Err(_) => {
            return 0;
        }
    };

    // SAFETY: c_path is a valid, NUL-terminated CString. statvfs is initialized to zero
    // and only read after a successful statvfs() call. No aliasing or lifetime issues.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), &mut stat) == 0 {
            let free_bytes = stat.f_bavail * stat.f_frsize;
            free_bytes / (1024 * 1024 * 1024)
        } else {
            0
        }
    }
}

/// Checks if a path is writable by the current user.
fn is_writable(path: &str) -> bool {
    let p = Path::new(path);
    if !p.exists() {
        return false;
    }
    match fs::metadata(p) {
        Ok(meta) => {
            let mode = meta.mode();
            let uid = meta.uid();
            let gid = meta.gid();
            // SAFETY: getuid/getgid are always safe to call, have no preconditions
            let my_uid = unsafe { libc::getuid() };
            let my_gid = unsafe { libc::getgid() };

            if my_uid == 0 {
                return true;
            }
            if uid == my_uid {
                return (mode & 0o200) != 0;
            }
            if gid == my_gid {
                return (mode & 0o020) != 0;
            }
            (mode & 0o002) != 0
        }
        Err(_) => false,
    }
}

/// Returns the path to the configuration file.
fn config_file_path() -> Option<String> {
    dirs::config_dir().map(|p| {
        p.join("star-control").join("config.json").to_string_lossy().into_owned()
    })
}

/// Result of checking if initial setup is required.
#[derive(Serialize, Deserialize)]
pub struct SetupCheck {
    pub needs_setup: bool,
    pub default_path: String,
}

// --- Tauri Commands ---

#[tauri::command]
pub async fn check_needs_setup() -> Result<SetupCheck, String> {
    tokio::task
        ::spawn_blocking(move || {
            let default_path = if let Ok(home) = std::env::var("HOME") {
                format!("{}/Games/star-citizen", home)
            } else {
                "/tmp/star-citizen".into()
            };

            // Check if config exists and has a valid install_path
            let has_valid_install = config_file_path()
                .and_then(|p| fs::read_to_string(p).ok())
                .and_then(|c| serde_json::from_str::<AppConfig>(&c).ok())
                .map(|cfg| {
                    let path = expand_tilde(&cfg.install_path);
                    !path.is_empty() && Path::new(&path).exists()
                })
                .unwrap_or(false);

            SetupCheck {
                needs_setup: !has_valid_install,
                default_path,
            }
        }).await
        .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn create_install_directory(path: String) -> Result<(), String> {
    tokio::task
        ::spawn_blocking(move || {
            let expanded = expand_tilde(&path);
            fs::create_dir_all(&expanded).map_err(|e| format!("Failed to create directory: {}", e))
        }).await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn validate_install_path(path: String) -> Result<PathValidation, String> {
    tokio::task
        ::spawn_blocking(move || {
            let expanded = expand_tilde(&path);

            if expanded.is_empty() {
                return PathValidation {
                    valid: false,
                    writable: false,
                    free_space_gb: 0,
                    space_sufficient: false,
                    message: "Path cannot be empty".into(),
                };
            }

            let existing_parent = find_existing_parent(&expanded);
            let writable = is_writable(&existing_parent);
            let free_space_gb = get_free_space_gb(&existing_parent);
            let space_sufficient = free_space_gb >= 100;

            let (valid, message) = if !writable {
                (false, format!("No write permission on {}", existing_parent))
            } else if !space_sufficient {
                (
                    false,
                    format!("{} GB free (100 GB required) at {}", free_space_gb, existing_parent),
                )
            } else {
                (true, format!("{} GB free at {}", free_space_gb, existing_parent))
            };

            PathValidation {
                valid,
                writable,
                free_space_gb,
                space_sufficient,
                message,
            }
        }).await
        .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn scan_runners(base_path: String) -> Result<ScanRunnersResult, String> {
    tokio::task
        ::spawn_blocking(move || {
            let expanded = expand_tilde(&base_path);
            let runners_dir = Path::new(&expanded).join("runners");
            let runners_dir_str = runners_dir.to_string_lossy().into_owned();

            let mut runners = Vec::new();

            if runners_dir.is_dir() {
                if let Ok(entries) = fs::read_dir(&runners_dir) {
                    for entry in entries.flatten() {
                        let entry_path = entry.path();
                        if !entry_path.is_dir() {
                            continue;
                        }

                        let wine_exe = entry_path.join("bin").join("wine");
                        if wine_exe.exists() {
                            let name = entry_path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned();
                            let bin_path = entry_path.join("bin").to_string_lossy().into_owned();
                            let wine_executable = wine_exe.to_string_lossy().into_owned();

                            runners.push(DetectedRunner {
                                name,
                                bin_path,
                                wine_executable,
                            });
                        }
                    }
                }
            }

            runners.sort_by(|a, b| a.name.cmp(&b.name));

            ScanRunnersResult {
                runners,
                runners_dir: runners_dir_str,
            }
        }).await
        .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn save_config(config: AppConfig) -> Result<(), String> {
    tokio::task
        ::spawn_blocking(move || {
            let path = config_file_path().ok_or("Could not determine config directory")?;
            let config_path = Path::new(&path);

            if let Some(parent) = config_path.parent() {
                fs
                    ::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create config directory: {}", e))?;
            }

            // Merge with existing config to preserve fields the frontend doesn't manage
            let defaults = AppConfig::default();
            let merged = if
                let Some(existing) = config_path
                    .exists()
                    .then(|| fs::read_to_string(config_path).ok())
                    .flatten()
                    .and_then(|c| serde_json::from_str::<AppConfig>(&c).ok())
            {
                // Preserve runner_sources: use existing if available, otherwise use defaults
                let runner_sources = if
                    existing.runner_sources.is_empty() &&
                    config.runner_sources.is_empty()
                {
                    defaults.runner_sources.clone()
                } else if config.runner_sources.is_empty() {
                    existing.runner_sources
                } else {
                    config.runner_sources
                };

                AppConfig {
                    github_token: match &config.github_token {
                        Some(t) if t.is_empty() => None, // explicit clear
                        Some(_) => config.github_token, // new value
                        None => existing.github_token, // preserve existing
                    },
                    runner_sources,
                    install_mode: if config.install_mode.is_empty() {
                        existing.install_mode
                    } else {
                        config.install_mode
                    },
                    ..config
                }
            } else {
                // No existing config - use provided config but fill defaults for missing fields
                AppConfig {
                    runner_sources: if config.runner_sources.is_empty() {
                        defaults.runner_sources
                    } else {
                        config.runner_sources
                    },
                    github_token: config.github_token,
                    install_path: config.install_path,
                    selected_runner: config.selected_runner,
                    performance: config.performance,
                    log_level: if config.log_level.is_empty() {
                        defaults.log_level
                    } else {
                        config.log_level
                    },
                    auto_backup_on_launch: config.auto_backup_on_launch,
                    install_mode: if config.install_mode.is_empty() {
                        defaults.install_mode
                    } else {
                        config.install_mode
                    },
                }
            };

            let json = serde_json
                ::to_string_pretty(&merged)
                .map_err(|e| format!("Failed to serialize config: {}", e))?;

            fs::write(config_path, json).map_err(|e| format!("Failed to write config: {}", e))?;

            Ok(())
        }).await
        .map_err(|e| format!("Task failed: {}", e))?
}

#[tauri::command]
pub async fn load_config() -> Result<Option<AppConfig>, String> {
    tokio::task
        ::spawn_blocking(move || {
            let path = match config_file_path() {
                Some(p) => p,
                None => {
                    return None;
                }
            };
            let contents = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[config] Failed to read config file {}: {}", path, e);
                    return None;
                }
            };
            let config: AppConfig = match serde_json::from_str(&contents) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[config] Failed to parse config JSON: {}", e);
                    return None;
                }
            };

            // Fill in defaults for runner_sources if empty and save to config
            let defaults = AppConfig::default();
            if config.runner_sources.is_empty() {
                // Write defaults to config file immediately
                let new_config = AppConfig {
                    runner_sources: defaults.runner_sources.clone(),
                    ..config
                };
                if let Ok(json) = serde_json::to_string_pretty(&new_config) {
                    let _ = fs::write(&path, json);
                }
                return Some(new_config);
            }

            Some(config)
        }).await
        .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn reset_app() -> Result<(), String> {
    let config_path = config_file_path().ok_or("Could not determine config directory")?;
    let cache_path = cache_file_path();

    let existing: Option<AppConfig> = fs
        ::read_to_string(&config_path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok());

    let install_path = existing
        .as_ref()
        .map(|c| expand_tilde(&c.install_path))
        .unwrap_or_default();
    let github_token = existing.as_ref().and_then(|c| c.github_token.clone());

    // Delete installation directory
    if !install_path.is_empty() {
        let p = Path::new(&install_path);
        if p.exists() {
            fs::remove_dir_all(p).map_err(|e| format!("Failed to delete installation: {}", e))?;
        }
    }

    // Remove config file
    let _ = fs::remove_file(&config_path);

    // Remove cache file
    if let Some(cp) = cache_path {
        let _ = fs::remove_file(&cp);
    }

    // Preserve github_token in a minimal config
    if let Some(token) = github_token {
        let minimal = AppConfig {
            github_token: Some(token),
            ..AppConfig::default()
        };
        if let Ok(json) = serde_json::to_string_pretty(&minimal) {
            if let Some(parent) = Path::new(&config_path).parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&config_path, json);
        }
    }

    Ok(())
}

// --- Cache Commands ---

#[tauri::command]
pub async fn load_runner_cache() -> Result<RunnerCache, String> {
    tokio::task
        ::spawn_blocking(move || {
            let path = match cache_file_path() {
                Some(p) => p,
                None => {
                    return RunnerCache::default();
                }
            };
            let contents = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => {
                    return RunnerCache::default();
                }
            };
            let cache: AppCache = match serde_json::from_str(&contents) {
                Ok(c) => c,
                Err(_) => {
                    return RunnerCache::default();
                }
            };
            cache.runners
        }).await
        .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn save_runner_cache(runners: Vec<CachedRunner>) -> Result<(), String> {
    let runners_cache = RunnerCache {
        runners,
        cached_at: std::time::SystemTime
            ::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };

    let result = tokio::task::spawn_blocking(move || {
        let path = match cache_file_path() {
            Some(p) => p,
            None => {
                return Err("Could not determine cache directory".to_string());
            }
        };
        let cache_path = Path::new(&path);

        if let Some(parent) = cache_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return Err(format!("Failed to create cache directory: {}", e));
            }
        }

        // Load existing cache
        let mut cache = if cache_path.exists() {
            fs::read_to_string(&path)
                .ok()
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or_default()
        } else {
            AppCache::default()
        };

        cache.runners = runners_cache;

        let json = match serde_json::to_string_pretty(&cache) {
            Ok(j) => j,
            Err(e) => {
                return Err(format!("Failed to serialize cache: {}", e));
            }
        };

        if let Err(e) = fs::write(cache_path, json) {
            return Err(format!("Failed to write cache: {}", e));
        }

        Ok(())
    }).await;

    match result {
        Ok(r) => r,
        Err(e) => Err(format!("Task failed: {}", e)),
    }
}

#[tauri::command]
pub async fn load_dxvk_cache() -> Result<DxvkCache, String> {
    tokio::task
        ::spawn_blocking(move || {
            let path = match cache_file_path() {
                Some(p) => p,
                None => {
                    return DxvkCache::default();
                }
            };
            let contents = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => {
                    return DxvkCache::default();
                }
            };
            let cache: AppCache = match serde_json::from_str(&contents) {
                Ok(c) => c,
                Err(_) => {
                    return DxvkCache::default();
                }
            };
            cache.dxvk
        }).await
        .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn save_dxvk_cache(releases: Vec<CachedDxvkRelease>) -> Result<(), String> {
    let dxvk_cache = DxvkCache {
        releases,
        cached_at: std::time::SystemTime
            ::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };

    let result = tokio::task::spawn_blocking(move || {
        let path = match cache_file_path() {
            Some(p) => p,
            None => {
                return Err("Could not determine cache directory".to_string());
            }
        };
        let cache_path = Path::new(&path);

        if let Some(parent) = cache_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return Err(format!("Failed to create cache directory: {}", e));
            }
        }

        // Load existing cache
        let mut cache = if cache_path.exists() {
            fs::read_to_string(&path)
                .ok()
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or_default()
        } else {
            AppCache::default()
        };

        cache.dxvk = dxvk_cache;

        let json = match serde_json::to_string_pretty(&cache) {
            Ok(j) => j,
            Err(e) => {
                return Err(format!("Failed to serialize cache: {}", e));
            }
        };

        if let Err(e) = fs::write(cache_path, json) {
            return Err(format!("Failed to write cache: {}", e));
        }

        Ok(())
    }).await;

    match result {
        Ok(r) => r,
        Err(e) => Err(format!("Task failed: {}", e)),
    }
}

// --- Runner Sources Management ---

/// Result of adding a new runner source.
#[derive(Serialize, Deserialize)]
pub struct AddRunnerSourceResult {
    pub success: bool,
    pub message: String,
    pub added_sources: Vec<String>,
}

#[tauri::command]
pub async fn add_runner_source_from_github(
    name: String,
    api_url: String
) -> Result<AddRunnerSourceResult, String> {
    let name = name.trim().to_string();
    let api_url = api_url.trim().to_string();

    if name.is_empty() {
        return Err("Name cannot be empty".into());
    }
    if api_url.is_empty() {
        return Err("API URL cannot be empty".into());
    }

    // Validate URL format
    if !api_url.contains("api.github.com/repos") {
        return Err(
            "URL must be a GitHub API URL (e.g., https://api.github.com/repos/owner/repo/releases)".into()
        );
    }

    let result = tokio::task
        ::spawn_blocking(move || {
            let config_path = match config_file_path() {
                Some(p) => Path::new(&p).to_path_buf(),
                None => {
                    return Err("Could not determine config directory".to_string());
                }
            };

            // Load existing config
            let mut config = if config_path.exists() {
                let contents = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
                serde_json::from_str::<AppConfig>(&contents).map_err(|e| e.to_string())?
            } else {
                AppConfig::default()
            };

            // Check if source already exists
            if config.runner_sources.iter().any(|s| s.name == name) {
                return Ok(AddRunnerSourceResult {
                    success: false,
                    message: format!("Source '{}' already exists", name),
                    added_sources: vec![],
                });
            }

            // Determine filter based on name
            let filter = if name.to_lowercase().contains("kron4ek") {
                Some("kron4ek".into())
            } else {
                Some("all".into())
            };

            // Add new source
            config.runner_sources.push(RunnerSourceConfig {
                name: name.clone(),
                api_url: api_url.clone(),
                filter,
                enabled: true,
            });

            // Save config
            let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
            fs::write(&config_path, json).map_err(|e| e.to_string())?;

            Ok(AddRunnerSourceResult {
                success: true,
                message: format!("Added source '{}'", name),
                added_sources: vec![name],
            })
        }).await
        .map_err(|e| format!("Task failed: {}", e))?;

    result
}

#[tauri::command]
pub async fn import_lug_helper_sources() -> Result<AddRunnerSourceResult, String> {
    // LUG-Helper Wine runner sources (from https://github.com/starcitizen-lug/lug-helper)
    let lug_sources = vec![
        ("LUG", "https://api.github.com/repos/starcitizen-lug/lug-wine/releases"),
        (
            "LUG Experimental",
            "https://api.github.com/repos/starcitizen-lug/lug-wine-experimental/releases",
        ),
        ("RawFox", "https://api.github.com/repos/starcitizen-lug/raw-wine/releases"),
        ("Kron4ek", "https://api.github.com/repos/Kron4ek/Wine-Builds/releases")
    ];

    let result = tokio::task
        ::spawn_blocking(move || {
            let config_path = match config_file_path() {
                Some(p) => Path::new(&p).to_path_buf(),
                None => {
                    return Err("Could not determine config directory".to_string());
                }
            };

            // Load existing config
            let mut config = if config_path.exists() {
                let contents = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
                serde_json::from_str::<AppConfig>(&contents).map_err(|e| e.to_string())?
            } else {
                AppConfig::default()
            };

            let mut added_sources = Vec::new();

            for (name, api_url) in lug_sources {
                // Check if source already exists
                if !config.runner_sources.iter().any(|s| s.name == name) {
                    // Determine filter based on name
                    let filter = if name.to_lowercase().contains("kron4ek") {
                        Some("kron4ek".into())
                    } else {
                        Some("all".into())
                    };

                    config.runner_sources.push(RunnerSourceConfig {
                        name: name.to_string(),
                        api_url: api_url.to_string(),
                        filter,
                        enabled: true,
                    });
                    added_sources.push(name.to_string());
                }
            }

            // Save config
            let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
            fs::write(&config_path, json).map_err(|e| e.to_string())?;

            let message = if added_sources.is_empty() {
                "All LUG sources already configured".to_string()
            } else {
                format!("Added {} new source(s)", added_sources.len())
            };

            Ok(AddRunnerSourceResult {
                success: true,
                message,
                added_sources,
            })
        }).await
        .map_err(|e| format!("Task failed: {}", e))?;

    result
}
