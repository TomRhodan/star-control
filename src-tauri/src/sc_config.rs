//! Star Citizen configuration and profile management.
//!
//! This module handles reading and writing Star Citizen configuration files,
//! including user profiles, action maps (key bindings), attributes, and
//! localization data. It also provides P4K (ZIP64) archive access for
//! reading default bindings and localization from the game's data files.
//!
//! Key functionality:
//! - Detecting installed SC versions (LIVE, PTU, EPTU, HOTFIX)
//! - Parsing and writing `actionmaps.xml` (user key bindings)
//! - Reading CryXmlB binary XML from P4K archives (master bindings)
//! - Localization label extraction from `global.ini`
//! - Profile backup and restore

use std::collections::{ HashMap, HashSet };
use std::time::UNIX_EPOCH;
use std::fs::{ self, File };
use std::io::{ Cursor, Read as IoRead, Seek, SeekFrom };
use sha2::{ Sha256, Digest };
use std::path::{ Path, PathBuf };
use chrono::Local;
use serde::{ Deserialize, Serialize };
use quick_xml::Reader;
use quick_xml::events::{ Event, BytesStart, BytesEnd, BytesDecl };
use tokio::sync::Mutex;
use tauri::Emitter;
use once_cell::sync::Lazy;
use quick_xml::writer::Writer;
use encoding_rs::UTF_16LE;
use crate::action_definitions::{
    CompleteBinding,
    BindingStats,
    BindingListResponse,
    ActionDefinitions,
    DeviceMapping,
};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ScDeviceOptions {
    pub name: String,
    pub options: Vec<ScDeviceOption>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScDeviceOption {
    pub input: String,
    pub deadzone: Option<f64>,
    pub saturation: Option<f64>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScDevice {
    pub device_type: String,
    pub instance: u32,
    pub product: String,
    pub guid: Option<String>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScBinding {
    pub action_name: String,
    pub input: String,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScAction {
    pub name: String,
    pub label: Option<String>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScActionMap {
    pub name: String,
    pub bindings: Vec<ScBinding>,
    pub actions: Vec<ScAction>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScActionProfile {
    pub profile_name: String,
    pub version: String,
    pub options_version: String,
    pub rebind_version: String,
    pub devices: Vec<ScDevice>,
    pub device_options: Vec<ScDeviceOptions>,
    pub action_maps: Vec<ScActionMap>,
}
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ParsedActionMaps {
    pub version: String,
    pub profiles: Vec<ScActionProfile>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct ScVersionInfo {
    pub version: String,
    pub path: String,
    pub has_usercfg: bool,
    pub has_attributes: bool,
    pub has_actionmaps: bool,
    pub has_exported_layouts: bool,
    pub has_custom_characters: bool,
    pub has_data_p4k: bool,
}
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct ScAttribute {
    pub name: String,
    pub value: String,
}
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct ScAttributes {
    pub version: String,
    pub attrs: Vec<ScAttribute>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct BackupInfo {
    pub id: String,
    pub created_at: String,
    pub timestamp: u64,
    pub version: String,
    pub backup_type: String,
    pub files: Vec<String>,
    pub label: String,
    #[serde(default)]
    pub file_hashes: HashMap<String, String>,
    #[serde(default)]
    pub device_map: Vec<DeviceMapping>,
    #[serde(default)]
    pub dirty: bool,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct VersionImportInfo {
    pub version: String,
    pub has_profiles: bool,
    pub has_controls_mappings: bool,
    pub has_custom_characters: bool,
    pub profile_file_count: u32,
    pub controls_file_count: u32,
    pub character_file_count: u32,
    pub score: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ImportResult {
    pub profiles_copied: u32,
    pub controls_copied: u32,
    pub characters_copied: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ExportedLayout {
    pub filename: String,
    pub label: String,
    pub modified: u64,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct DeviceReorderEntry {
    #[serde(rename = "deviceType")] pub device_type: String,
    #[serde(rename = "oldInstance")] pub old_instance: u32,
    #[serde(rename = "newInstance")] pub new_instance: u32,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct ScProfile {
    pub name: String,
    pub last_played: u64,
}
#[derive(Serialize, Deserialize)]
struct CachedLocalization {
    p4k_size: u64,
    p4k_modified: u64,
    labels: HashMap<String, String>,
}
#[derive(Serialize, Deserialize)]
struct CachedMasterBindings {
    p4k_size: u64,
    p4k_modified: u64,
    data: ParsedActionMaps,
}

static LOCALIZATION_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static MASTER_BINDINGS_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub(crate) fn expand_tilde(p: &str) -> String {
    if p.starts_with('~') {
        if let Some(h) = dirs::home_dir() {
            return p.replacen('~', &h.to_string_lossy(), 1);
        }
    }
    p.to_string()
}
pub fn sc_base_dir(gp: &str, v: &str) -> PathBuf {
    Path::new(gp).join("drive_c/Program Files/Roberts Space Industries/StarCitizen").join(v)
}
fn sc_p4k_path(gp: &str, v: &str) -> Result<PathBuf, String> {
    let p = sc_base_dir(gp, v).join("Data.p4k");
    if p.exists() {
        Ok(p)
    } else {
        Err("No P4K".into())
    }
}
fn master_bindings_cache_path(v: &str) -> Result<PathBuf, String> {
    Ok(
        dirs
            ::config_dir()
            .ok_or("No config dir")?
            .join("star-control/cache")
            .join(format!("master_bindings_{}.json", v))
    )
}
fn localization_cache_path(v: &str) -> Result<PathBuf, String> {
    Ok(
        dirs
            ::config_dir()
            .ok_or("No config dir")?
            .join("star-control/cache")
            .join(format!("localization_{}.json", v))
    )
}
fn backup_version_dir(v: &str) -> Result<PathBuf, String> {
    Ok(dirs::config_dir().ok_or("No config dir")?.join("star-control/backups").join(v))
}

/// Finds a subdirectory matching one of the given path variants.
/// Tries each variant joined to base, returns the first that exists.
fn find_dir_case_insensitive(base: &Path, variants: &[&str]) -> Option<PathBuf> {
    for v in variants {
        let p = base.join(v);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

fn get_attr(e: &BytesStart, n: &[u8]) -> Option<String> {
    for a in e.attributes().flatten() {
        if a.key.as_ref() == n {
            return Some(String::from_utf8_lossy(&a.value).into_owned());
        }
    }
    None
}
fn parse_global_ini(c: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for l in c.lines() {
        let t = l.trim();
        if t.is_empty() || t.starts_with(';') {
            continue;
        }
        if let Some(p) = t.find('=') {
            let mut k = t[..p].trim().to_string();
            if k.starts_with('@') {
                k = k[1..].to_string();
            }
            m.insert(k, t[p + 1..].trim().to_string());
        }
    }
    m
}

fn parse_actionmaps_xml(c: &str) -> Result<ParsedActionMaps, String> {
    let mut r = Reader::from_str(c.trim().trim_matches('\0'));
    r.config_mut().trim_text(true);
    let mut res = ParsedActionMaps::default();
    let mut current_device_options: Option<ScDeviceOptions> = None;
    let (mut cp, mut cm, mut ca) = (None, None, None);
    let mut buf = Vec::new();
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Eof) => {
                break;
            }
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                match tag.as_str() {
                    "actionmaps" => {
                        res.version = get_attr(e, b"version").unwrap_or("1".into());
                    }
                    "actionprofiles" => {
                        cp = Some(ScActionProfile {
                            profile_name: get_attr(e, b"profileName")
                                .or(get_attr(e, b"profile_name"))
                                .unwrap_or_default(),
                            version: get_attr(e, b"version").unwrap_or("1".into()),
                            options_version: get_attr(e, b"optionsVersion").unwrap_or("2".into()),
                            rebind_version: get_attr(e, b"rebindVersion").unwrap_or("2".into()),
                            devices: vec![],
                            device_options: vec![],
                            action_maps: vec![],
                        });
                    }
                    "options" => {
                        if let Some(ref mut p) = cp {
                            let mut product = get_attr(e, b"Product").unwrap_or_default();
                            let guid = if product.contains('{') {
                                product
                                    .split('{')
                                    .nth(1)
                                    .and_then(|s| s.strip_suffix('}'))
                                    .map(|s| s.to_string())
                            } else {
                                None
                            };
                            // Remove GUID from product name: "Device {GUID}" -> "Device"
                            if let Some(ref g) = guid {
                                let pattern = format!("{{{}}}", g);
                                if let Some(start) = product.find(&pattern) {
                                    let end_pos = start + pattern.len();
                                    if end_pos <= product.len() {
                                        product = format!(
                                            "{}{}",
                                            &product[..start].trim_end(),
                                            &product[end_pos..]
                                        );
                                    } else {
                                        product = product[..start].trim_end().to_string();
                                    }
                                }
                            }
                            p.devices.push(ScDevice {
                                device_type: get_attr(e, b"type").unwrap_or_default(),
                                instance: get_attr(e, b"instance")
                                    .and_then(|v| v.parse().ok())
                                    .unwrap_or(0),
                                product: product.trim().to_string(),
                                guid,
                            });
                        }
                    }
                    "deviceoptions" => {
                        let name = get_attr(e, b"name").unwrap_or_default();
                        current_device_options = Some(ScDeviceOptions {
                            name,
                            options: vec![],
                        });
                    }
                    "option" => {
                        if let Some(ref mut do_opts) = current_device_options {
                            let input = get_attr(e, b"input").unwrap_or_default();
                            let deadzone = get_attr(e, b"deadzone").and_then(|v| v.parse().ok());
                            let saturation = get_attr(e, b"saturation").and_then(|v|
                                v.parse().ok()
                            );
                            do_opts.options.push(ScDeviceOption {
                                input: input.clone(),
                                deadzone,
                                saturation,
                            });
                        }
                    }
                    "actionmap" => {
                        cm = Some(ScActionMap {
                            name: get_attr(e, b"name").unwrap_or_default(),
                            bindings: vec![],
                            actions: vec![],
                        });
                    }
                    "action" => {
                        let n = get_attr(e, b"name").unwrap_or_default();
                        ca = Some(n.clone());
                        if let Some(ref mut m) = cm {
                            m.actions.push(ScAction { name: n, label: get_attr(e, b"label") });
                        }
                    }
                    "rebind" => {
                        if let (Some(ref mut m), Some(ref a)) = (cm.as_mut(), ca.as_ref()) {
                            m.bindings.push(ScBinding {
                                action_name: a.to_string(),
                                input: get_attr(e, b"input").unwrap_or_default(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
                match tag.as_str() {
                    "actionmap" => {
                        if let (Some(m), Some(ref mut p)) = (cm.take(), cp.as_mut()) {
                            p.action_maps.push(m);
                        }
                    }
                    "actionprofiles" => {
                        if let Some(p) = cp.take() {
                            res.profiles.push(p);
                        }
                    }
                    "action" => {
                        ca = None;
                    }
                    "deviceoptions" => {
                        if let Some(do_opts) = current_device_options.take() {
                            if let Some(ref mut p) = cp {
                                p.device_options.push(do_opts);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(res)
}

fn write_actionmaps_xml(p: &Path, parsed: &ParsedActionMaps) -> Result<(), String> {
    let mut w = Writer::new_with_indent(Cursor::new(vec![]), b' ', 1);
    w.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None))).ok();
    let mut root = BytesStart::new("ActionMaps");
    root.push_attribute(("version", parsed.version.as_str()));
    w.write_event(Event::Start(root)).ok();

    for po in &parsed.profiles {
        let mut p_tag = BytesStart::new("ActionProfiles");
        p_tag.push_attribute(("version", po.version.as_str()));
        p_tag.push_attribute(("optionsVersion", po.options_version.as_str()));
        p_tag.push_attribute(("rebindVersion", po.rebind_version.as_str()));
        p_tag.push_attribute(("profileName", po.profile_name.as_str()));
        w.write_event(Event::Start(p_tag)).ok();

        // Write devices with GUID in Product attribute
        for d in &po.devices {
            let mut d_tag = BytesStart::new("options");
            d_tag.push_attribute(("type", d.device_type.as_str()));
            d_tag.push_attribute(("instance", d.instance.to_string().as_str()));
            if !d.product.is_empty() {
                // Include GUID in Product attribute if available
                let product_with_guid = if let Some(ref guid) = d.guid {
                    format!("{} {{{}}}", d.product.trim(), guid)
                } else {
                    d.product.clone()
                };
                d_tag.push_attribute(("Product", product_with_guid.as_str()));
            }
            w.write_event(Event::Empty(d_tag)).ok();
        }

        // Write device options (deadzone, saturation)
        for do_opts in &po.device_options {
            let mut do_tag = BytesStart::new("deviceoptions");
            do_tag.push_attribute(("name", do_opts.name.as_str()));
            w.write_event(Event::Start(do_tag)).ok();

            for opt in &do_opts.options {
                let mut o_tag = BytesStart::new("option");
                o_tag.push_attribute(("input", opt.input.as_str()));
                if let Some(dz) = opt.deadzone {
                    o_tag.push_attribute(("deadzone", format!("{}", dz).as_str()));
                }
                if let Some(sat) = opt.saturation {
                    o_tag.push_attribute(("saturation", format!("{}", sat).as_str()));
                }
                w.write_event(Event::Empty(o_tag)).ok();
            }

            w.write_event(Event::End(BytesEnd::new("deviceoptions"))).ok();
        }

        w.write_event(Event::Empty(BytesStart::new("modifiers"))).ok();

        for am in &po.action_maps {
            let mut am_tag = BytesStart::new("actionmap");
            am_tag.push_attribute(("name", am.name.as_str()));
            w.write_event(Event::Start(am_tag)).ok();

            for b in &am.bindings {
                let mut a_tag = BytesStart::new("action");
                a_tag.push_attribute(("name", b.action_name.as_str()));
                w.write_event(Event::Start(a_tag)).ok();

                let mut r_tag = BytesStart::new("rebind");
                r_tag.push_attribute(("input", b.input.as_str()));
                w.write_event(Event::Empty(r_tag)).ok();

                w.write_event(Event::End(BytesEnd::new("action"))).ok();
            }

            w.write_event(Event::End(BytesEnd::new("actionmap"))).ok();
        }

        w.write_event(Event::End(BytesEnd::new("ActionProfiles"))).ok();
    }

    w.write_event(Event::End(BytesEnd::new("ActionMaps"))).ok();
    fs::write(
        p,
        String::from_utf8(w.into_inner().into_inner()).map_err(|e| e.to_string())?
    ).map_err(|e| e.to_string())
}

/// Finds the ZIP64 central directory in a P4K archive file.
///
/// Searches backwards from the end of the file for the ZIP64
/// end-of-central-directory locator (PK\x06\x07), then reads
/// the central directory offset and size from the EOCD64 record.
///
/// # Arguments
/// * `file` - Open file handle to the P4K archive
/// * `file_length` - Total file size in bytes
///
/// # Returns
/// Tuple of `(central_directory_offset, central_directory_size)`.
///
/// # Errors
/// Returns an error if the ZIP64 signature is not found or file I/O fails.
fn find_central_directory(file: &mut File, file_length: u64) -> Result<(u64, u64), String> {
    let search_size = (65536u64).min(file_length) as usize;
    file
        .seek(SeekFrom::End(-(search_size as i64)))
        .map_err(|e| format!("Failed to seek to end of P4K: {}", e))?;

    let mut buffer = vec![0u8; search_size];
    file.read_exact(&mut buffer).map_err(|e| format!("Failed to read P4K tail: {}", e))?;

    for i in (0..search_size.saturating_sub(4)).rev() {
        if &buffer[i..i + 4] == b"PK\x06\x07" {
            // Found ZIP64 EOCD locator — read offset to EOCD64 record
            file
                .seek(SeekFrom::Start(file_length - (search_size as u64) + (i as u64) + 8))
                .map_err(|e| format!("Failed to seek to EOCD64 locator: {}", e))?;

            let mut offset_bytes = [0u8; 8];
            file
                .read_exact(&mut offset_bytes)
                .map_err(|e| format!("Failed to read EOCD64 offset: {}", e))?;
            let eocd_offset = u64::from_le_bytes(offset_bytes);

            // Read central directory size and offset from EOCD64 record
            file
                .seek(SeekFrom::Start(eocd_offset + 40))
                .map_err(|e| format!("Failed to seek to EOCD64 record: {}", e))?;

            let mut size_bytes = [0u8; 8];
            file
                .read_exact(&mut size_bytes)
                .map_err(|e| format!("Failed to read central directory size: {}", e))?;

            let mut cd_offset_bytes = [0u8; 8];
            file
                .read_exact(&mut cd_offset_bytes)
                .map_err(|e| format!("Failed to read central directory offset: {}", e))?;

            return Ok((u64::from_le_bytes(cd_offset_bytes), u64::from_le_bytes(size_bytes)));
        }
    }

    Err("No ZIP64 end-of-central-directory locator found in P4K".into())
}

/// Reads and decompresses a single file from a Star Citizen P4K (ZIP64) archive.
///
/// P4K files use ZIP64 format with zstd-compressed entries. This function
/// locates the file in the central directory, reads the compressed data,
/// and decompresses it if necessary (methods 93/100 = zstd).
///
/// # Arguments
/// * `game_path` - Wine prefix / game base path
/// * `version` - SC version folder (e.g. "LIVE", "PTU")
/// * `file_path` - Path inside the archive (forward slashes converted to backslashes)
///
/// # Returns
/// The decompressed file contents as a byte vector.
///
/// # Errors
/// Returns an error if the P4K file is missing, the entry is not found,
/// or decompression fails.
pub fn read_p4k_file(game_path: &str, version: &str, file_path: &str) -> Result<Vec<u8>, String> {
    let p4k_path = sc_p4k_path(game_path, version)?;
    let mut file = File::open(&p4k_path).map_err(|e| e.to_string())?;
    let file_length = file
        .metadata()
        .map_err(|e| e.to_string())?
        .len();

    let (cd_offset, cd_size) = find_central_directory(&mut file, file_length)?;

    file
        .seek(SeekFrom::Start(cd_offset))
        .map_err(|e| format!("Failed to seek to central directory: {}", e))?;
    let mut central_dir = vec![0u8; cd_size as usize];
    file
        .read_exact(&mut central_dir)
        .map_err(|e| format!("Failed to read central directory: {}", e))?;

    let search_name = file_path.replace('/', "\\");
    let mut pos = 0;

    while pos + 46 <= central_dir.len() {
        if &central_dir[pos..pos + 4] != b"PK\x01\x02" {
            pos += 1;
            continue;
        }

        let name_length = u16::from_le_bytes([
            central_dir[pos + 28],
            central_dir[pos + 29],
        ]) as usize;
        let extra_length = u16::from_le_bytes([
            central_dir[pos + 30],
            central_dir[pos + 31],
        ]) as usize;
        let comment_length = u16::from_le_bytes([
            central_dir[pos + 32],
            central_dir[pos + 33],
        ]) as usize;

        if pos + 46 + name_length > central_dir.len() {
            break;
        }

        let entry_name = String::from_utf8_lossy(&central_dir[pos + 46..pos + 46 + name_length]);

        if entry_name.eq_ignore_ascii_case(&search_name) {
            let mut compressed_size = u32::from_le_bytes([
                central_dir[pos + 20],
                central_dir[pos + 21],
                central_dir[pos + 22],
                central_dir[pos + 23],
            ]) as u64;
            let mut local_header_offset = u32::from_le_bytes([
                central_dir[pos + 42],
                central_dir[pos + 43],
                central_dir[pos + 44],
                central_dir[pos + 45],
            ]) as u64;

            // Check ZIP64 extra field for large files
            if extra_length >= 28 {
                let extra_start = pos + 46 + name_length;
                if
                    extra_start + 28 <= central_dir.len() &&
                    u16::from_le_bytes([central_dir[extra_start], central_dir[extra_start + 1]]) ==
                        0x0001
                {
                    compressed_size = u64::from_le_bytes(
                        central_dir[extra_start + 12..extra_start + 20]
                            .try_into()
                            .map_err(|_| "Invalid ZIP64 compressed size field")?
                    );
                    local_header_offset = u64::from_le_bytes(
                        central_dir[extra_start + 20..extra_start + 28]
                            .try_into()
                            .map_err(|_| "Invalid ZIP64 offset field")?
                    );
                }
            }

            let compression_method = u16::from_le_bytes([
                central_dir[pos + 10],
                central_dir[pos + 11],
            ]);

            // Read local file header to find actual data offset
            file
                .seek(SeekFrom::Start(local_header_offset))
                .map_err(|e| format!("Failed to seek to local header: {}", e))?;
            let mut local_header = [0u8; 30];
            file
                .read_exact(&mut local_header)
                .map_err(|e| format!("Failed to read local header: {}", e))?;

            let local_name_len = u16::from_le_bytes([local_header[26], local_header[27]]) as u64;
            let local_extra_len = u16::from_le_bytes([local_header[28], local_header[29]]) as u64;
            let data_offset = local_header_offset + 30 + local_name_len + local_extra_len;

            file
                .seek(SeekFrom::Start(data_offset))
                .map_err(|e| format!("Failed to seek to file data: {}", e))?;
            let mut compressed_data = vec![0u8; compressed_size as usize];
            file
                .read_exact(&mut compressed_data)
                .map_err(|e| format!("Failed to read file data: {}", e))?;

            // Decompress zstd (compression methods 93 and 100)
            if compression_method == 100 || compression_method == 93 {
                let mut decoder = zstd::Decoder
                    ::new(Cursor::new(compressed_data))
                    .map_err(|e| format!("zstd init error: {}", e))?;
                let mut decompressed = vec![];
                decoder
                    .read_to_end(&mut decompressed)
                    .map_err(|e| format!("zstd decode error: {}", e))?;
                return Ok(decompressed);
            }

            return Ok(compressed_data);
        }

        pos += 46 + name_length + extra_length + comment_length;
    }

    Err("File not found in P4K archive".into())
}

/// Reads a null-terminated C string from a byte buffer at the given offset.
fn read_c_string(string_table: &[u8], offset: usize) -> String {
    if offset >= string_table.len() {
        return String::new();
    }
    let length = string_table[offset..]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(string_table.len() - offset);
    String::from_utf8_lossy(&string_table[offset..offset + length]).into_owned()
}

/// Recursively traverses a CryXmlB binary XML node tree and extracts action maps.
///
/// Processes nodes to find `actionmap`, `action`, and `rebind` elements
/// that define Star Citizen's default key bindings.
///
/// # Arguments
/// * `data` - Complete CryXmlB file data
/// * `node_table_offset` - Byte offset to the node table
/// * `attr_table_offset` - Byte offset to the attribute table
/// * `string_table` - Slice containing all null-terminated strings
/// * `child_table` - Slice containing child node indices (4 bytes each)
/// * `node_index` - Index of the current node to process
/// * `profile` - Action profile being built
/// * `current_map_index` - Index of the current action map, if inside one
#[allow(clippy::too_many_arguments)]
fn traverse_xml_node(
    data: &[u8],
    node_table_offset: usize,
    attr_table_offset: usize,
    string_table: &[u8],
    child_table: &[u8],
    node_index: usize,
    profile: &mut ScActionProfile,
    mut current_map_index: Option<usize>
) {
    let node_start = node_table_offset + node_index * 28;
    if node_start + 28 > data.len() {
        return;
    }

    let node_data = &data[node_start..node_start + 28];
    let tag_offset = u32::from_le_bytes(node_data[0..4].try_into().unwrap_or_default()) as usize;
    let tag = read_c_string(string_table, tag_offset).to_lowercase();

    if tag == "xboxone" || tag == "gamepad" {
        return;
    }

    let attr_count = u16::from_le_bytes(node_data[8..10].try_into().unwrap_or_default()) as usize;
    let child_count = u16::from_le_bytes(node_data[10..12].try_into().unwrap_or_default()) as usize;
    let first_attr_index = u32::from_le_bytes(
        node_data[16..20].try_into().unwrap_or_default()
    ) as usize;
    let first_child_index = u32::from_le_bytes(
        node_data[20..24].try_into().unwrap_or_default()
    ) as usize;

    // Collect attributes for this node
    let mut attrs = HashMap::new();
    for i in 0..attr_count {
        let attr_offset = attr_table_offset + (first_attr_index + i) * 8;
        if attr_offset + 8 > data.len() {
            continue;
        }
        let key_offset = u32::from_le_bytes(
            data[attr_offset..attr_offset + 4].try_into().unwrap_or_default()
        ) as usize;
        let value_offset = u32::from_le_bytes(
            data[attr_offset + 4..attr_offset + 8].try_into().unwrap_or_default()
        ) as usize;
        let key = read_c_string(string_table, key_offset).to_lowercase();
        let value = read_c_string(string_table, value_offset);
        attrs.insert(key, value);
    }

    // Build action maps from recognized tags
    if tag == "actionmap" {
        if let Some(name) = attrs.get("name") {
            current_map_index = Some(profile.action_maps.len());
            profile.action_maps.push(ScActionMap {
                name: name.clone(),
                bindings: vec![],
                actions: vec![],
            });
        }
    } else if tag == "action" {
        if let (Some(idx), Some(name)) = (current_map_index, attrs.get("name").or(attrs.get("id"))) {
            profile.action_maps[idx].actions.push(ScAction {
                name: name.clone(),
                label: attrs.get("label").or(attrs.get("uilabel")).cloned(),
            });
        }
    } else if tag == "rebind" {
        if let (Some(idx), Some(input)) = (current_map_index, attrs.get("input")) {
            if let Some(last_action) = profile.action_maps[idx].actions.last() {
                let action_name = last_action.name.clone();
                profile.action_maps[idx].bindings.push(ScBinding {
                    action_name,
                    input: input.clone(),
                });
            }
        }
    }

    // Recurse into child nodes
    for i in 0..child_count {
        let child_offset = (first_child_index + i) * 4;
        if child_offset + 4 <= child_table.len() {
            let child_node_index = u32::from_le_bytes(
                child_table[child_offset..child_offset + 4].try_into().unwrap_or_default()
            ) as usize;
            traverse_xml_node(
                data,
                node_table_offset,
                attr_table_offset,
                string_table,
                child_table,
                child_node_index,
                profile,
                current_map_index
            );
        }
    }
}

/// Parses a CryXmlB binary XML file containing Star Citizen's default action maps.
///
/// CryXmlB is Star Citizen's binary XML format. The file header contains offsets
/// to node, attribute, child, and string tables which are traversed recursively
/// to extract all default key bindings.
///
/// # Arguments
/// * `data` - Raw CryXmlB file bytes (must start with `CryXmlB` magic)
///
/// # Returns
/// Parsed action maps containing all default bindings under a "master" profile.
///
/// # Errors
/// Returns an error if the magic bytes don't match, the header is too short,
/// or table offsets extend beyond the file boundary.
fn parse_cryxmlb_full(data: &[u8]) -> Result<ParsedActionMaps, String> {
    if !data.starts_with(b"CryXmlB") {
        return Err("Not a CryXmlB file".into());
    }
    if data.len() < 44 {
        return Err("CryXmlB header too short".into());
    }

    let node_table_offset = u32::from_le_bytes(
        data[12..16].try_into().map_err(|_| "Invalid node table offset")?
    ) as usize;
    let attr_table_offset = u32::from_le_bytes(
        data[20..24].try_into().map_err(|_| "Invalid attr table offset")?
    ) as usize;
    let child_table_offset = u32::from_le_bytes(
        data[28..32].try_into().map_err(|_| "Invalid child table offset")?
    ) as usize;
    let child_table_count = u32::from_le_bytes(
        data[32..36].try_into().map_err(|_| "Invalid child table count")?
    ) as usize;
    let string_table_offset = u32::from_le_bytes(
        data[36..40].try_into().map_err(|_| "Invalid string table offset")?
    ) as usize;
    let string_table_size = u32::from_le_bytes(
        data[40..44].try_into().map_err(|_| "Invalid string table size")?
    ) as usize;

    if string_table_offset + string_table_size > data.len() {
        return Err("String table extends beyond file boundary".into());
    }
    if child_table_offset + child_table_count * 4 > data.len() {
        return Err("Child table extends beyond file boundary".into());
    }

    let string_table = &data[string_table_offset..string_table_offset + string_table_size];
    let child_table = &data[child_table_offset..child_table_offset + child_table_count * 4];

    let mut profile = ScActionProfile {
        profile_name: "master".into(),
        version: "1".into(),
        options_version: "2".into(),
        rebind_version: "2".into(),
        devices: vec![],
        device_options: vec![],
        action_maps: vec![],
    };

    traverse_xml_node(
        data,
        node_table_offset,
        attr_table_offset,
        string_table,
        child_table,
        0,
        &mut profile,
        None
    );

    Ok(ParsedActionMaps {
        version: "1".into(),
        profiles: vec![profile],
    })
}

/// Reads the USER.cfg file for the given game path and version.
#[tauri::command]
pub async fn read_user_cfg(gp: String, v: String) -> Result<String, String> {
    let p = sc_base_dir(&expand_tilde(&gp), &v).join("USER.cfg");
    if !p.exists() {
        return Ok("".into());
    }
    fs::read_to_string(p).map_err(|e| e.to_string())
}
/// Writes content to the USER.cfg file for the given game path and version.
#[tauri::command]
pub async fn write_user_cfg(gp: String, v: String, c: String) -> Result<(), String> {
    let p = sc_base_dir(&expand_tilde(&gp), &v).join("USER.cfg");
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(p, c).map_err(|e| e.to_string())
}
/// Detects installed Star Citizen versions (LIVE, PTU, EPTU, HOTFIX) at the given path.
#[tauri::command]
pub async fn detect_sc_versions(gp: String) -> Result<Vec<ScVersionInfo>, String> {
    log::debug!("[detect_sc_versions] ========== START ==========");
    log::debug!("[detect_sc_versions] Input game_path: '{}'", gp);

    let exp = expand_tilde(&gp);
    log::debug!("[detect_sc_versions] Expanded path: '{}'", exp);

    // Try multiple possible paths
    let paths_to_try: Vec<PathBuf> = vec![
        // Wine prefix path (standard)
        Path::new(&exp).join("drive_c/Program Files/Roberts Space Industries/StarCitizen"),
        // Direct Linux installation
        Path::new(&exp).join("StarCitizen"),
        // The game path itself might be the StarCitizen folder
        Path::new(&exp).to_path_buf()
    ];

    for base in paths_to_try.iter() {
        log::debug!("[detect_sc_versions] Checking path: {}", base.display());
        log::debug!("[detect_sc_versions]   Exists: {}, IsDir: {}", base.exists(), base.is_dir());

        if base.exists() && base.is_dir() {
            // List contents of directory for debugging
            if let Ok(entries) = fs::read_dir(base) {
                let entry_names: Vec<String> = entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .take(10)
                    .collect();
                log::debug!("[detect_sc_versions]   First 10 entries: {:?}", entry_names);

                let has_version_folders: bool = entry_names.iter().any(|name| {
                    let n = name.to_lowercase();
                    n == "live" || n == "ptu" || n == "eptu" || n == "hotfix"
                });

                if has_version_folders {
                    log::debug!(
                        "[detect_sc_versions] Found valid SC installation at: {}",
                        base.display()
                    );
                    return detect_sc_versions_from_path(base);
                } else {
                    log::debug!(
                        "[detect_sc_versions]   No version folders found in this directory"
                    );
                }
            }
        }
    }

    // Return error with path info for debugging
    log::debug!("[detect_sc_versions] ========== END - NOT FOUND ==========");
    Err(format!("StarCitizen directory not found. Game path: '{}'", gp))
}

fn detect_sc_versions_from_path(base: &Path) -> Result<Vec<ScVersionInfo>, String> {
    let mut res = vec![];
    log::debug!("[detect_sc_versions] Reading directory: {}", base.display());

    match fs::read_dir(base) {
        Ok(es) => {
            for e in es.flatten() {
                let path = e.path();
                if !path.is_dir() {
                    continue;
                }
                let n = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                log::debug!("[detect_sc_versions] Found version folder: {}", n);

                // Check profiles path
                let profiles_path = path.join("user/client/0/Profiles/default");
                let has_usercfg = path.join("USER.cfg").exists();
                let has_attributes = profiles_path.join("attributes.xml").exists();
                let has_actionmaps = profiles_path.join("actionmaps.xml").exists();
                let has_exported_layouts = path.join("user/client/0/controls/mappings").is_dir();
                let user_base = path.join("user/client/0");
                let has_custom_characters = find_dir_case_insensitive(&user_base, &["CustomCharacters", "customcharacters"]).map_or(false, |d| {
                    fs::read_dir(&d).map_or(false, |mut es| es.any(|e| e.ok().map_or(false, |e| e.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("chf")))))
                });
                let has_data_p4k = path.join("Data.p4k").exists();
                log::debug!(
                    "[detect_sc_versions]   has_usercfg: {}, has_attributes: {}, has_actionmaps: {}",
                    has_usercfg,
                    has_attributes,
                    has_actionmaps
                );
                res.push(ScVersionInfo {
                    version: n,
                    path: path.to_string_lossy().into_owned(),
                    has_usercfg,
                    has_attributes,
                    has_actionmaps,
                    has_exported_layouts,
                    has_custom_characters,
                    has_data_p4k,
                });
            }
        }
        Err(e) => {
            log::debug!(
                "[detect_sc_versions] Failed to read directory: {} - Error: {}",
                base.display(),
                e
            );
        }
    }

    res.sort_by_key(|v| {
        match v.version.as_str() {
            "LIVE" => 0,
            "PTU" => 1,
            "HOTFIX" => 2,
            _ => 3,
        }
    });
    log::debug!("[detect_sc_versions] Returning {} versions", res.len());
    Ok(res)
}
/// Lists available user profiles for the given version.
#[tauri::command]
pub async fn list_profiles(gp: String, v: String) -> Result<Vec<ScProfile>, String> {
    let p = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles");
    if !p.is_dir() {
        return Ok(vec![]);
    }
    let mut res = vec![];
    if let Ok(es) = fs::read_dir(p) {
        for e in es.flatten() {
            let path = e.path();
            if !path.is_dir() || path.file_name().unwrap_or_default() == "frontend" {
                continue;
            }
            let n = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let mut last = 0;
            if let Ok(c) = fs::read_to_string(path.join("attributes.xml")) {
                if let Some(s) = c.find("lastPlayed=\"") {
                    if let Some(e) = c[s + 12..].find('"') {
                        last = c[s + 12..s + 12 + e].parse().unwrap_or(0);
                    }
                }
            }
            res.push(ScProfile { name: n, last_played: last });
        }
    }
    res.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(res)
}
/// Exports the default profile (actionmaps, attributes, profile) to a destination directory.
#[tauri::command]
pub async fn export_profile(gp: String, v: String, dp: String) -> Result<(), String> {
    let src = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles/default");
    let dest = Path::new(&dp);
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
        if src.join(f).exists() {
            fs::copy(src.join(f), dest.join(f)).ok();
        }
    }
    Ok(())
}
/// Imports profile files from a source directory into the default profile.
#[tauri::command]
pub async fn import_profile(gp: String, v: String, sp: String) -> Result<(), String> {
    let dest = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles/default");
    let src = Path::new(&sp);
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
        if src.join(f).exists() {
            fs::copy(src.join(f), dest.join(f)).ok();
        }
    }
    Ok(())
}
/// Reads and parses the attributes.xml file from the default profile.
#[tauri::command]
pub async fn read_attributes(gp: String, v: String) -> Result<ScAttributes, String> {
    let p = sc_base_dir(&expand_tilde(&gp), &v).join(
        "user/client/0/Profiles/default/attributes.xml"
    );
    if !p.exists() {
        return Ok(ScAttributes::default());
    }
    let c = fs::read_to_string(p).map_err(|e| e.to_string())?;
    let mut attrs = ScAttributes::default();
    if let Some(s) = c.find("Version=\"").or(c.find("version=\"")) {
        let st = s + 9;
        if let Some(e) = c[st..].find('"') {
            attrs.version = c[st..st + e].to_string();
        }
    }
    let mut pos = 0;
    while let Some(s) = c[pos..].find("<Attr ") {
        let st = pos + s;
        if let Some(ns) = c[st..].find("name=\"") {
            let nst = st + ns + 6;
            if let Some(ne) = c[nst..].find('"') {
                let n = c[nst..nst + ne].to_string();
                if let Some(vs) = c[nst + ne..].find("value=\"") {
                    let vst = nst + ne + vs + 7;
                    if let Some(ve) = c[vst..].find('"') {
                        attrs.attrs.push(ScAttribute {
                            name: n,
                            value: c[vst..vst + ve].to_string(),
                        });
                    }
                }
            }
        }
        pos = st + 1;
    }
    Ok(attrs)
}
/// Writes attributes back to attributes.xml in the default profile.
#[tauri::command]
pub async fn write_attributes(gp: String, v: String, attrs: ScAttributes) -> Result<(), String> {
    let p = sc_base_dir(&expand_tilde(&gp), &v).join(
        "user/client/0/Profiles/default/attributes.xml"
    );
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).ok();
    }
    let mut xml = format!("<Attributes Version=\"{}\">\n", attrs.version);
    for a in attrs.attrs {
        xml.push_str(&format!(" <Attr name=\"{}\" value=\"{}\"/>\n", a.name, a.value));
    }
    xml.push_str("</Attributes>\n");
    fs::write(p, xml).map_err(|e| e.to_string())
}
/// Parses actionmaps.xml from the default profile, or from an exported layout file if `source` is given.
#[tauri::command]
pub async fn parse_actionmaps(
    gp: String,
    v: String,
    source: Option<String>
) -> Result<ParsedActionMaps, String> {
    let exp = expand_tilde(&gp);
    let p = match source {
        Some(f) => sc_base_dir(&exp, &v).join("user/client/0/controls/mappings").join(f),
        None => sc_base_dir(&exp, &v).join("user/client/0/Profiles/default/actionmaps.xml"),
    };
    if !p.exists() {
        return Err("Not found".into());
    }
    parse_actionmaps_xml(&fs::read_to_string(p).map_err(|e| e.to_string())?)
}
/// Returns the built-in action definitions (categories and their actions).
#[tauri::command]
pub fn get_action_definitions() -> ActionDefinitions {
    ActionDefinitions::new()
}
pub async fn get_master_bindings(gp: String, v: String) -> Result<ParsedActionMaps, String> {
    let pp = sc_p4k_path(&gp, &v)?;
    let cp = master_bindings_cache_path(&v)?;
    let meta = fs::metadata(&pp).map_err(|e| e.to_string())?;
    let sz = meta.len();
    let modif = meta
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if cp.exists() {
        if let Ok(c) = fs::read_to_string(&cp) {
            if let Ok(cached) = serde_json::from_str::<CachedMasterBindings>(&c) {
                if cached.p4k_size == sz && cached.p4k_modified == modif {
                    return Ok(cached.data);
                }
            }
        }
    }

    let _g = MASTER_BINDINGS_LOCK.lock().await;
    let res: Result<ParsedActionMaps, String> = tokio::task
        ::spawn_blocking(move || {
            let p4k = sc_base_dir(&gp, &v).join("Data.p4k");
            if !p4k.exists() {
                return Err("No P4K file found".into());
            }

            let mut file = File::open(&p4k).map_err(|e| e.to_string())?;
            let file_length = file
                .metadata()
                .map_err(|e| e.to_string())?
                .len();
            let (cd_offset, cd_size) = find_central_directory(&mut file, file_length)?;

            file
                .seek(SeekFrom::Start(cd_offset))
                .map_err(|e| format!("Failed to seek to central directory: {}", e))?;
            let mut central_dir = vec![0u8; cd_size as usize];
            file
                .read_exact(&mut central_dir)
                .map_err(|e| format!("Failed to read central directory: {}", e))?;

            let pattern = "defaultprofile.xml";
            let mut found_files = vec![];
            let mut pos = 0;
            while pos + 46 <= central_dir.len() {
                if &central_dir[pos..pos + 4] != b"PK\x01\x02" {
                    pos += 1;
                    continue;
                }
                let name_length = u16::from_le_bytes([
                    central_dir[pos + 28],
                    central_dir[pos + 29],
                ]) as usize;
                if pos + 46 + name_length > central_dir.len() {
                    break;
                }
                let name = String::from_utf8_lossy(&central_dir[pos + 46..pos + 46 + name_length]);
                if name.to_lowercase().contains(pattern) {
                    found_files.push(name.to_string());
                }
                let extra_length = u16::from_le_bytes([
                    central_dir[pos + 30],
                    central_dir[pos + 31],
                ]) as usize;
                let comment_length = u16::from_le_bytes([
                    central_dir[pos + 32],
                    central_dir[pos + 33],
                ]) as usize;
                pos += 46 + name_length + extra_length + comment_length;
            }

            let master_path = found_files
                .iter()
                .find(|f| f.ends_with("defaultProfile.xml"))
                .ok_or("No master bindings file found in P4K")?;

            let master_raw = read_p4k_file(&gp, &v, master_path)?;
            let data = parse_cryxmlb_full(&master_raw)?;

            let cached = CachedMasterBindings {
                p4k_size: sz,
                p4k_modified: modif,
                data: data.clone(),
            };
            let cp_path = master_bindings_cache_path(&v)?;
            if let Some(parent) = cp_path.parent() {
                fs::create_dir_all(parent).ok();
            }
            if let Ok(json) = serde_json::to_string(&cached) {
                fs::write(cp_path, json).ok();
            }

            Ok(data)
        }).await
        .map_err(|e| format!("Task failed: {}", e))?;
    res
}
/// Extracts localization labels from the P4K archive for UI display.
///
/// Labels are cached to avoid re-reading the P4K on subsequent calls.
/// The cache is invalidated when the P4K file size or modification time changes.
///
/// # Arguments
/// * `game_path` - Wine prefix / game base path
/// * `version` - SC version folder (e.g. "LIVE")
/// * `language` - Language name (defaults to "english")
///
/// # Returns
/// Map of localization keys to translated strings.
#[tauri::command]
pub async fn get_localization_labels(
    game_path: String,
    version: String,
    language: Option<String>
) -> Result<HashMap<String, String>, String> {
    let pp = sc_p4k_path(&game_path, &version)?;
    let cp = localization_cache_path(&version)?;
    let meta = fs::metadata(&pp).map_err(|e| e.to_string())?;
    let sz = meta.len();
    let modif = meta
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if cp.exists() {
        if let Ok(s) = fs::read_to_string(&cp) {
            if let Ok(cached) = serde_json::from_str::<CachedLocalization>(&s) {
                if cached.p4k_size == sz && cached.p4k_modified == modif {
                    return Ok(cached.labels);
                }
            }
        }
    }

    let _g = LOCALIZATION_LOCK.lock().await;
    let lang = language.unwrap_or_else(|| "english".into());

    let res: Result<HashMap<String, String>, String> = tokio::task
        ::spawn_blocking(move || {
            let bytes = read_p4k_file(
                &game_path,
                &version,
                &format!("Localization/{}/global.ini", lang)
            ).or_else(|_|
                read_p4k_file(
                    &game_path,
                    &version,
                    &format!("Data/Localization/{}/global.ini", lang)
                )
            )?;

            let content = if bytes.starts_with(&[0xff, 0xfe]) {
                UTF_16LE.decode(&bytes[2..]).0.into_owned()
            } else {
                String::from_utf8_lossy(&bytes).into_owned()
            };

            let labels = parse_global_ini(&content);
            let cached = CachedLocalization {
                p4k_size: sz,
                p4k_modified: modif,
                labels: labels.clone(),
            };
            let cp_path = localization_cache_path(&version)?;
            if let Some(parent) = cp_path.parent() {
                fs::create_dir_all(parent).ok();
            }
            if let Ok(json) = serde_json::to_string(&cached) {
                fs::write(cp_path, json).ok();
            }
            Ok(labels)
        }).await
        .map_err(|e| format!("Task failed: {}", e))?;
    res
}
/// Returns all bindings merged from master defaults and user customizations, with localized labels.
#[tauri::command]
pub async fn get_complete_binding_list(
    gp: String,
    v: String
) -> Result<BindingListResponse, String> {
    let labels = get_localization_labels(gp.clone(), v.clone(), None).await.unwrap_or_default();
    let master = get_master_bindings(gp.clone(), v.clone()).await?;
    let master_profile = &master.profiles[0];
    let user_p = sc_base_dir(&expand_tilde(&gp), &v).join(
        "user/client/0/Profiles/default/actionmaps.xml"
    );
    let user_parsed = if user_p.exists() {
        parse_actionmaps_xml(&fs::read_to_string(user_p).unwrap_or_default()).ok()
    } else {
        None
    };
    let user_profile = user_parsed
        .as_ref()
        .and_then(|up|
            up.profiles.iter().find(|p| p.profile_name == "default" || p.profile_name.is_empty())
        );
    type MergedActions = HashMap<String, (Vec<String>, Option<String>, bool)>;
    let mut merged: HashMap<String, MergedActions> = HashMap::new();
    for am in &master_profile.action_maps {
        let map = merged.entry(am.name.clone()).or_default();
        for a in &am.actions {
            map.entry(a.name.clone()).or_insert_with(|| (vec![], a.label.clone(), false));
        }
        for b in &am.bindings {
            let entry = map.entry(b.action_name.clone()).or_insert_with(|| (vec![], None, false));
            if !entry.0.contains(&b.input) {
                entry.0.push(b.input.clone());
            }
        }
    }
    if let Some(up) = user_profile {
        for am in &up.action_maps {
            let map = merged.entry(am.name.clone()).or_default();
            for b in &am.bindings {
                let entry = map
                    .entry(b.action_name.clone())
                    .or_insert_with(|| (vec![], None, false));
                if !entry.0.contains(&b.input) {
                    entry.0.push(b.input.clone());
                }
                entry.2 = true;
            }
        }
    }
    let mut results = vec![];
    let mut stats = BindingStats { total: 0, custom: 0 };
    for (cat_name, actions) in merged {
        let cat_label = labels
            .get(&format!("ui_Control{}", cat_name))
            .or(labels.get(&cat_name))
            .cloned()
            .unwrap_or_else(|| cat_name.replace('_', " "));
        for (an, (inputs, alabel, is_custom)) in actions {
            stats.total += 1;
            if is_custom {
                stats.custom += 1;
            }
            let dn = alabel
                .as_ref()
                .and_then(|l| labels.get(l.strip_prefix('@').unwrap_or(l)))
                .or(labels.get(&format!("ui_Control{}", an)))
                .or(labels.get(&an))
                .cloned()
                .unwrap_or_else(|| an.replace('_', " "));
            if inputs.is_empty() {
                results.push(CompleteBinding {
                    category: cat_name.clone(),
                    category_label: cat_label.clone(),
                    action_name: an.clone(),
                    display_name: dn.clone(),
                    current_input: "".into(),
                    device_type: "none".into(),
                    description: None,
                    is_custom,
                });
            } else {
                for input in inputs {
                    results.push(CompleteBinding {
                        category: cat_name.clone(),
                        category_label: cat_label.clone(),
                        action_name: an.clone(),
                        display_name: dn.clone(),
                        current_input: input,
                        device_type: "none".into(),
                        description: None,
                        is_custom,
                    });
                }
            }
        }
    }
    results.sort_by(|a, b|
        a.category_label
            .to_lowercase()
            .cmp(&b.category_label.to_lowercase())
            .then(a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()))
    );
    Ok(BindingListResponse { bindings: results, stats })
}

/// Derives a device_map from the <options> tags in a parsed actionmaps.xml.
fn derive_device_map(parsed: &ParsedActionMaps) -> Vec<DeviceMapping> {
    let mut map = vec![];
    for profile in &parsed.profiles {
        for dev in &profile.devices {
            if dev.product.is_empty() { continue; }
            map.push(DeviceMapping {
                product_name: dev.product.clone(),
                device_type: dev.device_type.clone(),
                sc_guid: dev.guid.clone(),
                sc_instance: dev.instance,
                alias: None,
            });
        }
    }
    map
}

/// Reads and saves backup metadata, returning the updated BackupInfo.
fn save_backup_meta(bdir: &Path, info: &BackupInfo) -> Result<(), String> {
    let json = serde_json::to_string_pretty(info).map_err(|e| e.to_string())?;
    fs::write(bdir.join("backup_meta.json"), json).map_err(|e| e.to_string())
}

/// Returns all bindings from a profile's actionmaps.xml merged with master defaults.
#[tauri::command]
pub async fn get_profile_bindings(
    gp: String,
    v: String,
    profile_id: String
) -> Result<BindingListResponse, String> {
    let bdir = backup_version_dir(&v)?.join(&profile_id);
    let actionmaps_path = bdir.join("actionmaps.xml");

    if !actionmaps_path.exists() {
        return Ok(BindingListResponse {
            bindings: vec![],
            stats: BindingStats { total: 0, custom: 0 },
        });
    }

    // Parse profile's actionmaps.xml
    let user_xml = fs::read_to_string(&actionmaps_path).map_err(|e| e.to_string())?;
    let user_parsed = parse_actionmaps_xml(&user_xml)?;

    // Derive and persist device_map if missing from backup_meta
    let meta_path = bdir.join("backup_meta.json");
    if meta_path.exists() {
        let meta_json = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
        let mut meta: BackupInfo = serde_json::from_str(&meta_json).map_err(|e| e.to_string())?;
        if meta.device_map.is_empty() {
            meta.device_map = derive_device_map(&user_parsed);
            save_backup_meta(&bdir, &meta)?;
        }
    }

    // Get master bindings and localization labels
    let labels = get_localization_labels(gp.clone(), v.clone(), None).await.unwrap_or_default();
    let master = get_master_bindings(gp, v).await?;
    let master_profile = &master.profiles[0];

    let user_profile = user_parsed.profiles.iter()
        .find(|p| p.profile_name == "default" || p.profile_name.is_empty());

    // Build merged binding list (same logic as get_complete_binding_list)
    type MergedActions = HashMap<String, (Vec<String>, Option<String>, bool)>;
    let mut merged: HashMap<String, MergedActions> = HashMap::new();

    for am in &master_profile.action_maps {
        let map = merged.entry(am.name.clone()).or_default();
        for a in &am.actions {
            map.entry(a.name.clone()).or_insert_with(|| (vec![], a.label.clone(), false));
        }
        for b in &am.bindings {
            let entry = map.entry(b.action_name.clone()).or_insert_with(|| (vec![], None, false));
            if !entry.0.contains(&b.input) {
                entry.0.push(b.input.clone());
            }
        }
    }

    if let Some(up) = user_profile {
        for am in &up.action_maps {
            let map = merged.entry(am.name.clone()).or_default();
            for b in &am.bindings {
                let entry = map.entry(b.action_name.clone()).or_insert_with(|| (vec![], None, false));
                if !entry.0.contains(&b.input) {
                    entry.0.push(b.input.clone());
                }
                entry.2 = true;
            }
        }
    }

    let mut results = vec![];
    let mut stats = BindingStats { total: 0, custom: 0 };
    for (cat_name, actions) in merged {
        let cat_label = labels
            .get(&format!("ui_Control{}", cat_name))
            .or(labels.get(&cat_name))
            .cloned()
            .unwrap_or_else(|| cat_name.replace('_', " "));
        for (an, (inputs, alabel, is_custom)) in actions {
            stats.total += 1;
            if is_custom { stats.custom += 1; }
            let dn = alabel.as_ref()
                .and_then(|l| labels.get(l.strip_prefix('@').unwrap_or(l)))
                .or(labels.get(&format!("ui_Control{}", an)))
                .or(labels.get(&an))
                .cloned()
                .unwrap_or_else(|| an.replace('_', " "));
            if inputs.is_empty() {
                results.push(CompleteBinding {
                    category: cat_name.clone(),
                    category_label: cat_label.clone(),
                    action_name: an.clone(),
                    display_name: dn.clone(),
                    current_input: "".into(),
                    device_type: "none".into(),
                    description: None,
                    is_custom,
                });
            } else {
                for input in inputs {
                    results.push(CompleteBinding {
                        category: cat_name.clone(),
                        category_label: cat_label.clone(),
                        action_name: an.clone(),
                        display_name: dn.clone(),
                        current_input: input,
                        device_type: "none".into(),
                        description: None,
                        is_custom,
                    });
                }
            }
        }
    }
    results.sort_by(|a, b|
        a.category_label.to_lowercase().cmp(&b.category_label.to_lowercase())
            .then(a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()))
    );
    Ok(BindingListResponse { bindings: results, stats })
}

#[derive(Deserialize)]
pub struct AssignBindingArgs {
    pub game_path: String,
    pub version: String,
    pub action_name: String,
    pub category: String,
    pub input: String,
    pub old_input: Option<String>,
}
/// Assigns an input binding to an action in the user's actionmaps.xml.
#[tauri::command]
pub async fn assign_binding(args: AssignBindingArgs) -> Result<(), String> {
    let p = sc_base_dir(&expand_tilde(&args.game_path), &args.version).join(
        "user/client/0/Profiles/default/actionmaps.xml"
    );
    let mut parsed = if p.exists() {
        parse_actionmaps_xml(&fs::read_to_string(&p).map_err(|e| e.to_string())?)?
    } else {
        ParsedActionMaps {
            version: "1".into(),
            profiles: vec![ScActionProfile {
                profile_name: "default".into(),
                version: "1".into(),
                options_version: "2".into(),
                rebind_version: "2".into(),
                devices: vec![],
                device_options: vec![],
                action_maps: vec![],
            }],
        }
    };
    let profile = parsed.profiles
        .iter_mut()
        .find(|pr| pr.profile_name == "default" || pr.profile_name.is_empty())
        .ok_or("No profile")?;
    let mut found = false;
    for am in &mut profile.action_maps {
        if am.name == args.category {
            if let Some(ref old) = args.old_input {
                if
                    let Some(b) = am.bindings
                        .iter_mut()
                        .find(|b| b.action_name == args.action_name && b.input == *old)
                {
                    b.input = args.input.clone();
                    found = true;
                    break;
                }
            } else if
                let Some(b) = am.bindings.iter_mut().find(|b| b.action_name == args.action_name)
            {
                b.input = args.input.clone();
                found = true;
                break;
            }
        }
    }
    if !found {
        if let Some(am) = profile.action_maps.iter_mut().find(|am| am.name == args.category) {
            am.bindings.push(ScBinding {
                action_name: args.action_name.clone(),
                input: args.input,
            });
        } else {
            profile.action_maps.push(ScActionMap {
                name: args.category.clone(),
                bindings: vec![ScBinding {
                    action_name: args.action_name.clone(),
                    input: args.input,
                }],
                actions: vec![ScAction { name: args.action_name, label: None }],
            });
        }
    }
    write_actionmaps_xml(&p, &parsed)
}
#[derive(Deserialize)]
pub struct RemoveBindingArgs {
    pub game_path: String,
    pub version: String,
    pub action_name: String,
    pub input: String,
    #[allow(dead_code)] pub category: String,
}
/// Removes a specific input binding from an action in the user's actionmaps.xml.
#[tauri::command]
pub async fn remove_binding(args: RemoveBindingArgs) -> Result<(), String> {
    let p = sc_base_dir(&expand_tilde(&args.game_path), &args.version).join(
        "user/client/0/Profiles/default/actionmaps.xml"
    );
    if !p.exists() {
        return Err("Not found".into());
    }
    let mut parsed = parse_actionmaps_xml(&fs::read_to_string(&p).map_err(|e| e.to_string())?)?;
    let profile = parsed.profiles
        .iter_mut()
        .find(|pr| pr.profile_name == "default" || pr.profile_name.is_empty())
        .ok_or("No profile")?;
    for am in &mut profile.action_maps {
        am.bindings.retain(|b| !(b.action_name == args.action_name && b.input == args.input));
    }
    write_actionmaps_xml(&p, &parsed)
}

/// Helper: sets dirty flag and recalculates hash for actionmaps.xml in a backup.
fn mark_backup_dirty(bdir: &Path) -> Result<(), String> {
    let meta_path = bdir.join("backup_meta.json");
    let meta_json = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
    let mut meta: BackupInfo = serde_json::from_str(&meta_json).map_err(|e| e.to_string())?;
    meta.dirty = true;
    let actionmaps_path = bdir.join("actionmaps.xml");
    if let Some(h) = hash_file(&actionmaps_path) {
        meta.file_hashes.insert("actionmaps.xml".into(), h);
    }
    save_backup_meta(bdir, &meta)
}

/// Assigns an input binding to an action in a profile's actionmaps.xml.
#[tauri::command]
pub async fn assign_profile_binding(
    v: String,
    profile_id: String,
    action_map: String,
    action_name: String,
    new_input: String,
    old_input: Option<String>,
) -> Result<(), String> {
    let bdir = backup_version_dir(&v)?.join(&profile_id);
    let actionmaps_path = bdir.join("actionmaps.xml");
    if !actionmaps_path.exists() {
        return Err("Profile has no actionmaps.xml".into());
    }

    let mut parsed = parse_actionmaps_xml(
        &fs::read_to_string(&actionmaps_path).map_err(|e| e.to_string())?
    )?;

    let profile = parsed.profiles.iter_mut()
        .find(|pr| pr.profile_name == "default" || pr.profile_name.is_empty())
        .ok_or("No profile")?;

    let mut found = false;
    for am in &mut profile.action_maps {
        if am.name == action_map {
            if let Some(ref old) = old_input {
                if let Some(b) = am.bindings.iter_mut()
                    .find(|b| b.action_name == action_name && b.input == *old)
                {
                    b.input = new_input.clone();
                    found = true;
                    break;
                }
            } else if let Some(b) = am.bindings.iter_mut()
                .find(|b| b.action_name == action_name)
            {
                b.input = new_input.clone();
                found = true;
                break;
            }
        }
    }

    if !found {
        if let Some(am) = profile.action_maps.iter_mut().find(|am| am.name == action_map) {
            am.bindings.push(ScBinding {
                action_name: action_name.clone(),
                input: new_input,
            });
        } else {
            profile.action_maps.push(ScActionMap {
                name: action_map,
                bindings: vec![ScBinding { action_name: action_name.clone(), input: new_input }],
                actions: vec![ScAction { name: action_name, label: None }],
            });
        }
    }

    write_actionmaps_xml(&actionmaps_path, &parsed)?;
    mark_backup_dirty(&bdir)
}

/// Removes a binding from a profile's actionmaps.xml.
#[tauri::command]
pub async fn remove_profile_binding(
    v: String,
    profile_id: String,
    action_map: String,
    action_name: String,
) -> Result<(), String> {
    let bdir = backup_version_dir(&v)?.join(&profile_id);
    let actionmaps_path = bdir.join("actionmaps.xml");
    if !actionmaps_path.exists() {
        return Err("Profile has no actionmaps.xml".into());
    }

    let mut parsed = parse_actionmaps_xml(
        &fs::read_to_string(&actionmaps_path).map_err(|e| e.to_string())?
    )?;

    let profile = parsed.profiles.iter_mut()
        .find(|pr| pr.profile_name == "default" || pr.profile_name.is_empty())
        .ok_or("No profile")?;

    for am in &mut profile.action_maps {
        if am.name == action_map {
            am.bindings.retain(|b| b.action_name != action_name);
        }
    }

    write_actionmaps_xml(&actionmaps_path, &parsed)?;
    mark_backup_dirty(&bdir)
}

/// Applies a profile's files to the live SC directory and clears dirty flag.
#[tauri::command]
pub async fn apply_profile_to_sc(
    gp: String,
    v: String,
    profile_id: String,
) -> Result<(), String> {
    // Reuse existing restore_profile logic
    restore_profile(gp, v.clone(), profile_id.clone()).await?;

    // Clear dirty flag
    let bdir = backup_version_dir(&v)?.join(&profile_id);
    let meta_path = bdir.join("backup_meta.json");
    if meta_path.exists() {
        let meta_json = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
        let mut meta: BackupInfo = serde_json::from_str(&meta_json).map_err(|e| e.to_string())?;
        meta.dirty = false;
        save_backup_meta(&bdir, &meta)?;
    }

    // Update active_profiles.json
    save_active_profile(v, profile_id).await
}

/// Sets a user-defined alias for a device in a profile's device_map.
#[tauri::command]
pub async fn set_profile_device_alias(
    v: String,
    profile_id: String,
    product_name: String,
    alias: String,
) -> Result<(), String> {
    let bdir = backup_version_dir(&v)?.join(&profile_id);
    let meta_path = bdir.join("backup_meta.json");
    if !meta_path.exists() {
        return Err("Profile metadata not found".into());
    }

    let meta_json = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
    let mut meta: BackupInfo = serde_json::from_str(&meta_json).map_err(|e| e.to_string())?;

    let alias_val = if alias.trim().is_empty() { None } else { Some(alias) };
    if let Some(dm) = meta.device_map.iter_mut().find(|dm| dm.product_name == product_name) {
        dm.alias = alias_val;
    } else {
        return Err(format!("Device '{}' not found in profile device map", product_name));
    }

    save_backup_meta(&bdir, &meta)
}

/// Migrates old binding_database.json by renaming it to .bak.
/// Returns true if migration was performed, false if no old file found.
#[tauri::command]
pub async fn migrate_binding_database() -> Result<bool, String> {
    let config_dir = dirs::config_dir().ok_or("No config dir")?;
    let db_path = config_dir.join("star-control/bindings/binding_database.json");
    if db_path.exists() {
        let bak_path = config_dir.join("star-control/bindings/binding_database.json.bak");
        fs::rename(&db_path, &bak_path).map_err(|e| e.to_string())?;
        log::info!("Migrated binding_database.json → .bak");
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Reads a text file from the Data.p4k archive and returns its contents as a string.
#[tauri::command]
pub async fn read_p4k(
    game_path: String,
    version: String,
    file_path: String
) -> Result<String, String> {
    Ok(String::from_utf8_lossy(&read_p4k_file(&game_path, &version, &file_path)?).into_owned())
}
/// Lists files in the P4K archive, optionally filtered by a pattern.
#[tauri::command]
pub async fn list_p4k(
    game_path: String,
    version: String,
    pattern: Option<String>
) -> Result<Vec<String>, String> {
    let p = sc_base_dir(&game_path, &version).join("Data.p4k");
    if !p.exists() {
        return Err("No P4K".into());
    }

    let mut file = File::open(&p).map_err(|e| e.to_string())?;
    let file_length = file
        .metadata()
        .map_err(|e| e.to_string())?
        .len();
    let (cd_offset, cd_size) = find_central_directory(&mut file, file_length)?;

    file
        .seek(SeekFrom::Start(cd_offset))
        .map_err(|e| format!("Failed to seek to central directory: {}", e))?;
    let mut central_dir = vec![0u8; cd_size as usize];
    file
        .read_exact(&mut central_dir)
        .map_err(|e| format!("Failed to read central directory: {}", e))?;

    let filter = pattern.unwrap_or_default().to_lowercase();
    let mut results = vec![];
    let mut pos = 0;

    while pos + 46 <= central_dir.len() {
        if &central_dir[pos..pos + 4] != b"PK\x01\x02" {
            pos += 1;
            continue;
        }
        let name_length = u16::from_le_bytes([
            central_dir[pos + 28],
            central_dir[pos + 29],
        ]) as usize;
        if pos + 46 + name_length > central_dir.len() {
            break;
        }
        let name = String::from_utf8_lossy(&central_dir[pos + 46..pos + 46 + name_length]);
        if name.to_lowercase().contains(&filter) {
            results.push(name.to_string());
        }
        let extra_length = u16::from_le_bytes([
            central_dir[pos + 30],
            central_dir[pos + 31],
        ]) as usize;
        let comment_length = u16::from_le_bytes([
            central_dir[pos + 32],
            central_dir[pos + 33],
        ]) as usize;
        pos += 46 + name_length + extra_length + comment_length;
    }

    Ok(results)
}
/// Reads the global.ini localization file for the specified language from the P4K archive.
#[tauri::command]
pub async fn get_localization_ini(
    gp: String,
    v: String,
    lang: Option<String>
) -> Result<String, String> {
    let b = read_p4k_file(
        &gp,
        &v,
        &format!("Data/Localization/{}/global.ini", lang.unwrap_or("english".into()))
    )?;
    Ok(String::from_utf8_lossy(&b).into_owned())
}
/// Lists available localization languages by scanning the P4K archive.
#[tauri::command]
pub async fn list_localization_languages(gp: String, v: String) -> Result<Vec<String>, String> {
    let p4k = sc_base_dir(&gp, &v).join("Data.p4k");
    if !p4k.exists() {
        return Err("No P4K".into());
    }

    let mut file = File::open(&p4k).map_err(|e| e.to_string())?;
    let file_length = file
        .metadata()
        .map_err(|e| e.to_string())?
        .len();
    let (cd_offset, cd_size) = find_central_directory(&mut file, file_length)?;

    file
        .seek(SeekFrom::Start(cd_offset))
        .map_err(|e| format!("Failed to seek to central directory: {}", e))?;
    let mut central_dir = vec![0u8; cd_size as usize];
    file
        .read_exact(&mut central_dir)
        .map_err(|e| format!("Failed to read central directory: {}", e))?;

    let mut localization_files = vec![];
    let mut pos = 0;
    while pos + 46 <= central_dir.len() {
        if &central_dir[pos..pos + 4] != b"PK\x01\x02" {
            pos += 1;
            continue;
        }
        let name_length = u16::from_le_bytes([
            central_dir[pos + 28],
            central_dir[pos + 29],
        ]) as usize;
        if pos + 46 + name_length > central_dir.len() {
            break;
        }
        let name = String::from_utf8_lossy(&central_dir[pos + 46..pos + 46 + name_length]);
        if name.to_lowercase().contains("localization/") {
            localization_files.push(name.to_string());
        }
        let extra_length = u16::from_le_bytes([
            central_dir[pos + 30],
            central_dir[pos + 31],
        ]) as usize;
        let comment_length = u16::from_le_bytes([
            central_dir[pos + 32],
            central_dir[pos + 33],
        ]) as usize;
        pos += 46 + name_length + extra_length + comment_length;
    }

    let mut languages = HashSet::new();
    for entry in localization_files {
        if
            let Some(rest) = entry
                .strip_prefix("Data\\Localization\\")
                .or(entry.strip_prefix("Localization\\"))
        {
            if let Some(sep) = rest.find('\\') {
                let lang = rest[..sep].to_string();
                if !lang.is_empty() {
                    languages.insert(lang);
                }
            }
        }
    }

    let mut result: Vec<String> = languages.into_iter().collect();
    result.sort();
    Ok(result)
}
/// Reorders device instance numbers in actionmaps.xml to match the desired order.
#[tauri::command]
pub async fn reorder_devices(
    gp: String,
    v: String,
    new_order: Vec<DeviceReorderEntry>
) -> Result<(), String> {
    let p = sc_base_dir(&expand_tilde(&gp), &v).join(
        "user/client/0/Profiles/default/actionmaps.xml"
    );
    if !p.exists() {
        return Err("Not found".into());
    }
    let mut c = fs::read_to_string(&p).map_err(|e| e.to_string())?;
    for e in &new_order {
        let o = format!("instance=\"{}\"", e.old_instance);
        let pl = format!("instance=\"__REMAP_{}__\"", e.new_instance);
        c = c.replace(&o, &pl);
    }
    for e in &new_order {
        let pl = format!("instance=\"__REMAP_{}__\"", e.new_instance);
        let f = format!("instance=\"{}\"", e.new_instance);
        c = c.replace(&pl, &f);
    }
    fs::write(p, c).map_err(|e| e.to_string())
}
fn hash_file(path: &Path) -> Option<String> {
    let data = fs::read(path).ok()?;
    let hash = Sha256::digest(&data);
    Some(format!("{:x}", hash))
}

/// Creates a manual backup of the default profile with a user-provided label.
#[tauri::command]
pub async fn backup_profile_manual(gp: String, v: String, l: String) -> Result<BackupInfo, String> {
    backup_profile(gp, v, Some("manual".into()), Some(l)).await
}
/// Creates a timestamped backup of the default profile with optional type and label.
#[tauri::command]
pub async fn backup_profile(
    gp: String,
    v: String,
    bt: Option<String>,
    l: Option<String>
) -> Result<BackupInfo, String> {
    let now = Local::now();
    let id = now.format("%Y-%m-%dT%H-%M-%S").to_string();
    let bdir = backup_version_dir(&v)?;
    let target = bdir.join(&id);
    fs::create_dir_all(&target).ok();

    let expanded = expand_tilde(&gp);
    let user_base = sc_base_dir(&expanded, &v).join("user/client/0");
    let pdir = user_base.join("Profiles/default");
    let mut fs_list = vec![];
    let mut hashes = HashMap::new();
    for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
        let src = pdir.join(f);
        if src.exists() {
            if let Some(h) = hash_file(&src) { hashes.insert(f.to_string(), h); }
            fs::copy(&src, target.join(f)).ok();
            fs_list.push(f.to_string());
        }
    }

    // Backup Controls/Mappings
    if let Some(controls_dir) = find_dir_case_insensitive(&user_base, &["Controls/Mappings", "controls/mappings", "controls/Mappings"]) {
        let target_controls = target.join("controls_mappings");
        fs::create_dir_all(&target_controls).ok();
        if let Ok(entries) = fs::read_dir(&controls_dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("xml")) {
                    if let Some(name) = path.file_name() {
                        let key = format!("controls_mappings/{}", name.to_string_lossy());
                        if let Some(h) = hash_file(&path) { hashes.insert(key.clone(), h); }
                        fs::copy(&path, target_controls.join(name)).ok();
                        fs_list.push(key);
                    }
                }
            }
        }
    }

    // Backup CustomCharacters
    if let Some(chars_dir) = find_dir_case_insensitive(&user_base, &["CustomCharacters", "customcharacters"]) {
        let target_chars = target.join("custom_characters");
        fs::create_dir_all(&target_chars).ok();
        if let Ok(entries) = fs::read_dir(&chars_dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("chf")) {
                    if let Some(name) = path.file_name() {
                        let key = format!("custom_characters/{}", name.to_string_lossy());
                        if let Some(h) = hash_file(&path) { hashes.insert(key.clone(), h); }
                        fs::copy(&path, target_chars.join(name)).ok();
                        fs_list.push(key);
                    }
                }
            }
        }
    }

    // Derive device_map from the backed-up actionmaps.xml
    let device_map = if target.join("actionmaps.xml").exists() {
        if let Ok(xml) = fs::read_to_string(target.join("actionmaps.xml")) {
            if let Ok(parsed) = parse_actionmaps_xml(&xml) {
                derive_device_map(&parsed)
            } else { vec![] }
        } else { vec![] }
    } else { vec![] };

    let info = BackupInfo {
        id,
        created_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
        timestamp: now.timestamp() as u64,
        version: v,
        backup_type: bt.unwrap_or("manual".into()),
        files: fs_list,
        label: l.unwrap_or_default(),
        file_hashes: hashes,
        device_map,
        dirty: false,
    };

    if let Ok(json) = serde_json::to_string_pretty(&info) {
        fs::write(target.join("backup_meta.json"), json).ok();
    }

    Ok(info)
}
/// Restores a profile from a previously created backup.
#[tauri::command]
pub async fn restore_profile(gp: String, v: String, bid: String) -> Result<(), String> {
    let bdir = backup_version_dir(&v)?.join(bid);
    let expanded = expand_tilde(&gp);
    let user_base = sc_base_dir(&expanded, &v).join("user/client/0");
    let pdir = user_base.join("Profiles/default");
    fs::create_dir_all(&pdir).ok();
    for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
        if bdir.join(f).exists() {
            fs::copy(bdir.join(f), pdir.join(f)).ok();
        }
    }

    // Restore Controls/Mappings
    if bdir.join("controls_mappings").is_dir() {
        let target_controls = find_dir_case_insensitive(&user_base, &["Controls/Mappings", "controls/mappings", "controls/Mappings"])
            .unwrap_or_else(|| user_base.join("Controls/Mappings"));
        fs::create_dir_all(&target_controls).ok();
        if let Ok(entries) = fs::read_dir(bdir.join("controls_mappings")) {
            for e in entries.flatten() {
                let path = e.path();
                if let Some(name) = path.file_name() {
                    fs::copy(&path, target_controls.join(name)).ok();
                }
            }
        }
    }

    // Restore CustomCharacters
    if bdir.join("custom_characters").is_dir() {
        let target_chars = find_dir_case_insensitive(&user_base, &["CustomCharacters", "customcharacters"])
            .unwrap_or_else(|| user_base.join("CustomCharacters"));
        fs::create_dir_all(&target_chars).ok();
        if let Ok(entries) = fs::read_dir(bdir.join("custom_characters")) {
            for e in entries.flatten() {
                let path = e.path();
                if let Some(name) = path.file_name() {
                    fs::copy(&path, target_chars.join(name)).ok();
                }
            }
        }
    }

    Ok(())
}

/// Imports settings from another version as a new saved profile in the target version.
/// If `bid` is provided, copies from that saved profile; otherwise copies from the source version's live SC files.
/// Does NOT overwrite any SC files — only creates a new profile that the user can then load.
#[tauri::command]
pub async fn import_version_as_profile(
    gp: String,
    source_version: String,
    target_version: String,
    bid: Option<String>,
    label: Option<String>,
) -> Result<BackupInfo, String> {
    let now = Local::now();
    let id = now.format("%Y-%m-%dT%H-%M-%S").to_string();
    let bdir = backup_version_dir(&target_version)?;
    let target = bdir.join(&id);
    fs::create_dir_all(&target).ok();

    let auto_label = label.unwrap_or_else(|| format!("Imported from {}", source_version));
    let mut fs_list = vec![];
    let mut hashes = HashMap::new();

    if let Some(ref backup_id) = bid {
        // Copy from a saved profile in the source version
        let src_backup = backup_version_dir(&source_version)?.join(backup_id);
        if !src_backup.is_dir() {
            return Err(format!("Backup {} not found in version {}", backup_id, source_version));
        }
        for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
            let src = src_backup.join(f);
            if src.exists() {
                if let Some(h) = hash_file(&src) { hashes.insert(f.to_string(), h); }
                fs::copy(&src, target.join(f)).ok();
                fs_list.push(f.to_string());
            }
        }
        if src_backup.join("controls_mappings").is_dir() {
            let tc = target.join("controls_mappings");
            fs::create_dir_all(&tc).ok();
            if let Ok(entries) = fs::read_dir(src_backup.join("controls_mappings")) {
                for e in entries.flatten() {
                    let path = e.path();
                    if let Some(name) = path.file_name() {
                        let key = format!("controls_mappings/{}", name.to_string_lossy());
                        if let Some(h) = hash_file(&path) { hashes.insert(key.clone(), h); }
                        fs::copy(&path, tc.join(name)).ok();
                        fs_list.push(key);
                    }
                }
            }
        }
        if src_backup.join("custom_characters").is_dir() {
            let tc = target.join("custom_characters");
            fs::create_dir_all(&tc).ok();
            if let Ok(entries) = fs::read_dir(src_backup.join("custom_characters")) {
                for e in entries.flatten() {
                    let path = e.path();
                    if let Some(name) = path.file_name() {
                        let key = format!("custom_characters/{}", name.to_string_lossy());
                        if let Some(h) = hash_file(&path) { hashes.insert(key.clone(), h); }
                        fs::copy(&path, tc.join(name)).ok();
                        fs_list.push(key);
                    }
                }
            }
        }
    } else {
        // Copy from the source version's live SC files
        let expanded = expand_tilde(&gp);
        let source_base = sc_base_dir(&expanded, &source_version).join("user/client/0");
        let pdir = source_base.join("Profiles/default");
        for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
            let src = pdir.join(f);
            if src.exists() {
                if let Some(h) = hash_file(&src) { hashes.insert(f.to_string(), h); }
                fs::copy(&src, target.join(f)).ok();
                fs_list.push(f.to_string());
            }
        }
        if let Some(controls_dir) = find_dir_case_insensitive(&source_base, &["Controls/Mappings", "controls/mappings", "controls/Mappings"]) {
            let tc = target.join("controls_mappings");
            fs::create_dir_all(&tc).ok();
            if let Ok(entries) = fs::read_dir(&controls_dir) {
                for e in entries.flatten() {
                    let path = e.path();
                    if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("xml")) {
                        if let Some(name) = path.file_name() {
                            let key = format!("controls_mappings/{}", name.to_string_lossy());
                            if let Some(h) = hash_file(&path) { hashes.insert(key.clone(), h); }
                            fs::copy(&path, tc.join(name)).ok();
                            fs_list.push(key);
                        }
                    }
                }
            }
        }
        if let Some(chars_dir) = find_dir_case_insensitive(&source_base, &["CustomCharacters", "customcharacters"]) {
            let tc = target.join("custom_characters");
            fs::create_dir_all(&tc).ok();
            if let Ok(entries) = fs::read_dir(&chars_dir) {
                for e in entries.flatten() {
                    let path = e.path();
                    if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("chf")) {
                        if let Some(name) = path.file_name() {
                            let key = format!("custom_characters/{}", name.to_string_lossy());
                            if let Some(h) = hash_file(&path) { hashes.insert(key.clone(), h); }
                            fs::copy(&path, tc.join(name)).ok();
                            fs_list.push(key);
                        }
                    }
                }
            }
        }
    }

    if fs_list.is_empty() {
        // Clean up empty backup dir
        fs::remove_dir_all(&target).ok();
        return Err("No files found to import.".into());
    }

    // Derive device_map from actionmaps.xml if present
    let device_map = if target.join("actionmaps.xml").exists() {
        if let Ok(xml) = fs::read_to_string(target.join("actionmaps.xml")) {
            if let Ok(parsed) = parse_actionmaps_xml(&xml) {
                derive_device_map(&parsed)
            } else { vec![] }
        } else { vec![] }
    } else { vec![] };

    let info = BackupInfo {
        id,
        created_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
        timestamp: now.timestamp() as u64,
        version: target_version,
        backup_type: "imported".into(),
        files: fs_list,
        label: auto_label,
        file_hashes: hashes,
        device_map,
        dirty: false,
    };

    if let Ok(json) = serde_json::to_string_pretty(&info) {
        fs::write(target.join("backup_meta.json"), json).ok();
    }

    Ok(info)
}

/// Lists all profile backups for a given version, sorted newest first.
#[tauri::command]
pub async fn list_backups(v: String) -> Result<Vec<BackupInfo>, String> {
    let d = backup_version_dir(&v)?;
    if !d.is_dir() {
        return Ok(vec![]);
    }
    let mut res = vec![];
    if let Ok(es) = fs::read_dir(d) {
        for e in es.flatten() {
            if let Ok(c) = fs::read_to_string(e.path().join("backup_meta.json")) {
                if let Ok(i) = serde_json::from_str::<BackupInfo>(&c) {
                    res.push(i);
                }
            }
        }
    }
    res.sort_by_key(|b| std::cmp::Reverse(b.timestamp));
    Ok(res)
}
/// Deletes a profile backup by its ID.
#[tauri::command]
pub async fn delete_backup(v: String, bid: String) -> Result<(), String> {
    let p = backup_version_dir(&v)?.join(bid);
    if p.is_dir() {
        fs::remove_dir_all(p).ok();
    }
    Ok(())
}
/// Compares current SC files against a backup's stored hashes.
#[derive(Serialize, Deserialize, Clone)]
pub struct FileStatus {
    pub file: String,
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProfileStatus {
    pub matched: bool,
    pub files: Vec<FileStatus>,
}

/// Collects all current SC profile-related files with their hashes.
fn collect_current_sc_hashes(user_base: &Path) -> HashMap<String, String> {
    let mut current = HashMap::new();
    let pdir = user_base.join("Profiles/default");
    for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
        let src = pdir.join(f);
        if src.exists() {
            if let Some(h) = hash_file(&src) { current.insert(f.to_string(), h); }
        }
    }
    if let Some(controls_dir) = find_dir_case_insensitive(user_base, &["Controls/Mappings", "controls/mappings", "controls/Mappings"]) {
        if let Ok(entries) = fs::read_dir(&controls_dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("xml")) {
                    if let Some(name) = path.file_name() {
                        let key = format!("controls_mappings/{}", name.to_string_lossy());
                        if let Some(h) = hash_file(&path) { current.insert(key, h); }
                    }
                }
            }
        }
    }
    if let Some(chars_dir) = find_dir_case_insensitive(user_base, &["CustomCharacters", "customcharacters"]) {
        if let Ok(entries) = fs::read_dir(&chars_dir) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("chf")) {
                    if let Some(name) = path.file_name() {
                        let key = format!("custom_characters/{}", name.to_string_lossy());
                        if let Some(h) = hash_file(&path) { current.insert(key, h); }
                    }
                }
            }
        }
    }
    current
}

#[tauri::command]
pub async fn check_profile_status(gp: String, v: String, bid: String) -> Result<ProfileStatus, String> {
    let bdir = backup_version_dir(&v)?.join(&bid);
    let meta_path = bdir.join("backup_meta.json");
    let meta: BackupInfo = serde_json::from_str(
        &fs::read_to_string(&meta_path).map_err(|e| e.to_string())?
    ).map_err(|e| e.to_string())?;

    if meta.file_hashes.is_empty() {
        return Ok(ProfileStatus { matched: false, files: vec![] });
    }

    let expanded = expand_tilde(&gp);
    let user_base = sc_base_dir(&expanded, &v).join("user/client/0");
    let current = collect_current_sc_hashes(&user_base);

    let mut files = vec![];
    let mut all_match = true;

    // Check files from backup
    for (file, saved_hash) in &meta.file_hashes {
        let status = match current.get(file) {
            Some(cur_hash) if cur_hash == saved_hash => "unchanged",
            Some(_) => { all_match = false; "modified" },
            None => { all_match = false; "deleted" },
        };
        files.push(FileStatus { file: file.clone(), status: status.into() });
    }

    // Check for new files not in backup
    for file in current.keys() {
        if !meta.file_hashes.contains_key(file) {
            all_match = false;
            files.push(FileStatus { file: file.clone(), status: "new".into() });
        }
    }

    files.sort_by(|a, b| a.file.cmp(&b.file));

    Ok(ProfileStatus { matched: all_match, files })
}

fn active_profiles_path() -> Result<std::path::PathBuf, String> {
    Ok(dirs::config_dir().ok_or("No config dir")?.join("star-control/active_profiles.json"))
}

#[tauri::command]
pub async fn load_active_profiles() -> Result<HashMap<String, String>, String> {
    let path = active_profiles_path()?;
    if !path.exists() { return Ok(HashMap::new()); }
    let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_active_profile(v: String, bid: String) -> Result<(), String> {
    let path = active_profiles_path()?;
    let mut map: HashMap<String, String> = if path.exists() {
        serde_json::from_str(&fs::read_to_string(&path).unwrap_or_default()).unwrap_or_default()
    } else {
        HashMap::new()
    };
    if bid.is_empty() {
        map.remove(&v);
    } else {
        map.insert(v, bid);
    }
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).ok(); }
    fs::write(path, serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Updates the label of an existing backup.
#[tauri::command]
pub async fn update_backup_label(v: String, bid: String, l: String) -> Result<(), String> {
    let p = backup_version_dir(&v)?.join(bid).join("backup_meta.json");
    let mut i: BackupInfo = serde_json
        ::from_str(&fs::read_to_string(&p).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    i.label = l;
    fs::write(p, serde_json::to_string_pretty(&i).map_err(|e| e.to_string())?).map_err(|e|
        e.to_string()
    )
}

/// Lists other SC version folders that have importable profile/control/character data.
#[tauri::command]
pub async fn list_importable_versions(gp: String, target_version: String) -> Result<Vec<VersionImportInfo>, String> {
    let base = Path::new(&expand_tilde(&gp)).join("drive_c/Program Files/Roberts Space Industries/StarCitizen");
    let mut result = vec![];
    let entries = fs::read_dir(&base).map_err(|e| e.to_string())?;
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_dir() { continue; }
        let version = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if version == target_version { continue; }

        let user_base = path.join("user/client/0");
        let profiles_dir = user_base.join("Profiles/default");

        let profile_file_count = ["actionmaps.xml", "attributes.xml", "profile.xml"]
            .iter()
            .filter(|f| profiles_dir.join(f).exists())
            .count() as u32;

        let (controls_dir, controls_file_count) = match find_dir_case_insensitive(&user_base, &["Controls/Mappings", "controls/mappings", "controls/Mappings"]) {
            Some(d) => {
                let count = fs::read_dir(&d).map_or(0, |es| es.flatten().filter(|e| e.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))).count()) as u32;
                (true, count)
            }
            None => (false, 0)
        };

        let (chars_dir, character_file_count) = match find_dir_case_insensitive(&user_base, &["CustomCharacters", "customcharacters"]) {
            Some(d) => {
                let count = fs::read_dir(&d).map_or(0, |es| es.flatten().filter(|e| e.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("chf"))).count()) as u32;
                (true, count)
            }
            None => (false, 0)
        };

        let score = profile_file_count * 3 + controls_file_count * 2 + character_file_count;
        if score == 0 { continue; }

        result.push(VersionImportInfo {
            version,
            has_profiles: profile_file_count > 0,
            has_controls_mappings: controls_dir,
            has_custom_characters: chars_dir,
            profile_file_count,
            controls_file_count,
            character_file_count,
            score,
        });
    }
    result.sort_by(|a, b| b.score.cmp(&a.score));
    Ok(result)
}

/// Imports profile, controls, and character data from one SC version to another.
#[tauri::command]
pub async fn import_from_version(gp: String, source_version: String, target_version: String) -> Result<ImportResult, String> {
    let expanded = expand_tilde(&gp);
    let source_base = sc_base_dir(&expanded, &source_version).join("user/client/0");
    let target_base = sc_base_dir(&expanded, &target_version).join("user/client/0");

    // Save existing settings before they get overwritten by import
    let target_profiles = target_base.join("Profiles/default");
    if target_profiles.join("actionmaps.xml").exists() {
        let _ = backup_profile(gp.clone(), target_version.clone(), Some("pre-import".into()), Some(format!("Before import from {}", source_version))).await;
    }

    // Copy profiles
    let mut profiles_copied = 0u32;
    let source_profiles = source_base.join("Profiles/default");
    if source_profiles.is_dir() {
        fs::create_dir_all(&target_profiles).map_err(|e| e.to_string())?;
        for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
            let src = source_profiles.join(f);
            if src.exists() {
                fs::copy(&src, target_profiles.join(f)).map_err(|e| e.to_string())?;
                profiles_copied += 1;
            }
        }
    }

    // Copy Controls/Mappings
    let mut controls_copied = 0u32;
    if let Some(source_controls) = find_dir_case_insensitive(&source_base, &["Controls/Mappings", "controls/mappings", "controls/Mappings"]) {
        let target_controls = find_dir_case_insensitive(&target_base, &["Controls/Mappings", "controls/mappings", "controls/Mappings"])
            .unwrap_or_else(|| target_base.join("Controls/Mappings"));
        fs::create_dir_all(&target_controls).map_err(|e| e.to_string())?;
        if let Ok(entries) = fs::read_dir(&source_controls) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("xml")) {
                    if let Some(name) = path.file_name() {
                        fs::copy(&path, target_controls.join(name)).map_err(|e| e.to_string())?;
                        controls_copied += 1;
                    }
                }
            }
        }
    }

    // Copy CustomCharacters
    let mut characters_copied = 0u32;
    if let Some(source_chars) = find_dir_case_insensitive(&source_base, &["CustomCharacters", "customcharacters"]) {
        let target_chars = find_dir_case_insensitive(&target_base, &["CustomCharacters", "customcharacters"])
            .unwrap_or_else(|| target_base.join("CustomCharacters"));
        fs::create_dir_all(&target_chars).map_err(|e| e.to_string())?;
        if let Ok(entries) = fs::read_dir(&source_chars) {
            for e in entries.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("chf")) {
                    if let Some(name) = path.file_name() {
                        fs::copy(&path, target_chars.join(name)).map_err(|e| e.to_string())?;
                        characters_copied += 1;
                    }
                }
            }
        }
    }

    Ok(ImportResult { profiles_copied, controls_copied, characters_copied })
}

/// Lists exported control layout XML files from the controls/mappings directory.
#[tauri::command]
pub async fn list_exported_layouts(
    game_path: String,
    version: String
) -> Result<Vec<ExportedLayout>, String> {
    let d = sc_base_dir(&expand_tilde(&game_path), &version).join(
        "user/client/0/controls/mappings"
    );
    if !d.is_dir() {
        return Ok(vec![]);
    }
    let mut res = vec![];
    if let Ok(es) = fs::read_dir(d) {
        for e in es.flatten() {
            let path = e.path();
            if path.extension().is_some_and(|ext| ext == "xml") {
                let f = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                let m = path
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                res.push(ExportedLayout {
                    label: f.trim_end_matches(".xml").replace('_', " "),
                    filename: f,
                    modified: m,
                });
            }
        }
    }
    res.sort_by_key(|l| std::cmp::Reverse(l.modified));
    Ok(res)
}

/// Copies Data.p4k from source version to target version with progress reporting
#[tauri::command]
pub async fn copy_data_p4k(
    gp: String,
    source_version: String,
    target_version: String,
    window: tauri::Window,
) -> Result<(), String> {
    let exp = expand_tilde(&gp);

    // Try multiple possible paths
    let base_paths: Vec<PathBuf> = vec![
        Path::new(&exp).join("drive_c/Program Files/Roberts Space Industries/StarCitizen"),
        Path::new(&exp).join("StarCitizen"),
        Path::new(&exp).to_path_buf()
    ];

    let mut base = None;
    for p in &base_paths {
        if p.exists() && p.is_dir() {
            base = Some(p.clone());
            break;
        }
    }

    let base = base.ok_or_else(|| "StarCitizen directory not found".to_string())?;

    let source = base.join(&source_version).join("Data.p4k");
    let target = base.join(&target_version).join("Data.p4k");

    if !source.exists() {
        return Err(format!("Source Data.p4k not found at {}", source.display()));
    }

    if target.exists() {
        return Err("Target already has Data.p4k".to_string());
    }

    // Get file size for progress calculation
    let metadata = fs::metadata(&source).map_err(|e| e.to_string())?;
    let total_size = metadata.len();

    // Create parent dir if needed
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // Copy with progress using tokio (runs in background thread)
    let source_clone = source.clone();
    let target_clone = target.clone();
    let target_for_emit = target.clone();
    let window_clone = window.clone();

    tokio::task::spawn_blocking(move || {
        copy_with_progress(&source_clone, &target_clone, total_size, move |copied, total| {
            let percent = (copied as f64 / total as f64 * 100.0) as u32;
            let _ = window_clone.emit("data-p4k-progress", serde_json::json!({
                "version": target_for_emit.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
                "percent": percent,
                "copied": copied,
                "total": total
            }));
        })
    })
    .await
    .map_err(|e| format!("Task error: {}", e))??;

    log::info!("Copied Data.p4k from {} to {}", source_version, target_version);

    // Emit completion event
    let _ = window.emit("data-p4k-copy-complete", serde_json::json!({
        "version": target_version,
        "success": true
    }));

    Ok(())
}

/// Copy file with progress tracking (synchronous for use in spawn_blocking)
fn copy_with_progress<F>(from: &Path, to: &Path, total_size: u64, mut progress_callback: F) -> Result<u64, String>
where
    F: FnMut(u64, u64) + Send,
{
    use std::io::{BufReader, BufWriter, Read, Write};

    let input = BufReader::new(
        fs::File::open(from).map_err(|e| e.to_string())?
    );
    let mut input: Box<dyn Read> = Box::new(input);

    let output = BufWriter::new(
        fs::File::create(to).map_err(|e| e.to_string())?
    );
    let mut output: Box<dyn Write> = Box::new(output);

    let mut written: u64 = 0;
    let mut buffer = [0u8; 1024 * 1024]; // 1MB buffer

    loop {
        let bytes_read = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if bytes_read == 0 {
            break;
        }

        output.write_all(&buffer[..bytes_read]).map_err(|e| e.to_string())?;
        written += bytes_read as u64;

        // Report progress every 10MB, but at least once for small files
        if written == 0 || written % (10 * 1024 * 1024) == 0 {
            progress_callback(written, total_size);
        }
    }

    output.flush().map_err(|e| e.to_string())?;
    progress_callback(written, total_size);

    Ok(written)
}

/// Aborts a running copy by removing partial file
#[tauri::command]
pub async fn abort_copy_data_p4k(gp: String, version: String) -> Result<(), String> {
    let exp = expand_tilde(&gp);

    let base_paths: Vec<PathBuf> = vec![
        Path::new(&exp).join("drive_c/Program Files/Roberts Space Industries/StarCitizen"),
        Path::new(&exp).join("StarCitizen"),
        Path::new(&exp).to_path_buf()
    ];

    let mut base = None;
    for p in &base_paths {
        if p.exists() && p.is_dir() {
            base = Some(p.clone());
            break;
        }
    }

    let base = base.ok_or_else(|| "StarCitizen directory not found".to_string())?;
    let target = base.join(&version).join("Data.p4k");

    if target.exists() {
        fs::remove_file(&target).map_err(|e| e.to_string())?;
        log::info!("Aborted copy - removed partial Data.p4k for {}", version);
    }

    Ok(())
}
