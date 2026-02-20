use crate::sc_config::{expand_tilde, sc_base_dir};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

// ============================================================
// Data Structures
// ============================================================

#[derive(Serialize, Deserialize, Clone)]
pub struct LanguageSource {
    pub language_code: String,
    pub language_name: String,
    pub flag: String,
    pub source_repo: String,
    pub source_label: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LocalizationStatus {
    pub installed: bool,
    pub language_code: Option<String>,
    pub language_name: Option<String>,
    pub source_label: Option<String>,
    pub installed_at: Option<String>,
    pub file_size: Option<u64>,
    pub cfg_language: Option<String>,
    pub cfg_language_audio: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LocalizationInstallResult {
    pub success: bool,
    pub message: String,
    pub bytes: u64,
}

#[derive(Serialize, Deserialize, Clone)]
struct LocalizationMeta {
    language_code: String,
    language_name: String,
    source_label: String,
    installed_at: String,
    file_size: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LocalizationProgress {
    pub phase: String,
    pub percent: f64,
    pub message: String,
}

// ============================================================
// Path Helpers
// ============================================================

fn sc_localization_dir(game_path: &str, version: &str, language_code: &str) -> PathBuf {
    sc_base_dir(game_path, version)
        .join("data")
        .join("Localization")
        .join(language_code)
}

fn meta_dir() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|p| p.join("star-control").join("localization"))
        .ok_or_else(|| "Could not determine config directory".to_string())
}

fn meta_path(version: &str) -> Result<PathBuf, String> {
    Ok(meta_dir()?.join(format!("{}.json", version)))
}

// ============================================================
// Download URL Builder
// ============================================================

fn build_download_url(source_repo: &str, language_code: &str, version: &str) -> String {
    let branch = match version {
        "PTU" | "EPTU" | "HOTFIX" | "TECH-PREVIEW" => "ptu",
        _ => "main",
    };

    if source_repo == "rjcncpt/StarCitizen-Deutsch-INI" {
        let folder = if branch == "ptu" { "ptu" } else { "live" };
        format!(
            "https://raw.githubusercontent.com/{}/{}/{}/global.ini",
            source_repo, "main", folder
        )
    } else {
        // Dymerz/StarCitizen-Localization
        format!(
            "https://raw.githubusercontent.com/{}/{}/data/Localization/{}/global.ini",
            source_repo, branch, language_code
        )
    }
}

// ============================================================
// USER.cfg Language Management
// ============================================================

fn update_user_cfg_language(
    game_path: &str,
    version: &str,
    language_code: &str,
) -> Result<(), String> {
    let cfg_path = sc_base_dir(game_path, version).join("USER.cfg");

    let content = if cfg_path.exists() {
        fs::read_to_string(&cfg_path)
            .map_err(|e| format!("Failed to read USER.cfg: {}", e))?
    } else {
        String::new()
    };

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

    let mut found_lang = false;
    let mut found_audio = false;

    for line in lines.iter_mut() {
        let trimmed = line.trim();
        if trimmed.starts_with("g_language") && trimmed.contains('=') {
            *line = format!("g_language = {}", language_code);
            found_lang = true;
        } else if trimmed.starts_with("g_languageAudio") && trimmed.contains('=') {
            *line = "g_languageAudio = english".to_string();
            found_audio = true;
        }
    }

    if !found_lang {
        lines.push(format!("g_language = {}", language_code));
    }
    if !found_audio {
        lines.push("g_languageAudio = english".to_string());
    }

    let result = lines.join("\n");
    // Ensure trailing newline
    let result = if result.ends_with('\n') {
        result
    } else {
        format!("{}\n", result)
    };

    if let Some(parent) = cfg_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    fs::write(&cfg_path, result).map_err(|e| format!("Failed to write USER.cfg: {}", e))
}

fn remove_user_cfg_language(game_path: &str, version: &str) -> Result<(), String> {
    let cfg_path = sc_base_dir(game_path, version).join("USER.cfg");

    if !cfg_path.exists() {
        return Ok(());
    }

    let content =
        fs::read_to_string(&cfg_path).map_err(|e| format!("Failed to read USER.cfg: {}", e))?;

    let lines: Vec<&str> = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with("g_language") && trimmed.contains('='))
                && !(trimmed.starts_with("g_languageAudio") && trimmed.contains('='))
        })
        .collect();

    let result = lines.join("\n");
    let result = if result.ends_with('\n') {
        result
    } else {
        format!("{}\n", result)
    };

    fs::write(&cfg_path, result).map_err(|e| format!("Failed to write USER.cfg: {}", e))
}

fn parse_cfg_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(key) && trimmed.contains('=') {
            let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
            if parts.len() == 2 && parts[0].trim() == key {
                let mut val = parts[1].trim().to_string();
                // Strip inline comments
                if let Some(idx) = val.find(';') {
                    val = val[..idx].trim().to_string();
                }
                return Some(val);
            }
        }
    }
    None
}

// ============================================================
// Metadata Management
// ============================================================

fn save_meta(version: &str, meta: &LocalizationMeta) -> Result<(), String> {
    let path = meta_path(version)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create meta directory: {}", e))?;
    }
    let json = serde_json::to_string_pretty(meta)
        .map_err(|e| format!("Failed to serialize meta: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write meta: {}", e))
}

fn load_meta(version: &str) -> Option<LocalizationMeta> {
    let path = meta_path(version).ok()?;
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn delete_meta(version: &str) -> Result<(), String> {
    let path = meta_path(version)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Failed to delete meta: {}", e))?;
    }
    Ok(())
}

// ============================================================
// Tauri Commands
// ============================================================

#[derive(Serialize)]
pub struct LocalizationUpdateCheck {
    pub update_available: bool,
    pub local_size: u64,
    pub remote_size: u64,
}

#[tauri::command]
pub async fn check_localization_update(
    game_path: String,
    version: String,
) -> Result<LocalizationUpdateCheck, String> {
    let meta = load_meta(&version)
        .ok_or_else(|| "No localization installed".to_string())?;

    let languages = get_available_languages().await?;
    let source = languages
        .iter()
        .find(|l| l.language_code == meta.language_code)
        .ok_or_else(|| "Language source not found".to_string())?;

    let url = build_download_url(&source.source_repo, &meta.language_code, &version);

    let client = reqwest::Client::new();
    let resp = client
        .head(&url)
        .send()
        .await
        .map_err(|e| format!("HEAD request failed: {}", e))?;

    let remote_size = resp
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    // Also check local file size on disk (not just meta) for accuracy
    let expanded = expand_tilde(&game_path);
    let ini_path = sc_localization_dir(&expanded, &version, &meta.language_code)
        .join("global.ini");
    let local_size = std::fs::metadata(&ini_path)
        .map(|m| m.len())
        .unwrap_or(meta.file_size);

    Ok(LocalizationUpdateCheck {
        update_available: remote_size > 0 && remote_size != local_size,
        local_size,
        remote_size,
    })
}

#[tauri::command]
pub async fn get_available_languages() -> Result<Vec<LanguageSource>, String> {
    Ok(vec![
        LanguageSource {
            language_code: "german_(germany)".to_string(),
            language_name: "Deutsch".to_string(),
            flag: "DE".to_string(),
            source_repo: "rjcncpt/StarCitizen-Deutsch-INI".to_string(),
            source_label: "rjcncpt German".to_string(),
        },
        LanguageSource {
            language_code: "german_(germany)".to_string(),
            language_name: "Deutsch".to_string(),
            flag: "DE".to_string(),
            source_repo: "Dymerz/StarCitizen-Localization".to_string(),
            source_label: "Community Localization".to_string(),
        },
        LanguageSource {
            language_code: "french_(france)".to_string(),
            language_name: "Fran\u{00e7}ais".to_string(),
            flag: "FR".to_string(),
            source_repo: "Dymerz/StarCitizen-Localization".to_string(),
            source_label: "Community Localization".to_string(),
        },
        LanguageSource {
            language_code: "spanish_(spain)".to_string(),
            language_name: "Espa\u{00f1}ol".to_string(),
            flag: "ES".to_string(),
            source_repo: "Dymerz/StarCitizen-Localization".to_string(),
            source_label: "Community Localization".to_string(),
        },
        LanguageSource {
            language_code: "italian_(italy)".to_string(),
            language_name: "Italiano".to_string(),
            flag: "IT".to_string(),
            source_repo: "Dymerz/StarCitizen-Localization".to_string(),
            source_label: "Community Localization".to_string(),
        },
        LanguageSource {
            language_code: "portuguese_(brazil)".to_string(),
            language_name: "Portugu\u{00ea}s".to_string(),
            flag: "BR".to_string(),
            source_repo: "Dymerz/StarCitizen-Localization".to_string(),
            source_label: "Community Localization".to_string(),
        },
    ])
}

#[tauri::command]
pub async fn get_localization_status(
    game_path: String,
    version: String,
) -> Result<LocalizationStatus, String> {
    let expanded = expand_tilde(&game_path);

    // Read USER.cfg for language settings
    let cfg_path = sc_base_dir(&expanded, &version).join("USER.cfg");
    let (cfg_language, cfg_language_audio) = if cfg_path.exists() {
        let content = fs::read_to_string(&cfg_path).unwrap_or_default();
        (
            parse_cfg_value(&content, "g_language"),
            parse_cfg_value(&content, "g_languageAudio"),
        )
    } else {
        (None, None)
    };

    // Check metadata
    if let Some(meta) = load_meta(&version) {
        let ini_path = sc_localization_dir(&expanded, &version, &meta.language_code)
            .join("global.ini");

        if ini_path.exists() {
            let file_size = fs::metadata(&ini_path).map(|m| m.len()).ok();

            return Ok(LocalizationStatus {
                installed: true,
                language_code: Some(meta.language_code),
                language_name: Some(meta.language_name),
                source_label: Some(meta.source_label),
                installed_at: Some(meta.installed_at),
                file_size,
                cfg_language,
                cfg_language_audio,
            });
        }
    }

    // Fallback: check if g_language is set but no metadata exists
    if let Some(ref lang) = cfg_language {
        let ini_path = sc_localization_dir(&expanded, &version, lang).join("global.ini");
        if ini_path.exists() {
            let file_size = fs::metadata(&ini_path).map(|m| m.len()).ok();
            return Ok(LocalizationStatus {
                installed: true,
                language_code: Some(lang.clone()),
                language_name: None,
                source_label: None,
                installed_at: None,
                file_size,
                cfg_language,
                cfg_language_audio,
            });
        }
    }

    Ok(LocalizationStatus {
        installed: false,
        language_code: None,
        language_name: None,
        source_label: None,
        installed_at: None,
        file_size: None,
        cfg_language,
        cfg_language_audio,
    })
}

#[tauri::command]
pub async fn install_localization(
    app: AppHandle,
    game_path: String,
    version: String,
    language_code: String,
    source_repo: String,
    language_name: String,
    source_label: String,
) -> Result<LocalizationInstallResult, String> {
    let expanded = expand_tilde(&game_path);

    // Emit progress: starting
    let _ = app.emit(
        "localization-progress",
        LocalizationProgress {
            phase: "download".to_string(),
            percent: 0.0,
            message: format!("Downloading {} translation...", language_name),
        },
    );

    // Build URL
    let url = build_download_url(&source_repo, &language_code, &version);

    // Download
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with status: {}",
            response.status()
        ));
    }

    let _ = app.emit(
        "localization-progress",
        LocalizationProgress {
            phase: "download".to_string(),
            percent: 50.0,
            message: "Downloading...".to_string(),
        },
    );

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let file_size = bytes.len() as u64;

    let _ = app.emit(
        "localization-progress",
        LocalizationProgress {
            phase: "install".to_string(),
            percent: 75.0,
            message: "Installing translation file...".to_string(),
        },
    );

    // Create directory and write file
    let loc_dir = sc_localization_dir(&expanded, &version, &language_code);
    fs::create_dir_all(&loc_dir)
        .map_err(|e| format!("Failed to create localization directory: {}", e))?;

    let ini_path = loc_dir.join("global.ini");
    fs::write(&ini_path, &bytes)
        .map_err(|e| format!("Failed to write global.ini: {}", e))?;

    // Update USER.cfg
    update_user_cfg_language(&expanded, &version, &language_code)?;

    // Save metadata
    let now = Local::now();
    let meta = LocalizationMeta {
        language_code: language_code.clone(),
        language_name: language_name.clone(),
        source_label: source_label.clone(),
        installed_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
        file_size,
    };
    save_meta(&version, &meta)?;

    let _ = app.emit(
        "localization-progress",
        LocalizationProgress {
            phase: "done".to_string(),
            percent: 100.0,
            message: "Installation complete!".to_string(),
        },
    );

    Ok(LocalizationInstallResult {
        success: true,
        message: format!("{} translation installed successfully", language_name),
        bytes: file_size,
    })
}

#[tauri::command]
pub async fn remove_localization(
    game_path: String,
    version: String,
) -> Result<(), String> {
    let expanded = expand_tilde(&game_path);

    // Load meta to find the language code
    let language_code = if let Some(meta) = load_meta(&version) {
        meta.language_code
    } else {
        // Fallback: read from USER.cfg
        let cfg_path = sc_base_dir(&expanded, &version).join("USER.cfg");
        if cfg_path.exists() {
            let content = fs::read_to_string(&cfg_path).unwrap_or_default();
            parse_cfg_value(&content, "g_language")
                .ok_or_else(|| "No localization found to remove".to_string())?
        } else {
            return Err("No localization found to remove".to_string());
        }
    };

    // Delete global.ini
    let ini_path = sc_localization_dir(&expanded, &version, &language_code).join("global.ini");
    if ini_path.exists() {
        fs::remove_file(&ini_path)
            .map_err(|e| format!("Failed to delete global.ini: {}", e))?;
    }

    // Try to remove the language directory if empty
    let lang_dir = sc_localization_dir(&expanded, &version, &language_code);
    if lang_dir.exists() {
        let _ = fs::remove_dir(&lang_dir); // Ignore error if not empty
    }

    // Clean up USER.cfg
    remove_user_cfg_language(&expanded, &version)?;

    // Delete metadata
    delete_meta(&version)?;

    Ok(())
}
