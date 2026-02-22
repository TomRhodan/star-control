//! Wine/Proton runner management module.
//!
//! This module handles:
//! - Fetching available Wine/Proton runners from configured GitHub sources
//! - Installing runners (downloading, extracting archives)
//! - Deleting installed runners
//! - Canceling ongoing downloads
//!
//! Supported archive formats: .tar.gz, .tar.xz, .tar.zst, .tar.zstd

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

use crate::config::{AppConfig, RunnerSourceConfig};

/// Loads the GitHub token from the configuration file.
fn load_github_token() -> Option<String> {
    let config_path = dirs::config_dir()?.join("star-control").join("config.json");
    let contents = std::fs::read_to_string(config_path).ok()?;
    let config: AppConfig = serde_json::from_str(&contents).ok()?;
    config.github_token
}

/// Flag to signal cancellation of runner download.
static CANCEL_FLAG: AtomicBool = AtomicBool::new(false);

// --- Runner Sources ---

/// Loads runner sources from the configuration file.
fn load_runner_sources() -> Vec<RunnerSourceConfig> {
    let config_path = match dirs::config_dir() {
        Some(p) => p.join("star-control").join("config.json"),
        None => return AppConfig::default().runner_sources,
    };

    let contents = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return AppConfig::default().runner_sources,
    };

    let config: AppConfig = match serde_json::from_str(&contents) {
        Ok(c) => c,
        Err(_) => return AppConfig::default().runner_sources,
    };

    // If runner_sources is empty in config, return defaults
    if config.runner_sources.is_empty() {
        AppConfig::default().runner_sources
    } else {
        config.runner_sources
    }
}

/// Returns the appropriate filter function based on the runner source filter setting.
fn get_filter_fn(filter: &Option<String>) -> fn(&str) -> bool {
    match filter.as_deref() {
        Some("kron4ek") => filter_kron4ek,
        _ => accept_all,
    }
}

/// Accepts all runner names.
fn accept_all(_name: &str) -> bool {
    true
}

/// Filters out 32-bit (x86, wow64) runners for Kron4ek builds.
fn filter_kron4ek(name: &str) -> bool {
    let lower = name.to_lowercase();
    !lower.contains("x86") && !lower.contains("wow64")
}

// --- Structs ---

/// Information about an available runner from a GitHub source.
#[derive(Serialize, Deserialize, Clone)]
pub struct AvailableRunner {
    pub name: String,
    pub source: String,
    pub version: String,
    pub download_url: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub installed: bool,
}

/// Result of fetching available runners from all sources.
#[derive(Serialize, Deserialize)]
pub struct FetchRunnersResult {
    pub runners: Vec<AvailableRunner>,
    pub errors: Vec<String>,
}

/// Progress update during runner download.
#[derive(Serialize, Deserialize, Clone)]
pub struct DownloadProgress {
    pub phase: String,
    pub runner_name: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percent: f64,
    pub message: String,
}

/// Result of a runner installation operation.
#[derive(Serialize, Deserialize)]
pub struct InstallRunnerResult {
    pub success: bool,
    pub runner_name: String,
    pub install_path: String,
    pub message: String,
}

// --- GitHub API types ---

/// GitHub release response structure.
#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

/// GitHub release asset (download file).
#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

// --- Helper: strip known archive extensions ---

/// Removes common archive extensions from a filename.
fn strip_archive_ext(name: &str) -> String {
    let mut s = name.to_string();
    for ext in &[".tar.gz", ".tar.xz", ".tar.zst", ".tar.zstd"] {
        if s.ends_with(ext) {
            s = s[..s.len() - ext.len()].to_string();
            return s;
        }
    }
    s
}

/// Checks if a filename is a supported archive format.
fn is_archive(name: &str) -> bool {
    name.ends_with(".tar.gz")
        || name.ends_with(".tar.xz")
        || name.ends_with(".tar.zst")
        || name.ends_with(".tar.zstd")
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

// --- Tauri Commands ---

#[tauri::command]
pub async fn fetch_available_runners(base_path: String) -> FetchRunnersResult {
    let expanded = expand_tilde(&base_path);
    let runners_dir = Path::new(&expanded).join("runners");

    let token = load_github_token();
    let sources = load_runner_sources();

    let client = reqwest::Client::builder()
        .user_agent("star-control/0.1.3")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let mut all_runners = Vec::new();
    let mut errors = Vec::new();

    for source in sources {
        if !source.enabled {
            continue;
        }

        let filter_fn = get_filter_fn(&source.filter);
        let url = format!("{}?per_page=25", source.api_url);
        let mut request = client.get(&url);
        if let Some(ref t) = token {
            request = request.header("Authorization", format!("Bearer {}", t));
        }
        match request.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    errors.push(format!(
                        "{}: GitHub API returned {}",
                        source.name,
                        resp.status()
                    ));
                    continue;
                }
                match resp.json::<Vec<GhRelease>>().await {
                    Ok(releases) => {
                        for release in &releases {
                            for asset in &release.assets {
                                if !is_archive(&asset.name) {
                                    continue;
                                }
                                if !filter_fn(&asset.name) {
                                    continue;
                                }

                                let display_name = strip_archive_ext(&asset.name);
                                let installed = runners_dir.join(&display_name).is_dir();

                                all_runners.push(AvailableRunner {
                                    name: display_name,
                                    source: source.name.clone(),
                                    version: release.tag_name.clone(),
                                    download_url: asset.browser_download_url.clone(),
                                    file_name: asset.name.clone(),
                                    size_bytes: asset.size,
                                    installed,
                                });
                            }
                        }
                    }
                    Err(e) => {
                        errors.push(format!("{}: Failed to parse response: {}", source.name, e));
                    }
                }
            }
            Err(e) => {
                errors.push(format!("{}: {}", source.name, e));
            }
        }
    }

    FetchRunnersResult {
        runners: all_runners,
        errors,
    }
}

#[tauri::command]
pub async fn install_runner(
    app: AppHandle,
    download_url: String,
    file_name: String,
    base_path: String,
) -> InstallRunnerResult {
    let expanded = expand_tilde(&base_path);
    let runner_name = strip_archive_ext(&file_name);
    let runners_dir = Path::new(&expanded).join("runners");
    let final_path = runners_dir.join(&runner_name);

    // Already installed?
    if final_path.is_dir() {
        return InstallRunnerResult {
            success: true,
            runner_name: runner_name.clone(),
            install_path: final_path.to_string_lossy().into_owned(),
            message: "Runner already installed".into(),
        };
    }

    // Reset cancel flag
    CANCEL_FLAG.store(false, Ordering::SeqCst);

    // Ensure runners dir exists
    if let Err(e) = tokio::fs::create_dir_all(&runners_dir).await {
        return InstallRunnerResult {
            success: false,
            runner_name: runner_name.clone(),
            install_path: String::new(),
            message: format!("Failed to create runners directory: {}", e),
        };
    }

    let tmp_dir = runners_dir.join(format!(".tmp-{}", runner_name));
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
    if let Err(e) = tokio::fs::create_dir_all(&tmp_dir).await {
        return InstallRunnerResult {
            success: false,
            runner_name: runner_name.clone(),
            install_path: String::new(),
            message: format!("Failed to create temp directory: {}", e),
        };
    }

    let archive_path = tmp_dir.join(&file_name);

    // --- Download phase ---
    let emit_progress = |phase: &str, downloaded: u64, total: u64, msg: &str| {
        let percent = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let _ = app.emit(
            "runner-download-progress",
            DownloadProgress {
                phase: phase.to_string(),
                runner_name: runner_name.clone(),
                bytes_downloaded: downloaded,
                total_bytes: total,
                percent,
                message: msg.to_string(),
            },
        );
    };

    let client = reqwest::Client::builder()
        .user_agent("star-control/0.1.3")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let response = match client.get(&download_url).send().await {
        Ok(r) => r,
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            emit_progress("error", 0, 0, &format!("Download failed: {}", e));
            return InstallRunnerResult {
                success: false,
                runner_name: runner_name.clone(),
                install_path: String::new(),
                message: format!("Download failed: {}", e),
            };
        }
    };

    let total_bytes = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    emit_progress("downloading", 0, total_bytes, "Starting download...");

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let mut file = match tokio::fs::File::create(&archive_path).await {
        Ok(f) => f,
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            emit_progress("error", 0, 0, &format!("Failed to create file: {}", e));
            return InstallRunnerResult {
                success: false,
                runner_name: runner_name.clone(),
                install_path: String::new(),
                message: format!("Failed to create file: {}", e),
            };
        }
    };

    let mut stream = response.bytes_stream();
    while let Some(chunk_result) = stream.next().await {
        if CANCEL_FLAG.load(Ordering::SeqCst) {
            let _ = file.flush().await;
            drop(file);
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            emit_progress("error", downloaded, total_bytes, "Download cancelled");
            return InstallRunnerResult {
                success: false,
                runner_name: runner_name.clone(),
                install_path: String::new(),
                message: "Download cancelled by user".into(),
            };
        }

        match chunk_result {
            Ok(chunk) => {
                if let Err(e) = file.write_all(&chunk).await {
                    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
                    emit_progress("error", downloaded, total_bytes, &format!("Write error: {}", e));
                    return InstallRunnerResult {
                        success: false,
                        runner_name: runner_name.clone(),
                        install_path: String::new(),
                        message: format!("Write error: {}", e),
                    };
                }
                downloaded += chunk.len() as u64;
                emit_progress("downloading", downloaded, total_bytes, "Downloading...");
            }
            Err(e) => {
                let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
                emit_progress("error", downloaded, total_bytes, &format!("Stream error: {}", e));
                return InstallRunnerResult {
                    success: false,
                    runner_name: runner_name.clone(),
                    install_path: String::new(),
                    message: format!("Stream error: {}", e),
                };
            }
        }
    }

    if let Err(e) = file.flush().await {
        let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
        emit_progress("error", downloaded, total_bytes, &format!("Flush error: {}", e));
        return InstallRunnerResult {
            success: false,
            runner_name: runner_name.clone(),
            install_path: String::new(),
            message: format!("Flush error: {}", e),
        };
    }
    drop(file);

    // --- Extraction phase ---
    emit_progress("extracting", downloaded, total_bytes, "Extracting archive...");

    let extract_dir = tmp_dir.join("extract");
    let archive_path_clone = archive_path.clone();
    let extract_dir_clone = extract_dir.clone();
    let file_name_clone = file_name.clone();

    let extract_result = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&extract_dir_clone)?;

        let file = std::fs::File::open(&archive_path_clone)?;

        if file_name_clone.ends_with(".tar.gz") {
            let decoder = flate2::read::GzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(&extract_dir_clone)?;
        } else if file_name_clone.ends_with(".tar.xz") {
            let decoder = xz2::read::XzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(&extract_dir_clone)?;
        } else if file_name_clone.ends_with(".tar.zst") || file_name_clone.ends_with(".tar.zstd") {
            let decoder = zstd::stream::read::Decoder::new(file)?;
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(&extract_dir_clone)?;
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Unknown archive format",
            ));
        }

        Ok::<(), std::io::Error>(())
    })
    .await;

    match extract_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            emit_progress("error", downloaded, total_bytes, &format!("Extraction failed: {}", e));
            return InstallRunnerResult {
                success: false,
                runner_name: runner_name.clone(),
                install_path: String::new(),
                message: format!("Extraction failed: {}", e),
            };
        }
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            emit_progress("error", downloaded, total_bytes, &format!("Task error: {}", e));
            return InstallRunnerResult {
                success: false,
                runner_name: runner_name.clone(),
                install_path: String::new(),
                message: format!("Task error: {}", e),
            };
        }
    }

    // --- Normalize directory structure (LUG-Helper convention) ---
    // If extract contains a single directory, use that; otherwise wrap everything
    let entries: Vec<_> = match std::fs::read_dir(&extract_dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            emit_progress("error", downloaded, total_bytes, &format!("Read error: {}", e));
            return InstallRunnerResult {
                success: false,
                runner_name: runner_name.clone(),
                install_path: String::new(),
                message: format!("Read extracted dir error: {}", e),
            };
        }
    };

    if entries.len() == 1 && entries[0].path().is_dir() {
        // Single directory — rename it to the final name
        if let Err(e) = std::fs::rename(entries[0].path(), &final_path) {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            emit_progress("error", downloaded, total_bytes, &format!("Move error: {}", e));
            return InstallRunnerResult {
                success: false,
                runner_name: runner_name.clone(),
                install_path: String::new(),
                message: format!("Failed to move runner: {}", e),
            };
        }
    } else {
        // Multiple items — rename extract dir to final name
        if let Err(e) = std::fs::rename(&extract_dir, &final_path) {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            emit_progress("error", downloaded, total_bytes, &format!("Move error: {}", e));
            return InstallRunnerResult {
                success: false,
                runner_name: runner_name.clone(),
                install_path: String::new(),
                message: format!("Failed to move runner: {}", e),
            };
        }
    }

    // Cleanup tmp dir
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    emit_progress("complete", downloaded, total_bytes, "Installation complete!");

    InstallRunnerResult {
        success: true,
        runner_name: runner_name.clone(),
        install_path: final_path.to_string_lossy().into_owned(),
        message: "Runner installed successfully".into(),
    }
}

#[tauri::command]
pub fn cancel_runner_install() -> bool {
    CANCEL_FLAG.store(true, Ordering::SeqCst);
    true
}

#[tauri::command]
pub async fn delete_runner(runner_name: String, base_path: String) -> Result<(), String> {
    let expanded = expand_tilde(&base_path);
    let runner_path = Path::new(&expanded).join("runners").join(&runner_name);

    if !runner_path.is_dir() {
        return Err(format!("Runner directory not found: {}", runner_path.display()));
    }

    // Safety check: must contain bin/wine
    let wine_bin = runner_path.join("bin").join("wine");
    if !wine_bin.exists() {
        return Err(format!(
            "Safety check failed: {} does not contain bin/wine",
            runner_path.display()
        ));
    }

    tokio::fs::remove_dir_all(&runner_path)
        .await
        .map_err(|e| format!("Failed to delete runner: {}", e))?;

    Ok(())
}
