use chrono::Local;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

// ============================================================
// Data Structures
// ============================================================

#[derive(Serialize, Deserialize, Clone)]
pub struct ScAttribute {
    pub name: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ScAttributes {
    #[serde(rename = "Attr", default)]
    pub attrs: Vec<ScAttribute>,
    #[serde(default)]
    pub version: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ScVersionInfo {
    pub version: String,
    pub path: String,
    pub has_usercfg: bool,
    pub has_attributes: bool,
    pub has_actionmaps: bool,
    pub has_exported_layouts: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ScDevice {
    pub device_type: String,
    pub instance: u32,
    pub product: String,
    pub guid: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ScDeviceOption {
    pub input: String,
    pub deadzone: Option<f64>,
    pub saturation: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ScDeviceOptions {
    pub name: String,
    pub options: Vec<ScDeviceOption>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ScBinding {
    pub action_name: String,
    pub input: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ScActionMap {
    pub name: String,
    pub bindings: Vec<ScBinding>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ParsedActionMaps {
    pub version: String,
    pub profile_name: String,
    pub devices: Vec<ScDevice>,
    pub device_options: Vec<ScDeviceOptions>,
    pub action_maps: Vec<ScActionMap>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct BackupInfo {
    pub id: String,
    pub created_at: String,
    pub timestamp: u64,
    pub version: String,
    pub backup_type: String,
    pub files: Vec<String>,
    pub label: String,
}

impl Default for BackupInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            created_at: String::new(),
            timestamp: 0,
            version: String::new(),
            backup_type: String::new(),
            files: Vec::new(),
            label: String::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ExportedLayout {
    pub filename: String,
    pub label: String,
    pub modified: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DeviceReorderEntry {
    pub old_instance: u32,
    pub new_instance: u32,
}

// ============================================================
// Path Helpers
// ============================================================

pub(crate) fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen('~', &home, 1);
        }
    }
    path.to_string()
}

pub(crate) fn sc_base_dir(game_path: &str, version: &str) -> PathBuf {
    Path::new(game_path)
        .join("drive_c")
        .join("Program Files")
        .join("Roberts Space Industries")
        .join("StarCitizen")
        .join(version)
}

fn sc_user_dir(game_path: &str, version: &str) -> PathBuf {
    sc_base_dir(game_path, version)
        .join("user")
        .join("client")
        .join("0")
}

fn sc_profile_dir(game_path: &str, version: &str) -> PathBuf {
    sc_user_dir(game_path, version)
        .join("Profiles")
        .join("default")
}

fn sc_mappings_dir(game_path: &str, version: &str) -> PathBuf {
    sc_user_dir(game_path, version)
        .join("controls")
        .join("mappings")
}

fn backup_base_dir() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|p| p.join("star-control").join("backups"))
        .ok_or_else(|| "Could not determine config directory".to_string())
}

fn backup_version_dir(version: &str) -> Result<PathBuf, String> {
    Ok(backup_base_dir()?.join(version))
}

// ============================================================
// Version Detection
// ============================================================

#[tauri::command]
pub async fn detect_sc_versions(game_path: String) -> Result<Vec<ScVersionInfo>, String> {
    let expanded = expand_tilde(&game_path);
    let sc_base = Path::new(&expanded)
        .join("drive_c")
        .join("Program Files")
        .join("Roberts Space Industries")
        .join("StarCitizen");

    let mut versions: Vec<ScVersionInfo> = Vec::new();

    if let Ok(entries) = fs::read_dir(&sc_base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();

                let user_cfg_path = path.join("USER.cfg");
                let profile_dir = sc_profile_dir(&expanded, &name);
                let actionmaps_path = profile_dir.join("actionmaps.xml");
                let attributes_path = profile_dir.join("attributes.xml");
                let mappings_dir = sc_mappings_dir(&expanded, &name);

                let has_exported_layouts = mappings_dir.is_dir()
                    && fs::read_dir(&mappings_dir)
                        .map(|entries| {
                            entries.flatten().any(|e| {
                                e.path()
                                    .extension()
                                    .map_or(false, |ext| ext == "xml")
                            })
                        })
                        .unwrap_or(false);

                versions.push(ScVersionInfo {
                    version: name,
                    path: path.to_string_lossy().into_owned(),
                    has_usercfg: user_cfg_path.exists(),
                    has_attributes: attributes_path.exists(),
                    has_actionmaps: actionmaps_path.exists(),
                    has_exported_layouts,
                });
            }
        }
    }

    versions.sort_by(|a, b| {
        fn priority(name: &str) -> u8 {
            match name {
                "LIVE" => 0,
                "PTU" => 1,
                "HOTFIX" => 2,
                _ => 3,
            }
        }
        let pa = priority(&a.version);
        let pb = priority(&b.version);
        pa.cmp(&pb).then_with(|| a.version.cmp(&b.version))
    });

    Ok(versions)
}

// ============================================================
// USER.cfg Commands (unchanged paths — already correct)
// ============================================================

#[tauri::command]
pub async fn read_user_cfg(game_path: String, version: String) -> Result<String, String> {
    let expanded = expand_tilde(&game_path);
    let user_cfg_path = sc_base_dir(&expanded, &version).join("USER.cfg");

    if !user_cfg_path.exists() {
        return Ok(String::new());
    }

    fs::read_to_string(&user_cfg_path)
        .map_err(|e| format!("Failed to read USER.cfg: {}", e))
}

#[tauri::command]
pub async fn write_user_cfg(
    game_path: String,
    version: String,
    content: String,
) -> Result<(), String> {
    let expanded = expand_tilde(&game_path);
    let user_cfg_path = sc_base_dir(&expanded, &version).join("USER.cfg");

    if let Some(parent) = user_cfg_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    fs::write(&user_cfg_path, content)
        .map_err(|e| format!("Failed to write USER.cfg: {}", e))
}

// ============================================================
// Profile Listing (refactored paths)
// ============================================================

#[derive(Serialize, Deserialize, Clone)]
pub struct ScProfile {
    pub name: String,
    #[serde(rename = "lastPlayed")]
    pub last_played: u64,
}

#[tauri::command]
pub async fn list_profiles(game_path: String, version: String) -> Result<Vec<ScProfile>, String> {
    let expanded = expand_tilde(&game_path);
    let profiles_path = sc_user_dir(&expanded, &version).join("Profiles");

    if !profiles_path.is_dir() {
        return Ok(Vec::new());
    }

    let mut profiles = Vec::new();

    if let Ok(entries) = fs::read_dir(&profiles_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();

                if name == "frontend" {
                    continue;
                }

                let mut last_played: u64 = 0;
                let attrs_path = path.join("attributes.xml");
                if attrs_path.exists() {
                    if let Ok(content) = fs::read_to_string(&attrs_path) {
                        if let Some(start) = content.find("lastPlayed=\"") {
                            let rest = &content[start + 12..];
                            if let Some(end) = rest.find('"') {
                                if let Ok(ts) = rest[..end].parse::<u64>() {
                                    last_played = ts;
                                }
                            }
                        }
                    }
                }

                profiles.push(ScProfile { name, last_played });
            }
        }
    }

    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(profiles)
}

// ============================================================
// Attributes (refactored paths)
// ============================================================

fn parse_attributes_xml(content: &str) -> ScAttributes {
    let mut attrs = ScAttributes::default();

    // Try "Version=" (actual SC format) and "version=" (fallback)
    let version_marker = content
        .find("Version=\"")
        .map(|pos| (pos, 9))
        .or_else(|| content.find("version=\"").map(|pos| (pos, 9)));
    if let Some((start, prefix_len)) = version_marker {
        let rest = &content[start + prefix_len..];
        if let Some(end) = rest.find('"') {
            attrs.version = rest[..end].to_string();
        }
    }

    let mut search_pos = 0;
    while let Some(start) = content[search_pos..].find("<Attr ") {
        let start = search_pos + start;
        if let Some(name_start) = content[start..].find("name=\"") {
            let name_start = start + name_start + 6;
            if let Some(name_end) = content[name_start..].find('"') {
                let name = content[name_start..name_start + name_end].to_string();
                if let Some(value_start) = content[name_start + name_end..].find("value=\"") {
                    let value_start = name_start + name_end + value_start + 7;
                    if let Some(value_end) = content[value_start..].find('"') {
                        let value = content[value_start..value_start + value_end].to_string();
                        attrs.attrs.push(ScAttribute { name, value });
                    }
                }
            }
        }
        search_pos = start + 1;
    }

    attrs
}

fn serialize_attributes_xml(attrs: &ScAttributes) -> String {
    let version_attr = if !attrs.version.is_empty() {
        format!(" Version=\"{}\"", attrs.version)
    } else {
        String::new()
    };

    let mut xml = format!("<Attributes{}>\n", version_attr);

    for attr in &attrs.attrs {
        let name = attr
            .name
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;");
        let value = attr
            .value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;");
        xml.push_str(&format!(
            " <Attr name=\"{}\" value=\"{}\"/>\n",
            name, value
        ));
    }

    xml.push_str("</Attributes>\n");
    xml
}

#[tauri::command]
pub async fn read_attributes(
    game_path: String,
    version: String,
) -> Result<ScAttributes, String> {
    let expanded = expand_tilde(&game_path);
    let attrs_path = sc_profile_dir(&expanded, &version).join("attributes.xml");

    if !attrs_path.exists() {
        return Ok(ScAttributes::default());
    }

    let content = fs::read_to_string(&attrs_path)
        .map_err(|e| format!("Failed to read attributes.xml: {}", e))?;

    Ok(parse_attributes_xml(&content))
}

#[tauri::command]
pub async fn write_attributes(
    game_path: String,
    version: String,
    attrs: ScAttributes,
) -> Result<(), String> {
    let expanded = expand_tilde(&game_path);
    let attrs_path = sc_profile_dir(&expanded, &version).join("attributes.xml");

    if let Some(parent) = attrs_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let content = serialize_attributes_xml(&attrs);
    fs::write(&attrs_path, content)
        .map_err(|e| format!("Failed to write attributes.xml: {}", e))
}

// ============================================================
// Export / Import Profile (refactored paths)
// ============================================================

#[tauri::command]
pub async fn export_profile(
    game_path: String,
    version: String,
    dest_path: String,
) -> Result<(), String> {
    let expanded = expand_tilde(&game_path);
    let source_path = sc_profile_dir(&expanded, &version);

    if !source_path.exists() {
        return Err("Profile directory does not exist".to_string());
    }

    let dest = Path::new(&dest_path);
    fs::create_dir_all(dest)
        .map_err(|e| format!("Failed to create destination directory: {}", e))?;

    for filename in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
        let src = source_path.join(filename);
        if src.exists() {
            fs::copy(&src, dest.join(filename))
                .map_err(|e| format!("Failed to copy {}: {}", filename, e))?;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn import_profile(
    game_path: String,
    version: String,
    source_path: String,
) -> Result<(), String> {
    let expanded = expand_tilde(&game_path);
    let dest_path = sc_profile_dir(&expanded, &version);
    let source = Path::new(&source_path);

    if !source.exists() {
        return Err(format!("Source path '{}' does not exist", source_path));
    }

    fs::create_dir_all(&dest_path)
        .map_err(|e| format!("Failed to create profile directory: {}", e))?;

    for filename in &["actionmaps.xml", "attributes.xml", "profile.xml"] {
        let src = source.join(filename);
        if src.exists() {
            fs::copy(&src, dest_path.join(filename))
                .map_err(|e| format!("Failed to copy {}: {}", filename, e))?;
        }
    }

    Ok(())
}

// ============================================================
// Parse actionmaps.xml
// ============================================================

fn parse_product_string(product_str: &str) -> (String, String) {
    // Product string format: "VKBsim Gladiator NXT R  {0200231D-0000-0000-0000-504944564944}"
    // or sometimes: "Name {GUID}"
    if let Some(guid_start) = product_str.rfind('{') {
        let name = product_str[..guid_start].trim().to_string();
        let guid = product_str[guid_start..].trim().to_string();
        (name, guid)
    } else {
        (product_str.trim().to_string(), String::new())
    }
}

fn get_attr_value(e: &quick_xml::events::BytesStart, attr_name: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == attr_name {
            return String::from_utf8(attr.value.to_vec()).ok();
        }
    }
    None
}

fn handle_start_or_empty_tag(
    e: &quick_xml::events::BytesStart,
    result: &mut ParsedActionMaps,
    current_actionmap: &mut Option<ScActionMap>,
    current_action_name: &mut Option<String>,
    current_device_options: &mut Option<ScDeviceOptions>,
) {
    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

    match tag_name.as_str() {
        // Handle both <ActionMaps> and <ActionProfiles> as metadata sources
        "ActionMaps" | "ActionProfiles" => {
            if let Some(v) = get_attr_value(e, b"version") {
                result.version = v;
            }
            // Try camelCase (actual SC format) and snake_case
            if let Some(v) = get_attr_value(e, b"profileName") {
                result.profile_name = v;
            } else if let Some(v) = get_attr_value(e, b"profile_name") {
                result.profile_name = v;
            }
        }
        "options" => {
            let device_type = get_attr_value(e, b"type").unwrap_or_default();
            let instance = get_attr_value(e, b"instance")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0);
            let product_raw = get_attr_value(e, b"Product").unwrap_or_default();

            // Only include devices that have a Product string (skip empty placeholder slots)
            if !product_raw.is_empty() {
                let (product, guid) = parse_product_string(&product_raw);
                result.devices.push(ScDevice {
                    device_type,
                    instance,
                    product,
                    guid,
                });
            }
        }
        "deviceoptions" => {
            let name = get_attr_value(e, b"name").unwrap_or_default();
            *current_device_options = Some(ScDeviceOptions {
                name,
                options: Vec::new(),
            });
        }
        "option" => {
            if let Some(ref mut dev_opts) = current_device_options {
                let input = get_attr_value(e, b"input").unwrap_or_default();
                let deadzone = get_attr_value(e, b"deadzone")
                    .and_then(|v| v.parse::<f64>().ok());
                let saturation = get_attr_value(e, b"saturation")
                    .and_then(|v| v.parse::<f64>().ok());
                dev_opts.options.push(ScDeviceOption {
                    input,
                    deadzone,
                    saturation,
                });
            }
        }
        "actionmap" => {
            let name = get_attr_value(e, b"name").unwrap_or_default();
            *current_actionmap = Some(ScActionMap {
                name,
                bindings: Vec::new(),
            });
        }
        "action" => {
            *current_action_name = get_attr_value(e, b"name");
        }
        "rebind" => {
            if let (Some(ref action_name), Some(ref mut am)) =
                (current_action_name, current_actionmap)
            {
                let input = get_attr_value(e, b"input").unwrap_or_default();
                if !input.is_empty() {
                    am.bindings.push(ScBinding {
                        action_name: action_name.clone(),
                        input,
                    });
                }
            }
        }
        _ => {}
    }
}

fn parse_actionmaps_xml(content: &str) -> Result<ParsedActionMaps, String> {
    let mut reader = Reader::from_str(content);

    let mut result = ParsedActionMaps {
        version: String::new(),
        profile_name: String::new(),
        devices: Vec::new(),
        device_options: Vec::new(),
        action_maps: Vec::new(),
    };

    let mut current_actionmap: Option<ScActionMap> = None;
    let mut current_action_name: Option<String> = None;
    let mut current_device_options: Option<ScDeviceOptions> = None;

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Empty(ref e)) => {
                handle_start_or_empty_tag(
                    e,
                    &mut result,
                    &mut current_actionmap,
                    &mut current_action_name,
                    &mut current_device_options,
                );
            }
            Ok(Event::Start(ref e)) => {
                handle_start_or_empty_tag(
                    e,
                    &mut result,
                    &mut current_actionmap,
                    &mut current_action_name,
                    &mut current_device_options,
                );
            }
            Ok(Event::End(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                match tag_name.as_str() {
                    "actionmap" => {
                        if let Some(am) = current_actionmap.take() {
                            if !am.bindings.is_empty() {
                                result.action_maps.push(am);
                            }
                        }
                    }
                    "action" => {
                        current_action_name = None;
                    }
                    "deviceoptions" => {
                        if let Some(opts) = current_device_options.take() {
                            result.device_options.push(opts);
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => {
                return Err(format!("XML parse error: {}", e));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(result)
}

#[tauri::command]
pub async fn parse_actionmaps(
    game_path: String,
    version: String,
    source: Option<String>,
) -> Result<ParsedActionMaps, String> {
    let expanded = expand_tilde(&game_path);

    let xml_path = match source {
        Some(ref filename) => sc_mappings_dir(&expanded, &version).join(filename),
        None => sc_profile_dir(&expanded, &version).join("actionmaps.xml"),
    };

    if !xml_path.exists() {
        return Err(format!(
            "File not found: {}",
            xml_path.to_string_lossy()
        ));
    }

    let content = fs::read_to_string(&xml_path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    parse_actionmaps_xml(&content)
}

// ============================================================
// Reorder Devices
// ============================================================

/// Find the full extent of an `<options ... >...</options>` or `<options ... />`
/// block for a joystick with the given instance number.
fn find_options_block(content: &str, instance: u32) -> Option<(usize, usize)> {
    let instance_str = format!("instance=\"{}\"", instance);
    let mut search_from = 0;

    loop {
        let rel = content[search_from..].find("<options ")?;
        let tag_start = search_from + rel;
        let rest = &content[tag_start..];

        // Find the end of the opening tag: either "/>" (self-closing) or ">"
        let self_close = rest.find("/>");
        let open_close = rest.find('>');

        let (is_self_closing, tag_end_rel) = match (self_close, open_close) {
            (Some(sc), Some(oc)) if sc < oc => (true, sc),
            (Some(sc), Some(oc)) if sc == oc + 1 => (true, oc), // "/>": > is at oc, / is at oc-1 — actually "/>" means sc = position of '/'
            (_, Some(oc)) => {
                // Check if the char before '>' is '/'
                if oc > 0 && rest.as_bytes()[oc - 1] == b'/' {
                    (true, oc - 1)
                } else {
                    (false, oc)
                }
            }
            _ => {
                search_from = tag_start + 1;
                continue;
            }
        };

        let tag_header = &rest[..tag_end_rel];

        if tag_header.contains("type=\"joystick\"") && tag_header.contains(&instance_str) {
            if is_self_closing {
                // Self-closing: ends at "/>" + 2 chars
                let block_end = tag_start
                    + rest.find("/>").map(|i| i + 2).unwrap_or(tag_end_rel + 2);
                return Some((tag_start, block_end));
            } else {
                // Has children: find </options>
                let close_tag = rest.find("</options>")?;
                let block_end = tag_start + close_tag + "</options>".len();
                return Some((tag_start, block_end));
            }
        }

        search_from = tag_start + 1;
    }
}

#[tauri::command]
pub async fn reorder_devices(
    game_path: String,
    version: String,
    new_order: Vec<DeviceReorderEntry>,
) -> Result<(), String> {
    let expanded = expand_tilde(&game_path);
    let actionmaps_path = sc_profile_dir(&expanded, &version).join("actionmaps.xml");

    if !actionmaps_path.exists() {
        return Err("actionmaps.xml not found".to_string());
    }

    // TODO: Auto-backup before modifying (disabled for now)
    // backup_profile_internal(&expanded, &version, "auto-pre-reorder", "")?;

    let content = fs::read_to_string(&actionmaps_path)
        .map_err(|e| format!("Failed to read actionmaps.xml: {}", e))?;

    let mut result = content.clone();

    // Step 1: Physically swap the <options> blocks for each pair
    // (new_order always contains pairs, e.g. [{1→2}, {2→1}])
    // We only need to process one direction of the swap
    let mut swapped: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for entry in &new_order {
        let pair = if entry.old_instance < entry.new_instance {
            (entry.old_instance, entry.new_instance)
        } else {
            (entry.new_instance, entry.old_instance)
        };
        if swapped.contains(&pair) {
            continue;
        }
        swapped.insert(pair);

        let block_a = find_options_block(&result, entry.old_instance)
            .ok_or_else(|| {
                format!(
                    "Could not find <options> block for joystick instance {}",
                    entry.old_instance
                )
            })?;
        let block_b = find_options_block(&result, entry.new_instance)
            .ok_or_else(|| {
                format!(
                    "Could not find <options> block for joystick instance {}",
                    entry.new_instance
                )
            })?;

        let text_a = result[block_a.0..block_a.1].to_string();
        let text_b = result[block_b.0..block_b.1].to_string();

        // Swap: replace the later block first to preserve earlier offsets
        let (first, second, first_text, second_text) = if block_a.0 < block_b.0 {
            (block_a, block_b, text_a, text_b)
        } else {
            (block_b, block_a, text_b, text_a)
        };

        let mut swapped_content = String::with_capacity(result.len());
        swapped_content.push_str(&result[..first.0]);
        swapped_content.push_str(&second_text);
        swapped_content.push_str(&result[first.1..second.0]);
        swapped_content.push_str(&first_text);
        swapped_content.push_str(&result[second.1..]);
        result = swapped_content;
    }

    // Step 2: Swap instance="X" attributes — ONLY for joystick <options> tags
    // Must scope to type="joystick" to avoid corrupting keyboard/gamepad/mouse instances
    for entry in &new_order {
        let old_pattern = format!("type=\"joystick\" instance=\"{}\"", entry.old_instance);
        let placeholder = format!(
            "type=\"joystick\" instance=\"__REMAP_{}__\"",
            entry.new_instance
        );
        result = result.replace(&old_pattern, &placeholder);
    }
    for entry in &new_order {
        let placeholder = format!(
            "type=\"joystick\" instance=\"__REMAP_{}__\"",
            entry.new_instance
        );
        let final_val = format!("type=\"joystick\" instance=\"{}\"", entry.new_instance);
        result = result.replace(&placeholder, &final_val);
    }

    // Step 2b: Swap <deviceoptions name="joystickN"> references
    for entry in &new_order {
        let old_name = format!("name=\"joystick{}\"", entry.old_instance);
        let placeholder = format!("name=\"__JOYSTICK_REMAP_{}__\"", entry.new_instance);
        result = result.replace(&old_name, &placeholder);
    }
    for entry in &new_order {
        let placeholder = format!("name=\"__JOYSTICK_REMAP_{}__\"", entry.new_instance);
        let final_name = format!("name=\"joystick{}\"", entry.new_instance);
        result = result.replace(&placeholder, &final_name);
    }

    // Step 3: Swap jsX_ binding prefixes
    for entry in &new_order {
        let old_prefix = format!("js{}_", entry.old_instance);
        let placeholder = format!("__JS_REMAP_{}_", entry.new_instance);
        result = result.replace(&old_prefix, &placeholder);
    }
    for entry in &new_order {
        let placeholder = format!("__JS_REMAP_{}_", entry.new_instance);
        let final_prefix = format!("js{}_", entry.new_instance);
        result = result.replace(&placeholder, &final_prefix);
    }

    // Write to temp file, then atomic rename
    let tmp_path = actionmaps_path.with_extension("xml.tmp");
    fs::write(&tmp_path, &result)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;

    fs::rename(&tmp_path, &actionmaps_path)
        .map_err(|e| format!("Failed to rename temp file: {}", e))?;

    Ok(())
}

// ============================================================
// Backup / Restore
// ============================================================

fn backup_profile_internal(
    game_path: &str,
    version: &str,
    backup_type: &str,
    label: &str,
) -> Result<BackupInfo, String> {
    let now = Local::now();
    let id = now.format("%Y-%m-%dT%H-%M-%S").to_string();
    let created_at = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let timestamp = now.timestamp() as u64;

    let backup_dir = backup_version_dir(version)?.join(&id);
    fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

    let profile_dir = sc_profile_dir(game_path, version);
    let base_dir = sc_base_dir(game_path, version);

    let files_to_backup = [
        (profile_dir.join("actionmaps.xml"), "actionmaps.xml"),
        (profile_dir.join("attributes.xml"), "attributes.xml"),
        (profile_dir.join("profile.xml"), "profile.xml"),
        (base_dir.join("USER.cfg"), "USER.cfg"),
    ];

    let mut backed_up_files = Vec::new();

    for (src, name) in &files_to_backup {
        if src.exists() {
            fs::copy(src, backup_dir.join(name))
                .map_err(|e| format!("Failed to copy {}: {}", name, e))?;
            backed_up_files.push(name.to_string());
        }
    }

    if backed_up_files.is_empty() {
        // Clean up empty backup dir
        let _ = fs::remove_dir(&backup_dir);
        return Err("No profile files found to backup".to_string());
    }

    let info = BackupInfo {
        id,
        created_at,
        timestamp,
        version: version.to_string(),
        backup_type: backup_type.to_string(),
        files: backed_up_files,
        label: label.to_string(),
    };

    // Write metadata
    let meta_json = serde_json::to_string_pretty(&info)
        .map_err(|e| format!("Failed to serialize backup metadata: {}", e))?;
    fs::write(backup_dir.join("backup_meta.json"), meta_json)
        .map_err(|e| format!("Failed to write backup metadata: {}", e))?;

    Ok(info)
}

#[tauri::command]
pub async fn backup_profile(
    game_path: String,
    version: String,
    backup_type: Option<String>,
    label: Option<String>,
) -> Result<BackupInfo, String> {
    let expanded = expand_tilde(&game_path);
    let bt = backup_type.unwrap_or_else(|| "manual".to_string());
    let lbl = label.unwrap_or_default();
    backup_profile_internal(&expanded, &version, &bt, &lbl)
}

#[tauri::command]
pub async fn restore_profile(
    game_path: String,
    version: String,
    backup_id: String,
) -> Result<(), String> {
    let expanded = expand_tilde(&game_path);

    // TODO: Safety backup before restoring (disabled for now)
    // let _ = backup_profile_internal(&expanded, &version, "auto-pre-restore", "");

    let backup_dir = backup_version_dir(&version)?.join(&backup_id);
    if !backup_dir.is_dir() {
        return Err(format!("Backup '{}' not found", backup_id));
    }

    let profile_dir = sc_profile_dir(&expanded, &version);
    let base_dir = sc_base_dir(&expanded, &version);

    fs::create_dir_all(&profile_dir)
        .map_err(|e| format!("Failed to create profile directory: {}", e))?;

    let files_to_restore = [
        ("actionmaps.xml", profile_dir.join("actionmaps.xml")),
        ("attributes.xml", profile_dir.join("attributes.xml")),
        ("profile.xml", profile_dir.join("profile.xml")),
        ("USER.cfg", base_dir.join("USER.cfg")),
    ];

    for (name, dest) in &files_to_restore {
        let src = backup_dir.join(name);
        if src.exists() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create directory: {}", e))?;
            }
            fs::copy(&src, dest)
                .map_err(|e| format!("Failed to restore {}: {}", name, e))?;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn list_backups(version: String) -> Result<Vec<BackupInfo>, String> {
    let backup_dir = backup_version_dir(&version)?;

    if !backup_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut backups = Vec::new();

    if let Ok(entries) = fs::read_dir(&backup_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let meta_path = path.join("backup_meta.json");
                if meta_path.exists() {
                    if let Ok(content) = fs::read_to_string(&meta_path) {
                        if let Ok(info) = serde_json::from_str::<BackupInfo>(&content) {
                            backups.push(info);
                        }
                    }
                }
            }
        }
    }

    // Sort by timestamp descending (newest first)
    backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(backups)
}

#[tauri::command]
pub async fn update_backup_label(
    version: String,
    backup_id: String,
    label: String,
) -> Result<(), String> {
    let backup_dir = backup_version_dir(&version)?.join(&backup_id);
    let meta_path = backup_dir.join("backup_meta.json");

    if !meta_path.exists() {
        return Err(format!("Backup '{}' not found", backup_id));
    }

    let content = fs::read_to_string(&meta_path)
        .map_err(|e| format!("Failed to read backup metadata: {}", e))?;

    let mut info: BackupInfo = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse backup metadata: {}", e))?;

    info.label = label;

    let meta_json = serde_json::to_string_pretty(&info)
        .map_err(|e| format!("Failed to serialize backup metadata: {}", e))?;

    fs::write(&meta_path, meta_json)
        .map_err(|e| format!("Failed to write backup metadata: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn delete_backup(version: String, backup_id: String) -> Result<(), String> {
    let backup_dir = backup_version_dir(&version)?.join(&backup_id);

    if !backup_dir.is_dir() {
        return Err(format!("Backup '{}' not found", backup_id));
    }

    fs::remove_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to delete backup: {}", e))
}

// ============================================================
// List Exported Layouts
// ============================================================

#[tauri::command]
pub async fn list_exported_layouts(
    game_path: String,
    version: String,
) -> Result<Vec<ExportedLayout>, String> {
    let expanded = expand_tilde(&game_path);
    let mappings_dir = sc_mappings_dir(&expanded, &version);

    if !mappings_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut layouts = Vec::new();

    if let Ok(entries) = fs::read_dir(&mappings_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "xml") {
                let filename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();

                // Create a friendly label from the filename
                let label = filename
                    .trim_end_matches(".xml")
                    .replace('_', " ")
                    .to_string();

                let modified = path
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                layouts.push(ExportedLayout {
                    filename,
                    label,
                    modified,
                });
            }
        }
    }

    layouts.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(layouts)
}
