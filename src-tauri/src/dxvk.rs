//! DXVK installation and management module.
//!
//! This module handles:
//! - Fetching available DXVK releases from GitHub
//! - Detecting installed DXVK versions in Wine prefixes
//! - Installing DXVK (downloading, extracting, copying DLLs)
//!
//! DXVK is a Vulkan-based implementation of Direct3D 9, 10, and 11
//! for Wine-based runners, providing better performance.

use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Emitter};

use crate::config::AppConfig;

/// Loads the GitHub token from the configuration file for API rate limit increase.
fn load_github_token() -> Option<String> {
    let config_path = dirs::config_dir()?.join("star-control").join("config.json");
    let contents = std::fs::read_to_string(config_path).ok()?;
    let config: AppConfig = serde_json::from_str(&contents).ok()?;
    config.github_token
}

// --- Structs ---

/// Information about an available DXVK release from GitHub.
#[derive(Serialize, Deserialize, Clone)]
pub struct DxvkRelease {
    pub version: String,
    pub download_url: String,
    pub file_name: String,
    pub size_bytes: u64,
}

/// Status of DXVK in a Wine prefix.
#[derive(Serialize, Deserialize, Clone)]
pub struct DxvkStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub dlls_found: Vec<String>,
}

/// Progress update for DXVK installation.
#[derive(Serialize, Deserialize, Clone)]
pub struct DxvkProgress {
    pub phase: String,
    pub percent: f64,
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
pub async fn fetch_dxvk_releases() -> Result<Vec<DxvkRelease>, String> {
    let token = load_github_token();

    let client = reqwest::Client::builder()
        .user_agent("star-control/0.1.4")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let url = "https://api.github.com/repos/doitsujin/dxvk/releases?per_page=10";
    let mut request = client.get(url);
    if let Some(ref t) = token {
        request = request.header("Authorization", format!("Bearer {}", t));
    }
    let resp = request
        .send()
        .await
        .map_err(|e| format!("Failed to fetch DXVK releases: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned {}", resp.status()));
    }

    let releases: Vec<GhRelease> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse DXVK releases: {}", e))?;

    let mut result = Vec::new();
    for release in &releases {
        for asset in &release.assets {
            if asset.name.ends_with(".tar.gz") {
                result.push(DxvkRelease {
                    version: release.tag_name.clone(),
                    download_url: asset.browser_download_url.clone(),
                    file_name: asset.name.clone(),
                    size_bytes: asset.size,
                });
                break; // one asset per release
            }
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn detect_dxvk_version(base_path: String) -> Result<DxvkStatus, String> {
    tokio::task::spawn_blocking(move || {
        let expanded = expand_tilde(&base_path);
        let prefix = Path::new(&expanded);
        let sys32 = prefix
            .join("drive_c")
            .join("windows")
            .join("system32");

        let check_dlls = ["d3d9.dll", "d3d10core.dll", "d3d11.dll", "dxgi.dll"];
        let mut dlls_found = Vec::new();

        for dll in &check_dlls {
            if sys32.join(dll).exists() {
                dlls_found.push(dll.to_string());
            }
        }

        // Read marker file for version
        let marker = prefix.join(".dxvk_version");
        let version = std::fs::read_to_string(&marker).ok().map(|s| s.trim().to_string());

        // If DLLs exist but no marker file, check if this is a winetricks installation
        // and create the marker file
        let installed = if !dlls_found.is_empty() && version.is_none() {
            // Try to detect winetricks DXVK - it typically installs to system32
            if !dlls_found.is_empty() {
                // Create marker file for winetricks-installed DXVK
                let _ = std::fs::write(&marker, "dxvk (winetricks)");
                Some("dxvk (winetricks)".to_string())
            } else {
                None
            }
        } else {
            version
        };

        let is_installed = !dlls_found.is_empty() && installed.is_some();

        DxvkStatus {
            installed: is_installed,
            version: installed,
            dlls_found,
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))
}

#[tauri::command]
pub async fn install_dxvk(
    app: AppHandle,
    download_url: String,
    version: String,
    base_path: String,
) -> Result<(), String> {
    let expanded = expand_tilde(&base_path);
    let prefix = Path::new(&expanded);
    let tmp_dir = prefix.join(".tmp");

    let emit = |phase: &str, percent: f64, msg: &str| {
        let _ = app.emit(
            "dxvk-progress",
            DxvkProgress {
                phase: phase.to_string(),
                percent,
                message: msg.to_string(),
            },
        );
    };

    // Ensure tmp dir
    tokio::fs::create_dir_all(&tmp_dir)
        .await
        .map_err(|e| format!("Failed to create tmp dir: {}", e))?;

    emit("downloading", 0.0, "Downloading DXVK...");

    // Download
    let client = reqwest::Client::builder()
        .user_agent("star-control/0.1.4")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let response = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    let total_bytes = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let archive_path = tmp_dir.join("dxvk.tar.gz");

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let mut file = tokio::fs::File::create(&archive_path)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    let mut stream = response.bytes_stream();
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                file.write_all(&chunk)
                    .await
                    .map_err(|e| format!("Write error: {}", e))?;
                downloaded += chunk.len() as u64;
                let percent = if total_bytes > 0 {
                    (downloaded as f64 / total_bytes as f64) * 50.0
                } else {
                    25.0
                };
                emit("downloading", percent, "Downloading DXVK...");
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&archive_path).await;
                return Err(format!("Stream error: {}", e));
            }
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;
    drop(file);

    // Extract
    emit("extracting", 50.0, "Extracting DXVK...");

    let extract_dir = tmp_dir.join("dxvk-extract");
    let extract_dir_clone = extract_dir.clone();
    let archive_path_clone = archive_path.clone();

    let extract_result = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&extract_dir_clone)?;
        let file = std::fs::File::open(&archive_path_clone)?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(&extract_dir_clone)?;
        Ok::<(), std::io::Error>(())
    })
    .await;

    match extract_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            return Err(format!("Extraction failed: {}", e));
        }
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&tmp_dir).await;
            return Err(format!("Task error: {}", e));
        }
    }

    emit("installing", 70.0, "Installing DXVK DLLs...");

    // Find the extracted directory (usually dxvk-X.Y.Z/)
    let entries: Vec<_> = std::fs::read_dir(&extract_dir)
        .map_err(|e| format!("Read error: {}", e))?
        .filter_map(|e| e.ok())
        .collect();

    let dxvk_dir = if entries.len() == 1 && entries[0].path().is_dir() {
        entries[0].path()
    } else {
        extract_dir.clone()
    };

    // Copy x64 DLLs to system32
    let sys32 = prefix.join("drive_c").join("windows").join("system32");
    let x64_dir = dxvk_dir.join("x64");

    tokio::fs::create_dir_all(&sys32)
        .await
        .map_err(|e| format!("Failed to create system32: {}", e))?;

    if x64_dir.is_dir() {
        copy_dlls(&x64_dir, &sys32)?;
    }

    // Copy x32 DLLs to syswow64
    let syswow64 = prefix.join("drive_c").join("windows").join("syswow64");
    let x32_dir = dxvk_dir.join("x32");

    tokio::fs::create_dir_all(&syswow64)
        .await
        .map_err(|e| format!("Failed to create syswow64: {}", e))?;

    if x32_dir.is_dir() {
        copy_dlls(&x32_dir, &syswow64)?;
    }

    emit("installing", 90.0, "Writing version marker...");

    // Write version marker
    let marker_path = prefix.join(".dxvk_version");
    std::fs::write(&marker_path, &version)
        .map_err(|e| format!("Failed to write marker: {}", e))?;

    // Cleanup
    let _ = tokio::fs::remove_dir_all(&tmp_dir).await;

    emit("complete", 100.0, "DXVK installed successfully!");

    Ok(())
}

fn copy_dlls(src_dir: &Path, dest_dir: &Path) -> Result<(), String> {
    let dll_names = ["d3d9.dll", "d3d10core.dll", "d3d11.dll", "dxgi.dll"];

    for name in &dll_names {
        let src = src_dir.join(name);
        if src.exists() {
            let dest = dest_dir.join(name);
            std::fs::copy(&src, &dest)
                .map_err(|e| format!("Failed to copy {}: {}", name, e))?;
        }
    }

    Ok(())
}
