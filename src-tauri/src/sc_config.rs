use std::collections::HashMap;
use std::time::UNIX_EPOCH;
use std::fs::{self, File};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use chrono::Local;
use serde::{Deserialize, Serialize};
use quick_xml::Reader;
use quick_xml::events::Event;
use quick_xml::events::BytesStart;
use tokio::sync::Mutex;
use once_cell::sync::Lazy;
use quick_xml::writer::Writer;
use encoding_rs::UTF_16LE;

use crate::action_definitions::{ActionDefinitions, CompleteBinding, BindingStats, BindingListResponse};

// ============================================================
// Types & Statics
// ============================================================

#[derive(Serialize, Deserialize, Clone)]
pub struct ScVersionInfo {
    pub version: String,
    pub path: String,
    pub has_usercfg: bool,
    pub has_attributes: bool,
    pub has_actionmaps: bool,
    pub has_exported_layouts: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ScAttribute { pub name: String, pub value: String }

#[derive(Serialize, Deserialize, Default)]
pub struct ScAttributes { pub version: String, pub attrs: Vec<ScAttribute> }

#[derive(Serialize, Deserialize)]
pub struct BackupInfo {
    pub id: String, pub created_at: String, pub timestamp: u64,
    pub version: String, pub backup_type: String, pub files: Vec<String>, pub label: String,
}

#[derive(Serialize, Deserialize)]
pub struct ExportedLayout { pub filename: String, pub label: String, pub modified: u64 }

#[derive(Serialize, Deserialize)]
pub struct DeviceReorderEntry {
    #[serde(rename = "deviceType")] pub device_type: String,
    #[serde(rename = "oldInstance")] pub old_instance: u32,
    #[serde(rename = "newInstance")] pub new_instance: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScBinding { pub action_name: String, pub input: String }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScActionMap { pub name: String, pub bindings: Vec<ScBinding>, pub actions: Vec<String> }

#[derive(Serialize, Deserialize, Clone)]
pub struct ScDevice { pub device_type: String, pub instance: u32, pub product: String, pub guid: String }

#[derive(Serialize, Deserialize, Clone)]
pub struct ScDeviceOption { pub input: String, pub deadzone: Option<f64>, pub saturation: Option<f64> }

#[derive(Serialize, Deserialize, Clone)]
pub struct ScDeviceOptions { pub name: String, pub options: Vec<ScDeviceOption> }

#[derive(Serialize, Deserialize, Default)]
pub struct ParsedActionMaps {
    pub version: String,
    #[serde(rename = "profileName")] pub profile_name: String,
    pub devices: Vec<ScDevice>,
    #[serde(rename = "deviceOptions")] pub device_options: Vec<ScDeviceOptions>,
    #[serde(rename = "action_maps")] pub action_maps: Vec<ScActionMap>,
}

#[derive(Serialize, Deserialize)]
struct CachedLocalization { p4k_size: u64, p4k_modified: u64, labels: HashMap<String, String> }

static LOCALIZATION_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// ============================================================
// Helper Functions
// ============================================================

fn log_debug(msg: &str) {
    let now = Local::now();
    println!("[DEBUG {}] {}", now.format("%H:%M:%S%.3f"), msg);
}

pub(crate) fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            let home = home.to_string_lossy();
            return path.replacen('~', &home, 1);
        }
    }
    path.to_string()
}

pub(crate) fn sc_base_dir(game_path: &str, version: &str) -> PathBuf {
    Path::new(game_path).join("drive_c").join("Program Files").join("Roberts Space Industries").join("StarCitizen").join(version)
}

fn sc_user_dir(game_path: &str, version: &str) -> PathBuf {
    sc_base_dir(game_path, version).join("user").join("client").join("0")
}

fn sc_profile_dir(game_path: &str, version: &str) -> PathBuf {
    sc_user_dir(game_path, version).join("Profiles").join("default")
}

fn sc_mappings_dir(game_path: &str, version: &str) -> PathBuf {
    sc_user_dir(game_path, version).join("controls").join("mappings")
}

fn sc_p4k_path(game_path: &str, version: &str) -> Result<PathBuf, String> {
    let base_dir = sc_base_dir(game_path, version);
    let possible_paths = vec![base_dir.join("Data.p4k"), base_dir.join("Data").join("Data.p4k"), base_dir.join("Data").join("Public").join("Data.p4k")];
    for path in &possible_paths { if path.exists() { return Ok(path.clone()); } }
    Err(format!("Data.p4k not found in {:?}", possible_paths))
}

fn localization_cache_path(version: &str) -> Result<PathBuf, String> {
    dirs::cache_dir().map(|p| p.join("star-control").join("localization").join(version).join("labels.json")).ok_or_else(|| "Could not determine cache directory".to_string())
}

fn backup_base_dir() -> Result<PathBuf, String> {
    dirs::config_dir().map(|p| p.join("star-control").join("backups")).ok_or_else(|| "Could not determine config directory".to_string())
}

fn backup_version_dir(version: &str) -> Result<PathBuf, String> { Ok(backup_base_dir()?.join(version)) }

// ============================================================
// CryXmlB Parser (High Robustness Version)
// ============================================================

fn read_cry_string(table: &[u8], offset: usize) -> String {
    if offset >= table.len() { return String::new(); }
    let end = table[offset..].iter().position(|&b| b == 0).unwrap_or(table.len() - offset);
    String::from_utf8_lossy(&table[offset..offset + end]).into_owned()
}

fn parse_cryxmlb_full(buffer: &[u8]) -> Result<ParsedActionMaps, String> {
    if !buffer.starts_with(b"CryXmlB") { return Err("Not a CryXmlB file".to_string()); }
    
    // Header reading with bounds checks
    let read_u32 = |off: usize| -> u32 {
        if off + 4 > buffer.len() { 0 } else { u32::from_le_bytes(buffer[off..off+4].try_into().unwrap()) }
    };

    let nt_off = read_u32(12) as usize;
    let nt_cnt = read_u32(16) as usize;
    let at_off = read_u32(20) as usize;
    let at_cnt = read_u32(24) as usize;
    let st_off = read_u32(36) as usize;
    let st_sz = read_u32(40) as usize;
    
    if st_off + st_sz > buffer.len() { return Err("Invalid String Table Offset".to_string()); }
    let st = &buffer[st_off..st_off + st_sz];

    log_debug(&format!("CryXmlB: Nodes={}, Attrs={}, Strings={} bytes", nt_cnt, at_cnt, st_sz));

    let mut res = ParsedActionMaps::default();
    let mut current_am: Option<ScActionMap> = None;
    let mut action_count = 0;

    for i in 0..nt_cnt {
        let off = nt_off + i * 28; 
        if off + 28 > buffer.len() { break; }
        
        let node_bytes = &buffer[off..off+28];
        let n_idx = u32::from_le_bytes(node_bytes[0..4].try_into().unwrap()) as usize;
        
        // In SC versions, Attribute Index is often at offset 12 or 16, not 4.
        // Let's try to detect it by checking the attribute count first.
        // Usually: [NameIdx:4][ChildIdx:4][NextIdx:4][AttrIdx:4][AttrCnt:2][...]
        let a_idx = u32::from_le_bytes(node_bytes[12..16].try_into().unwrap()) as usize;
        let a_cnt = u16::from_le_bytes(node_bytes[16..18].try_into().unwrap()) as usize;
        
        let tag = read_cry_string(st, n_idx).to_lowercase();

        if tag == "actionmap" || tag == "actiongroup" || tag == "action_group" {
            if let Some(am) = current_am.take() { res.action_maps.push(am); }
            let mut name = String::new();
            for j in 0..a_cnt {
                let ao = at_off + (a_idx + j) * 8;
                if ao + 8 <= buffer.len() {
                    let k_idx = u32::from_le_bytes(buffer[ao..ao+4].try_into().unwrap()) as usize;
                    let v_idx = u32::from_le_bytes(buffer[ao+4..ao+8].try_into().unwrap()) as usize;
                    if read_cry_string(st, k_idx).to_lowercase() == "name" { name = read_cry_string(st, v_idx); }
                }
            }
            current_am = Some(ScActionMap { name, bindings: vec![], actions: vec![] });
        } else if tag == "action" {
            let mut name = String::new();
            for j in 0..a_cnt {
                let ao = at_off + (a_idx + j) * 8;
                if ao + 8 <= buffer.len() {
                    let k_idx = u32::from_le_bytes(buffer[ao..ao+4].try_into().unwrap()) as usize;
                    let v_idx = u32::from_le_bytes(buffer[ao+4..ao+8].try_into().unwrap()) as usize;
                    let key = read_cry_string(st, k_idx).to_lowercase();
                    if key == "name" || key == "id" { name = read_cry_string(st, v_idx); }
                }
            }
            if let Some(ref mut am) = current_am {
                if !name.is_empty() { am.actions.push(name); action_count += 1; }
            }
        } else if tag == "rebind" {
            if let Some(ref mut am) = current_am {
                if let Some(an) = am.actions.last().cloned() {
                    for j in 0..a_cnt {
                        let ao = at_off + (a_idx + j) * 8;
                        if ao + 8 <= buffer.len() {
                            let k_idx = u32::from_le_bytes(buffer[ao..ao+4].try_into().unwrap()) as usize;
                            let v_idx = u32::from_le_bytes(buffer[ao+4..ao+8].try_into().unwrap()) as usize;
                            if read_cry_string(st, k_idx).to_lowercase() == "input" { 
                                am.bindings.push(ScBinding { action_name: an.clone(), input: read_cry_string(st, v_idx) }); 
                            }
                        }
                    }
                }
            }
        }
    }
    
    if let Some(am) = current_am { res.action_maps.push(am); }
    log_debug(&format!("Safe Parser Stats: {} categories, {} actions total", res.action_maps.len(), action_count));
    Ok(res)
}

// ============================================================
// P4K Reader logic
// ============================================================

pub fn read_p4k_file(game_path: &str, version: &str, file_path: &str) -> Result<Vec<u8>, String> {
    let p4k_path = sc_p4k_path(game_path, version)?;
    let mut file = File::open(&p4k_path).map_err(|e| format!("Failed to open P4K: {}", e))?;
    let len = file.metadata().unwrap().len();
    let (cd_offset, cd_size) = find_central_directory(&mut file, len)?;
    file.seek(SeekFrom::Start(cd_offset)).unwrap();
    let mut cd_buffer = vec![0u8; cd_size as usize]; file.read_exact(&mut cd_buffer).unwrap();
    let search_path = file_path.replace('/', "\\");
    let mut current_pos = 0;
    while current_pos + 46 <= cd_buffer.len() {
        if &cd_buffer[current_pos..current_pos+4] != b"PK\x01\x02" { current_pos += 1; continue; }
        let header = &cd_buffer[current_pos..current_pos+46];
        let n_len = u16::from_le_bytes([header[28], header[29]]) as usize;
        let e_len = u16::from_le_bytes([header[30], header[31]]) as usize;
        let c_len = u16::from_le_bytes([header[32], header[33]]) as usize;
        let name = String::from_utf8_lossy(&cd_buffer[current_pos+46..current_pos+46+n_len]);
        if name.eq_ignore_ascii_case(&search_path) {
            let mut comp_size = u32::from_le_bytes([header[20], header[21], header[22], header[23]]) as u64;
            let mut offset = u32::from_le_bytes([header[42], header[43], header[44], header[45]]) as u64;
            if e_len >= 28 {
                let ex = current_pos + 46 + n_len;
                if u16::from_le_bytes([cd_buffer[ex], cd_buffer[ex+1]]) == 0x0001 {
                    comp_size = u64::from_le_bytes(cd_buffer[ex+12..ex+20].try_into().unwrap());
                    offset = u64::from_le_bytes(cd_buffer[ex+20..ex+28].try_into().unwrap());
                }
            }
            let method = u16::from_le_bytes([header[10], header[11]]);
            file.seek(SeekFrom::Start(offset)).unwrap();
            let mut lh = [0u8; 30]; file.read_exact(&mut lh).unwrap();
            let data_off = offset + 30 + u16::from_le_bytes([lh[26], lh[27]]) as u64 + u16::from_le_bytes([lh[28], lh[29]]) as u64;
            file.seek(SeekFrom::Start(data_off)).unwrap();
            let mut data = vec![0u8; comp_size as usize]; file.read_exact(&mut data).unwrap();
            if method == 100 || method == 93 {
                let mut dec = zstd::Decoder::new(Cursor::new(data)).unwrap();
                let mut out = Vec::new(); dec.read_to_end(&mut out).unwrap();
                return Ok(out);
            }
            return Ok(data);
        }
        current_pos += 46 + n_len + e_len + c_len;
    }
    Err(format!("File '{}' not found in Data.p4k", file_path))
}

fn find_central_directory(file: &mut File, len: u64) -> Result<(u64, u64), String> {
    let scan = 65536.min(len); file.seek(SeekFrom::End(-(scan as i64))).unwrap();
    let mut buf = vec![0u8; scan as usize]; file.read_exact(&mut buf).unwrap();
    for i in (0..scan as usize - 4).rev() {
        if &buf[i..i+4] == b"PK\x06\x07" {
            file.seek(SeekFrom::Start(len - scan + i as u64 + 8)).unwrap();
            let mut off = [0u8; 8]; file.read_exact(&mut off).unwrap();
            let eocd_off = u64::from_le_bytes(off);
            file.seek(SeekFrom::Start(eocd_off + 40)).unwrap();
            let mut sz = [0u8; 8]; file.read_exact(&mut sz).unwrap();
            let mut cd_off = [0u8; 8]; file.read_exact(&mut cd_off).unwrap();
            return Ok((u64::from_le_bytes(cd_off), u64::from_le_bytes(sz)));
        }
    }
    Err("No ZIP64".to_string())
}

// ============================================================
// Public API
// ============================================================

#[tauri::command]
pub async fn get_localization_labels(game_path: String, version: String, language: Option<String>) -> Result<HashMap<String, String>, String> {
    let p4k = sc_p4k_path(&expand_tilde(&game_path), &version)?;
    let cache = localization_cache_path(&version)?;
    let meta = fs::metadata(&p4k).unwrap();
    let (sz, modif) = (meta.len(), meta.modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs());
    if cache.exists() {
        if let Ok(c) = fs::read_to_string(&cache) {
            if let Ok(cached) = serde_json::from_str::<CachedLocalization>(&c) {
                if cached.p4k_size == sz && cached.p4k_modified == modif { return Ok(cached.labels); }
            }
        }
    }
    let _g = LOCALIZATION_LOCK.lock().await;
    let lang = language.unwrap_or_else(|| "english".to_string());
    let result: Result<HashMap<String, String>, String> = tokio::task::spawn_blocking(move || {
        let buf = read_p4k_file(&game_path, &version, &format!("Data/Localization/{}/global.ini", lang))
            .or_else(|_| read_p4k_file(&game_path, &version, &format!("Localization/{}/global.ini", lang)))?;
        let content = if buf.starts_with(&[0xFF, 0xFE]) { UTF_16LE.decode(&buf[2..]).0.into_owned() } else { String::from_utf8_lossy(&buf).into_owned() };
        let labels = parse_global_ini(&content);
        let cached = CachedLocalization { p4k_size: sz, p4k_modified: modif, labels: labels.clone() };
        fs::create_dir_all(cache.parent().unwrap()).ok();
        fs::write(cache, serde_json::to_string(&cached).unwrap()).ok();
        Ok(labels)
    }).await.unwrap();
    result
}

#[tauri::command]
pub async fn get_complete_binding_list(game_path: String, version: String) -> Result<BindingListResponse, String> {
    log_debug(&format!("--- START get_complete_binding_list for {} ---", version));
    let labels = get_localization_labels(game_path.clone(), version.clone(), None).await.unwrap_or_default();
    
    // Find master profile path by scanning
    let files = list_p4k(game_path.clone(), version.clone(), Some("defaultProfile.xml".to_string())).await.unwrap_or_default();
    let master_path = files.iter().find(|f| f.ends_with("defaultProfile.xml")).cloned();
    
    if let Some(ref path) = master_path {
        log_debug(&format!("Found master profile at: {}", path));
    } else {
        return Err("Could not find defaultProfile.xml in game files.".to_string());
    }

    let master_raw = read_p4k_file(&game_path, &version, &master_path.unwrap())?;
    let master_parsed = parse_cryxmlb_full(&master_raw)?;

    let user_p = sc_profile_dir(&expand_tilde(&game_path), &version).join("actionmaps.xml");
    let user_parsed = if user_p.exists() { parse_actionmaps_xml(&fs::read_to_string(user_p).unwrap_or_default()).ok() } else { None };

    let mut merged: HashMap<String, HashMap<String, (Vec<String>, bool)>> = HashMap::new();
    for am in master_parsed.action_maps {
        let action_map = merged.entry(am.name).or_insert_with(HashMap::new);
        for action_name in am.actions { action_map.entry(action_name).or_insert_with(|| (Vec::new(), false)); }
        for b in am.bindings { let entry = action_map.entry(b.action_name).or_insert_with(|| (Vec::new(), false)); if !entry.0.contains(&b.input) { entry.0.push(b.input); } }
    }
    if let Some(up) = user_parsed {
        for am in up.action_maps {
            let action_map = merged.entry(am.name).or_insert_with(HashMap::new);
            for b in am.bindings { 
                let entry = action_map.entry(b.action_name).or_insert_with(|| (Vec::new(), false));
                if !entry.0.contains(&b.input) { entry.0.push(b.input); }
                entry.1 = true;
            }
        }
    }

    let mut results = Vec::new();
    let mut stats = BindingStats { total: 0, custom: 0 };
    for (cat_name, actions) in merged {
        let cat_label = labels.get(&format!("ui_Control{}", cat_name)).or(labels.get(&cat_name)).cloned().unwrap_or_else(|| cat_name.replace('_', " "));
        for (action_name, (inputs, is_custom)) in actions {
            stats.total += 1; if is_custom { stats.custom += 1; }
            let display_name = labels.get(&format!("ui_Control{}", action_name))
                .or(labels.get(&action_name))
                .or(labels.get(&format!("ui_Control_{}", action_name)))
                .or(labels.get(&format!("ui_v_{}", action_name.strip_prefix("v_").unwrap_or(&action_name))))
                .cloned().unwrap_or_else(|| action_name.replace('_', " "));
            if inputs.is_empty() {
                results.push(CompleteBinding { category: cat_label.clone(), action_name: action_name.clone(), display_name: display_name.clone(), current_input: "".to_string(), device_type: "none".to_string(), description: None, is_custom });
            } else {
                for input in inputs { results.push(CompleteBinding { category: cat_label.clone(), action_name: action_name.clone(), display_name: display_name.clone(), current_input: input, device_type: "none".to_string(), description: None, is_custom }); }
            }
        }
    }
    results.sort_by(|a, b| a.category.cmp(&b.category));
    log_debug(&format!("--- END get_complete_binding_list: {} entries generated ---", results.len()));
    Ok(BindingListResponse { bindings: results, stats })
}

fn parse_global_ini(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with(';') { continue; }
        if let Some(pos) = t.find('=') {
            let mut k = t[..pos].trim().to_string();
            let v = t[pos + 1..].trim().to_string();
            if k.starts_with('@') { k = k[1..].to_string(); }
            map.insert(k, v);
        }
    }
    map
}

#[tauri::command] pub async fn read_p4k(game_path: String, version: String, file_path: String) -> Result<String, String> { 
    Ok(String::from_utf8_lossy(&read_p4k_file(&game_path, &version, &file_path)?).into_owned())
}

#[tauri::command]
pub async fn list_p4k(game_path: String, version: String, pattern: Option<String>) -> Result<Vec<String>, String> {
    let p4k_path = sc_p4k_path(&game_path, &version)?;
    let mut file = File::open(&p4k_path).map_err(|e| e.to_string())?;
    let len = file.metadata().unwrap().len();
    let (cd_offset, cd_size) = find_central_directory(&mut file, len)?;
    file.seek(SeekFrom::Start(cd_offset)).unwrap();
    let mut cd_buffer = vec![0u8; cd_size as usize]; file.read_exact(&mut cd_buffer).unwrap();
    let mut files = Vec::new();
    let pat = pattern.unwrap_or_default().to_lowercase();
    let mut current_pos = 0;
    while current_pos + 46 <= cd_buffer.len() {
        if &cd_buffer[current_pos..current_pos+4] != b"PK\x01\x02" { current_pos += 1; continue; }
        let header = &cd_buffer[current_pos..current_pos+46];
        let n_len = u16::from_le_bytes([header[28], header[29]]) as usize;
        let e_len = u16::from_le_bytes([header[30], header[31]]) as usize;
        let c_len = u16::from_le_bytes([header[32], header[33]]) as usize;
        let name = String::from_utf8_lossy(&cd_buffer[current_pos+46..current_pos+46+n_len]);
        if name.to_lowercase().contains(&pat) { files.push(name.to_string()); }
        current_pos += 46 + n_len + e_len + c_len;
    }
    Ok(files)
}

#[tauri::command] pub async fn get_localization_ini(game_path: String, version: String, language: Option<String>) -> Result<String, String> {
    let buf = read_p4k_file(&game_path, &version, &format!("Data/Localization/{}/global.ini", language.unwrap_or_else(|| "english".to_string())))?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

#[tauri::command] pub async fn list_localization_languages(game_path: String, version: String) -> Result<Vec<String>, String> {
    let files = list_p4k(game_path, version, Some("Localization/".to_string())).await?;
    let mut languages = std::collections::HashSet::new();
    for file in files {
        if let Some(rest) = file.strip_prefix("Data\\Localization\\").or(file.strip_prefix("Localization\\")) {
            if let Some(end) = rest.find('\\') { let l = rest[..end].to_string(); if !l.is_empty() { languages.insert(l); } }
        }
    }
    let mut res: Vec<String> = languages.into_iter().collect(); res.sort(); Ok(res)
}

#[tauri::command] pub async fn detect_sc_versions(game_path: String) -> Result<Vec<ScVersionInfo>, String> {
    let exp = expand_tilde(&game_path);
    let base = Path::new(&exp).join("drive_c").join("Program Files").join("Roberts Space Industries").join("StarCitizen");
    let mut versions = Vec::new();
    if let Ok(entries) = fs::read_dir(&base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                let user_cfg = path.join("USER.cfg");
                let profile = sc_profile_dir(&exp, &name);
                versions.push(ScVersionInfo { version: name.clone(), path: path.to_string_lossy().into_owned(), has_usercfg: user_cfg.exists(), has_attributes: profile.join("attributes.xml").exists(), has_actionmaps: profile.join("actionmaps.xml").exists(), has_exported_layouts: sc_mappings_dir(&exp, &name).is_dir() });
            }
        }
    }
    versions.sort_by_key(|v| match v.version.as_str() { "LIVE" => 0, "PTU" => 1, "HOTFIX" => 2, _ => 3 });
    Ok(versions)
}

#[tauri::command] pub async fn read_user_cfg(game_path: String, version: String) -> Result<String, String> {
    let p = sc_base_dir(&expand_tilde(&game_path), &version).join("USER.cfg");
    if !p.exists() { return Ok(String::new()); }
    fs::read_to_string(p).map_err(|e| e.to_string())
}

#[tauri::command] pub async fn write_user_cfg(game_path: String, version: String, content: String) -> Result<(), String> {
    let p = sc_base_dir(&expand_tilde(&game_path), &version).join("USER.cfg");
    if let Some(parent) = p.parent() { fs::create_dir_all(parent).ok(); }
    fs::write(p, content).map_err(|e| e.to_string())
}

#[tauri::command] pub async fn list_profiles(game_path: String, version: String) -> Result<Vec<ScProfile>, String> {
    let p = sc_user_dir(&expand_tilde(&game_path), &version).join("Profiles");
    if !p.is_dir() { return Ok(Vec::new()); }
    let mut res = Vec::new();
    if let Ok(entries) = fs::read_dir(p) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                if name == "frontend" { continue; }
                let mut last = 0;
                if let Ok(c) = fs::read_to_string(path.join("attributes.xml")) {
                    if let Some(s) = c.find("lastPlayed=\"") { if let Some(e) = c[s+12..].find('"') { last = c[s+12..s+12+e].parse().unwrap_or(0); } }
                }
                res.push(ScProfile { name, last_played: last });
            }
        }
    }
    res.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(res)
}

#[derive(Serialize, Deserialize, Clone)] pub struct ScProfile { pub name: String, pub last_played: u64 }

#[tauri::command] pub async fn read_attributes(game_path: String, version: String) -> Result<ScAttributes, String> {
    let p = sc_profile_dir(&expand_tilde(&game_path), &version).join("attributes.xml");
    if !p.exists() { return Ok(ScAttributes::default()); }
    let content = fs::read_to_string(p).map_err(|e| e.to_string())?;
    Ok(parse_attributes_xml(&content))
}

fn parse_attributes_xml(content: &str) -> ScAttributes {
    let mut attrs = ScAttributes::default();
    if let Some(s) = content.find("Version=\"").or(content.find("version=\"")) {
        let start = s + 9; if let Some(e) = content[start..].find('"') { attrs.version = content[start..start+e].to_string(); }
    }
    let mut pos = 0;
    while let Some(s) = content[pos..].find("<Attr ") {
        let s = pos + s;
        if let Some(ns) = content[s..].find("name=\"") {
            let ns = s + ns + 6;
            if let Some(ne) = content[ns..].find('"') {
                let name = content[ns..ns+ne].to_string();
                if let Some(vs) = content[ns+ne..].find("value=\"") {
                    let vs = ns + ne + vs + 7;
                    if let Some(ve) = content[vs..].find('"') { attrs.attrs.push(ScAttribute { name, value: content[vs..vs+ve].to_string() }); }
                }
            }
        }
        pos = s + 1;
    }
    attrs
}

#[tauri::command] pub async fn write_attributes(game_path: String, version: String, attrs: ScAttributes) -> Result<(), String> {
    let p = sc_profile_dir(&expand_tilde(&game_path), &version).join("attributes.xml");
    if let Some(parent) = p.parent() { fs::create_dir_all(parent).ok(); }
    let mut xml = format!("<Attributes Version=\"{}\">\n", attrs.version);
    for a in attrs.attrs { xml.push_str(&format!(" <Attr name=\"{}\" value=\"{}\"/>\n", a.name, a.value)); }
    xml.push_str("</Attributes>\n");
    fs::write(p, xml).map_err(|e| e.to_string())
}

#[tauri::command] pub async fn export_profile(game_path: String, version: String, dest_path: String) -> Result<(), String> {
    let exp = expand_tilde(&game_path); let src = sc_profile_dir(&exp, &version); let dest = Path::new(&dest_path);
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] { if src.join(f).exists() { fs::copy(src.join(f), dest.join(f)).map_err(|e| e.to_string())?; } }
    Ok(())
}

#[tauri::command] pub async fn import_profile(game_path: String, version: String, source_path: String) -> Result<(), String> {
    let exp = expand_tilde(&game_path); let dest = sc_profile_dir(&exp, &version); let src = Path::new(&source_path);
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] { if src.join(f).exists() { fs::copy(src.join(f), dest.join(f)).map_err(|e| e.to_string())?; } }
    Ok(())
}

fn parse_actionmaps_xml(content: &str) -> Result<ParsedActionMaps, String> {
    let trimmed = content.trim().trim_matches('\0'); if trimmed.is_empty() { return Err("Empty XML".to_string()); }
    let mut reader = Reader::from_str(trimmed); reader.config_mut().trim_text(true);
    let mut result = ParsedActionMaps::default();
    let (mut current_am, mut current_action, mut current_opts) = (None, None, None);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let tag_owned = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let tag = tag_owned.as_str();
                match tag {
                    "ActionMaps" | "ActionProfiles" => {
                        if let Some(v) = get_attr(e, b"version") { result.version = v; }
                        if let Some(v) = get_attr(e, b"profileName").or(get_attr(e, b"profile_name")) { result.profile_name = v; }
                    }
                    "options" => {
                        let t = get_attr(e, b"type").unwrap_or_default();
                        let i = get_attr(e, b"instance").and_then(|v| v.parse().ok()).unwrap_or(0);
                        let p_raw = get_attr(e, b"Product").unwrap_or_default();
                        if !p_raw.is_empty() {
                            let (prod, guid) = if let Some(gs) = p_raw.rfind('{') { (p_raw[..gs].trim().to_string(), p_raw[gs..].trim().to_string()) } else { (p_raw.trim().to_string(), String::new()) };
                            result.devices.push(ScDevice { device_type: t, instance: i, product: prod, guid });
                        }
                    }
                    "deviceoptions" => { current_opts = Some(ScDeviceOptions { name: get_attr(e, b"name").unwrap_or_default(), options: Vec::new() }); }
                    "option" => { if let Some(ref mut o) = current_opts { o.options.push(ScDeviceOption { input: get_attr(e, b"input").unwrap_or_default(), deadzone: get_attr(e, b"deadzone").and_then(|v| v.parse().ok()), saturation: get_attr(e, b"saturation").and_then(|v| v.parse().ok()) }); } }
                    "actionmap" => { current_am = Some(ScActionMap { name: get_attr(e, b"name").unwrap_or_default(), bindings: Vec::new(), actions: Vec::new() }); }
                    "action" => { let name = get_attr(e, b"name").unwrap_or_default(); if let Some(ref mut am) = current_am { if !name.is_empty() { am.actions.push(name.clone()); } } current_action = Some(name); }
                    "rebind" => { if let (Some(ref a), Some(ref mut am)) = (current_action.as_ref(), current_am.as_mut()) { let i = get_attr(e, b"input").unwrap_or_default(); if !i.is_empty() { am.bindings.push(ScBinding { action_name: a.to_string(), input: i }); } } }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let tag_owned = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let tag = tag_owned.as_str();
                match tag {
                    "actionmap" => { if let Some(am) = current_am.take() { result.action_maps.push(am); } }
                    "action" => { current_action = None; }
                    "deviceoptions" => { if let Some(o) = current_opts.take() { result.device_options.push(o); } }
                    _ => {}
                }
            }
            Err(e) => return Err(e.to_string()), _ => {}
        }
        buf.clear();
    }
    Ok(result)
}

fn get_attr(e: &BytesStart, name: &[u8]) -> Option<String> {
    for a in e.attributes().flatten() { if a.key.as_ref() == name { return String::from_utf8(a.value.to_vec()).ok(); } }
    None
}

#[tauri::command] pub async fn reorder_devices(game_path: String, version: String, new_order: Vec<DeviceReorderEntry>) -> Result<(), String> {
    let exp = expand_tilde(&game_path); let p = sc_profile_dir(&exp, &version).join("actionmaps.xml");
    if !p.exists() { return Err("Not found".to_string()); }
    let mut content = fs::read_to_string(&p).map_err(|e| e.to_string())?;
    for entry in &new_order { let old = format!("instance=\"{}\"", entry.old_instance); let placeholder = format!("instance=\"__REMAP_{}__\"", entry.new_instance); content = content.replace(&old, &placeholder); }
    for entry in &new_order { let placeholder = format!("instance=\"__REMAP_{}__\"", entry.new_instance); let final_val = format!("instance=\"{}\"", entry.new_instance); content = content.replace(&placeholder, &final_val); }
    fs::write(p, content).map_err(|e| e.to_string())
}

#[tauri::command] pub fn get_action_definitions() -> ActionDefinitions { ActionDefinitions::new() }

#[tauri::command] pub async fn assign_binding(game_path: String, version: String, action_name: String, category: String, input: String) -> Result<(), String> {
    let p = sc_profile_dir(&expand_tilde(&game_path), &version).join("actionmaps.xml");
    if !p.exists() { return Err("Not found".to_string()); }
    let mut parsed = parse_actionmaps_xml(&fs::read_to_string(&p).map_err(|e| e.to_string())?)?;
    let mut found = false;
    for am in &mut parsed.action_maps { if let Some(b) = am.bindings.iter_mut().find(|b| b.action_name == action_name) { b.input = input.clone(); found = true; break; } }
    if !found {
        let cat = if !category.is_empty() { category } else { "general".to_string() };
        if let Some(am) = parsed.action_maps.iter_mut().find(|am| am.name == cat) { am.bindings.push(ScBinding { action_name: action_name.clone(), input }); }
        else { parsed.action_maps.push(ScActionMap { name: cat, bindings: vec![ScBinding { action_name: action_name.clone(), input }], actions: vec![action_name] }); }
    }
    write_actionmaps_xml(&p, &parsed)
}

#[tauri::command] pub async fn remove_binding(game_path: String, version: String, action_name: String, input: String, _category: String) -> Result<(), String> {
    let p = sc_profile_dir(&expand_tilde(&game_path), &version).join("actionmaps.xml");
    let mut parsed = parse_actionmaps_xml(&fs::read_to_string(&p).map_err(|e| e.to_string())?)?;
    for am in &mut parsed.action_maps { am.bindings.retain(|b| !(b.action_name == action_name && b.input == input)); }
    write_actionmaps_xml(&p, &parsed)
}

fn write_actionmaps_xml(path: &Path, parsed: &ParsedActionMaps) -> Result<(), String> {
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);
    writer.write_event(Event::Decl(quick_xml::events::BytesDecl::new("1.0", Some("UTF-8"), None))).ok();
    let mut root = BytesStart::new("ActionMaps");
    root.push_attribute(("version", "1"));
    writer.write_event(Event::Start(root)).ok();
    for am in &parsed.action_maps {
        let mut am_tag = BytesStart::new("actionmap");
        am_tag.push_attribute(("name", am.name.as_str()));
        writer.write_event(Event::Start(am_tag)).ok();
        for b in &am.bindings {
            let mut a_tag = BytesStart::new("action");
            a_tag.push_attribute(("name", b.action_name.as_str()));
            writer.write_event(Event::Start(a_tag)).ok();
            let mut r_tag = BytesStart::new("rebind");
            r_tag.push_attribute(("input", b.input.as_str()));
            writer.write_event(Event::Empty(r_tag)).ok();
            writer.write_event(Event::End(quick_xml::events::BytesEnd::new("action"))).ok();
        }
        writer.write_event(Event::End(quick_xml::events::BytesEnd::new("actionmap"))).ok();
    }
    writer.write_event(Event::End(quick_xml::events::BytesEnd::new("ActionMaps"))).ok();
    let res = writer.into_inner().into_inner();
    fs::write(path, String::from_utf8(res).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command] pub async fn backup_profile_manual(game_path: String, version: String, label: String) -> Result<BackupInfo, String> {
    backup_profile(game_path, version, Some("manual".to_string()), Some(label)).await
}

#[tauri::command] pub async fn backup_profile(game_path: String, version: String, backup_type: Option<String>, label: Option<String>) -> Result<BackupInfo, String> {
    let now = Local::now();
    let id = now.format("%Y-%m-%dT%H-%M-%S").to_string();
    let b_dir = backup_version_dir(&version)?.join(&id);
    fs::create_dir_all(&b_dir).map_err(|e| e.to_string())?;
    let p_dir = sc_profile_dir(&expand_tilde(&game_path), &version);
    let mut files = Vec::new();
    for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
        if p_dir.join(f).exists() { fs::copy(p_dir.join(f), b_dir.join(f)).ok(); files.push(f.to_string()); }
    }
    let info = BackupInfo { id, created_at: now.format("%Y-%m-%d %H:%M:%S").to_string(), timestamp: now.timestamp() as u64, version, backup_type: backup_type.unwrap_or("manual".to_string()), files, label: label.unwrap_or_default() };
    fs::write(b_dir.join("backup_meta.json"), serde_json::to_string_pretty(&info).unwrap()).ok();
    Ok(info)
}

#[tauri::command] pub async fn restore_profile(game_path: String, version: String, backup_id: String) -> Result<(), String> {
    let b_dir = backup_version_dir(&version)?.join(backup_id);
    let p_dir = sc_profile_dir(&expand_tilde(&game_path), &version);
    fs::create_dir_all(&p_dir).ok();
    for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
        if b_dir.join(f).exists() { fs::copy(b_dir.join(f), p_dir.join(f)).ok(); }
    }
    Ok(())
}

#[tauri::command] pub async fn list_backups(version: String) -> Result<Vec<BackupInfo>, String> {
    let d = backup_version_dir(&version)?;
    if !d.is_dir() { return Ok(Vec::new()); }
    let mut res = Vec::new();
    if let Ok(entries) = fs::read_dir(d) {
        for entry in entries.flatten() {
            if let Ok(c) = fs::read_to_string(entry.path().join("backup_meta.json")) {
                if let Ok(info) = serde_json::from_str::<BackupInfo>(&c) { res.push(info); }
            }
        }
    }
    res.sort_by_key(|b| std::cmp::Reverse(b.timestamp));
    Ok(res)
}

#[tauri::command] pub async fn update_backup_label(version: String, backup_id: String, label: String) -> Result<(), String> {
    let p = backup_version_dir(&version)?.join(backup_id).join("backup_meta.json");
    let mut info: BackupInfo = serde_json::from_str(&fs::read_to_string(&p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    info.label = label;
    fs::write(p, serde_json::to_string_pretty(&info).unwrap()).map_err(|e| e.to_string())
}

#[tauri::command] pub async fn delete_backup(version: String, backup_id: String) -> Result<(), String> {
    let p = backup_version_dir(&version)?.join(backup_id);
    if p.is_dir() { fs::remove_dir_all(p).map_err(|e| e.to_string()) } else { Ok(()) }
}

#[tauri::command] pub async fn list_exported_layouts(game_path: String, version: String) -> Result<Vec<ExportedLayout>, String> {
    let d = sc_mappings_dir(&expand_tilde(&game_path), &version);
    if !d.is_dir() { return Ok(Vec::new()); }
    let mut res = Vec::new();
    if let Ok(entries) = fs::read_dir(d) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "xml") {
                let filename = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                let modified = path.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                res.push(ExportedLayout { label: filename.trim_end_matches(".xml").replace('_', " "), filename, modified });
            }
        }
    }
    res.sort_by_key(|l| std::cmp::Reverse(l.modified));
    Ok(res)
}

#[tauri::command] pub async fn parse_actionmaps(game_path: String, version: String, source: Option<String>) -> Result<ParsedActionMaps, String> {
    let exp = expand_tilde(&game_path);
    let p = match source { Some(f) => sc_mappings_dir(&exp, &version).join(f), None => sc_profile_dir(&exp, &version).join("actionmaps.xml") };
    if !p.exists() { return Err("File not found".to_string()); }
    parse_actionmaps_xml(&fs::read_to_string(p).map_err(|e| e.to_string())?)
}
