use std::collections::{HashMap, HashSet};
use std::time::UNIX_EPOCH;
use std::fs::{self, File};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use chrono::Local;
use serde::{Deserialize, Serialize};
use quick_xml::Reader;
use quick_xml::events::{Event, BytesStart, BytesEnd, BytesDecl};
use tokio::sync::Mutex;
use once_cell::sync::Lazy;
use quick_xml::writer::Writer;
use encoding_rs::UTF_16LE;
use crate::action_definitions::{CompleteBinding, BindingStats, BindingListResponse, ActionDefinitions};

#[derive(Serialize, Deserialize, Clone, Debug, Default)] pub struct ScDeviceOptions { pub name: String, pub options: Vec<ScDeviceOption> }
#[derive(Serialize, Deserialize, Clone, Debug)] pub struct ScDeviceOption { pub input: String, pub deadzone: Option<f64>, pub saturation: Option<f64> }
#[derive(Serialize, Deserialize, Clone, Debug)] pub struct ScDevice { pub device_type: String, pub instance: u32, pub product: String, pub guid: Option<String> }
#[derive(Serialize, Deserialize, Clone, Debug)] pub struct ScBinding { pub action_name: String, pub input: String }
#[derive(Serialize, Deserialize, Clone, Debug)] pub struct ScAction { pub name: String, pub label: Option<String> }
#[derive(Serialize, Deserialize, Clone, Debug)] pub struct ScActionMap { pub name: String, pub bindings: Vec<ScBinding>, pub actions: Vec<ScAction> }
#[derive(Serialize, Deserialize, Clone, Debug)] pub struct ScActionProfile { pub profile_name: String, pub version: String, pub options_version: String, pub rebind_version: String, pub devices: Vec<ScDevice>, pub device_options: Vec<ScDeviceOptions>, pub action_maps: Vec<ScActionMap> }
#[derive(Serialize, Deserialize, Clone, Debug, Default)] pub struct ParsedActionMaps { pub version: String, pub profiles: Vec<ScActionProfile> }
#[derive(Serialize, Deserialize, Clone)] pub struct ScVersionInfo { pub version: String, pub path: String, pub has_usercfg: bool, pub has_attributes: bool, pub has_actionmaps: bool, pub has_exported_layouts: bool }
#[derive(Serialize, Deserialize, Default, Clone)] pub struct ScAttribute { pub name: String, pub value: String }
#[derive(Serialize, Deserialize, Default, Clone)] pub struct ScAttributes { pub version: String, pub attrs: Vec<ScAttribute> }
#[derive(Serialize, Deserialize, Clone)] pub struct BackupInfo { pub id: String, pub created_at: String, pub timestamp: u64, pub version: String, pub backup_type: String, pub files: Vec<String>, pub label: String }
#[derive(Serialize, Deserialize, Clone)] pub struct ExportedLayout { pub filename: String, pub label: String, pub modified: u64 }
#[derive(Serialize, Deserialize, Clone)] pub struct DeviceReorderEntry { #[serde(rename = "deviceType")] pub device_type: String, #[serde(rename = "oldInstance")] pub old_instance: u32, #[serde(rename = "newInstance")] pub new_instance: u32 }
#[derive(Serialize, Deserialize, Clone)] pub struct ScProfile { pub name: String, pub last_played: u64 }
#[derive(Serialize, Deserialize)] struct CachedLocalization { p4k_size: u64, p4k_modified: u64, labels: HashMap<String, String> }
#[derive(Serialize, Deserialize)] struct CachedMasterBindings { p4k_size: u64, p4k_modified: u64, data: ParsedActionMaps }

static LOCALIZATION_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static MASTER_BINDINGS_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub(crate) fn expand_tilde(p: &str) -> String { if p.starts_with('~') { if let Some(h) = dirs::home_dir() { return p.replacen('~', &h.to_string_lossy(), 1); } } p.to_string() }
pub fn sc_base_dir(gp: &str, v: &str) -> PathBuf { Path::new(gp).join("drive_c/Program Files/Roberts Space Industries/StarCitizen").join(v) }
fn sc_p4k_path(gp: &str, v: &str) -> Result<PathBuf, String> { let p = sc_base_dir(gp, v).join("Data.p4k"); if p.exists() { Ok(p) } else { Err("No P4K".into()) } }
fn master_bindings_cache_path(v: &str) -> Result<PathBuf, String> { Ok(dirs::config_dir().ok_or("No config dir")?.join("star-control/cache").join(format!("master_bindings_{}.json", v))) }
fn localization_cache_path(v: &str) -> Result<PathBuf, String> { Ok(dirs::config_dir().ok_or("No config dir")?.join("star-control/cache").join(format!("localization_{}.json", v))) }
fn backup_version_dir(v: &str) -> Result<PathBuf, String> { Ok(dirs::config_dir().ok_or("No config dir")?.join("star-control/backups").join(v)) }

fn get_attr(e: &BytesStart, n: &[u8]) -> Option<String> { for a in e.attributes().flatten() { if a.key.as_ref() == n { return Some(String::from_utf8_lossy(&a.value).into_owned()); } } None }
fn parse_global_ini(c: &str) -> HashMap<String, String> { let mut m = HashMap::new(); for l in c.lines() { let t = l.trim(); if t.is_empty() || t.starts_with(';') { continue; } if let Some(p) = t.find('=') { let mut k = t[..p].trim().to_string(); if k.starts_with('@') { k = k[1..].to_string(); } m.insert(k, t[p+1..].trim().to_string()); } } m }

fn parse_actionmaps_xml(c: &str) -> Result<ParsedActionMaps, String> {
    let mut r = Reader::from_str(c.trim().trim_matches('\0')); r.config_mut().trim_text(true);
    let mut res = ParsedActionMaps::default(); let (mut cp, mut cm, mut ca) = (None, None, None); let mut buf = Vec::new();
    loop { match r.read_event_into(&mut buf) { Ok(Event::Eof) => break, Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => { let tag = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
            match tag.as_str() { "actionmaps" => { res.version = get_attr(e, b"version").unwrap_or("1".into()); }
                "actionprofiles" => { cp = Some(ScActionProfile { profile_name: get_attr(e, b"profileName").or(get_attr(e, b"profile_name")).unwrap_or_default(), version: get_attr(e, b"version").unwrap_or("1".into()), options_version: get_attr(e, b"optionsVersion").unwrap_or("2".into()), rebind_version: get_attr(e, b"rebindVersion").unwrap_or("2".into()), devices: vec![], device_options: vec![], action_maps: vec![] }); }
                "options" => { if let Some(ref mut p) = cp { let mut product = get_attr(e, b"Product").unwrap_or_default(); let guid = if product.contains('{') { product.split('{').nth(1).and_then(|s| s.strip_suffix('}')).map(|s| s.to_string()) } else { None }; // Remove GUID from product name: "Device {GUID}" -> "Device"
                if let Some(ref g) = guid { let pattern = format!("{{{}}}", g); if let Some(start) = product.find(&pattern) { let end_pos = start + pattern.len(); if end_pos <= product.len() { product = format!("{}{}", &product[..start].trim_end(), &product[end_pos..]); } else { product = product[..start].trim_end().to_string(); } } } p.devices.push(ScDevice { device_type: get_attr(e, b"type").unwrap_or_default(), instance: get_attr(e, b"instance").and_then(|v| v.parse().ok()).unwrap_or(0), product: product.trim().to_string(), guid }); } }
                "actionmap" => { cm = Some(ScActionMap { name: get_attr(e, b"name").unwrap_or_default(), bindings: vec![], actions: vec![] }); }
                "action" => { let n = get_attr(e, b"name").unwrap_or_default(); ca = Some(n.clone()); if let Some(ref mut m) = cm { m.actions.push(ScAction { name: n, label: get_attr(e, b"label") }); } }
                "rebind" => { if let (Some(ref mut m), Some(ref a)) = (cm.as_mut(), ca.as_ref()) { m.bindings.push(ScBinding { action_name: a.to_string(), input: get_attr(e, b"input").unwrap_or_default() }); } }
                _ => {} } } Ok(Event::End(ref e)) => { let tag = String::from_utf8_lossy(e.name().as_ref()).to_lowercase();
            match tag.as_str() { "actionmap" => { if let (Some(m), Some(ref mut p)) = (cm.take(), cp.as_mut()) { p.action_maps.push(m); } }
                "actionprofiles" => { if let Some(p) = cp.take() { res.profiles.push(p); } } "action" => { ca = None; } _ => {} } } _ => {} } buf.clear(); } Ok(res)
}

fn write_actionmaps_xml(p: &Path, parsed: &ParsedActionMaps) -> Result<(), String> {
    let mut w = Writer::new_with_indent(Cursor::new(vec![]), b' ', 1); w.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None))).ok();
    let mut root = BytesStart::new("ActionMaps"); root.push_attribute(("version", parsed.version.as_str())); w.write_event(Event::Start(root)).ok();
    for po in &parsed.profiles { let mut p_tag = BytesStart::new("ActionProfiles"); p_tag.push_attribute(("version", po.version.as_str())); p_tag.push_attribute(("optionsVersion", po.options_version.as_str())); p_tag.push_attribute(("rebindVersion", po.rebind_version.as_str())); p_tag.push_attribute(("profileName", po.profile_name.as_str())); w.write_event(Event::Start(p_tag)).ok();
        for d in &po.devices { let mut d_tag = BytesStart::new("options"); d_tag.push_attribute(("type", d.device_type.as_str())); d_tag.push_attribute(("instance", d.instance.to_string().as_str())); if !d.product.is_empty() { d_tag.push_attribute(("Product", d.product.as_str())); } w.write_event(Event::Empty(d_tag)).ok(); }
        w.write_event(Event::Empty(BytesStart::new("modifiers"))).ok();
        for am in &po.action_maps { let mut am_tag = BytesStart::new("actionmap"); am_tag.push_attribute(("name", am.name.as_str())); w.write_event(Event::Start(am_tag)).ok();
            for b in &am.bindings { let mut a_tag = BytesStart::new("action"); a_tag.push_attribute(("name", b.action_name.as_str())); w.write_event(Event::Start(a_tag)).ok();
                let mut r_tag = BytesStart::new("rebind"); r_tag.push_attribute(("input", b.input.as_str())); w.write_event(Event::Empty(r_tag)).ok(); w.write_event(Event::End(BytesEnd::new("action"))).ok(); }
            w.write_event(Event::End(BytesEnd::new("actionmap"))).ok(); } w.write_event(Event::End(BytesEnd::new("ActionProfiles"))).ok(); }
    w.write_event(Event::End(BytesEnd::new("ActionMaps"))).ok(); fs::write(p, String::from_utf8(w.into_inner().into_inner()).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn find_central_directory(f: &mut File, l: u64) -> Result<(u64, u64), String> {
    let s = 65536.min(l); f.seek(SeekFrom::End(-(s as i64))).unwrap(); let mut b = vec![0u8; s as usize]; f.read_exact(&mut b).unwrap();
    for i in (0..s as usize - 4).rev() { if &b[i..i+4] == b"PK\x06\x07" { f.seek(SeekFrom::Start(l - s + i as u64 + 8)).unwrap(); let mut off = [0u8; 8]; f.read_exact(&mut off).unwrap(); let eo = u64::from_le_bytes(off); f.seek(SeekFrom::Start(eo + 40)).unwrap(); let mut sz = [0u8; 8]; f.read_exact(&mut sz).unwrap(); let mut co = [0u8; 8]; f.read_exact(&mut co).unwrap(); return Ok((u64::from_le_bytes(co), u64::from_le_bytes(sz))); } } Err("No ZIP64".into())
}

pub fn read_p4k_file(gp: &str, v: &str, fp: &str) -> Result<Vec<u8>, String> {
    let p = sc_p4k_path(gp, v)?; let mut f = File::open(&p).map_err(|e| e.to_string())?; let l = f.metadata().unwrap().len(); let (co, cs) = find_central_directory(&mut f, l)?; f.seek(SeekFrom::Start(co)).unwrap(); let mut cd = vec![0u8; cs as usize]; f.read_exact(&mut cd).unwrap();
    let search = fp.replace('/', "\\"); let mut pos = 0;
    while pos + 46 <= cd.len() { if &cd[pos..pos+4] != b"PK\x01\x02" { pos += 1; continue; }
        let nl = u16::from_le_bytes([cd[pos+28], cd[pos+29]]) as usize; let el = u16::from_le_bytes([cd[pos+30], cd[pos+31]]) as usize; let name = String::from_utf8_lossy(&cd[pos+46..pos+46+nl]);
        if name.eq_ignore_ascii_case(&search) { let mut csz = u32::from_le_bytes([cd[pos+20], cd[pos+21], cd[pos+22], cd[pos+23]]) as u64; let mut off = u32::from_le_bytes([cd[pos+42], cd[pos+43], cd[pos+44], cd[pos+45]]) as u64;
            if el >= 28 { let ex = pos + 46 + nl; if u16::from_le_bytes([cd[ex], cd[ex+1]]) == 0x0001 { csz = u64::from_le_bytes(cd[ex+12..ex+20].try_into().unwrap()); off = u64::from_le_bytes(cd[ex+20..ex+28].try_into().unwrap()); } }
            let m = u16::from_le_bytes([cd[pos+10], cd[pos+11]]); f.seek(SeekFrom::Start(off)).unwrap(); let mut lh = [0u8; 30]; f.read_exact(&mut lh).unwrap(); let doff = off + 30 + u16::from_le_bytes([lh[26], lh[27]]) as u64 + u16::from_le_bytes([lh[28], lh[29]]) as u64; f.seek(SeekFrom::Start(doff)).unwrap(); let mut d = vec![0u8; csz as usize]; f.read_exact(&mut d).unwrap();
            if m == 100 || m == 93 {
                // Use zstd-safe decoder with proper error handling
                let mut decoder = zstd::Decoder::new(Cursor::new(d)).map_err(|e| format!("zstd init error: {}", e))?;
                let mut out = vec![];
                decoder.read_to_end(&mut out).map_err(|e| format!("zstd decode error: {}", e))?;
                return Ok(out);
            } return Ok(d);
        } pos += 46 + nl + el + u16::from_le_bytes([cd[pos+32], cd[pos+33]]) as usize; } Err("Not found".into())
}

fn rcs(t: &[u8], o: usize) -> String { if o >= t.len() { return "".into(); } let e = t[o..].iter().position(|&b| b == 0).unwrap_or(t.len() - o); String::from_utf8_lossy(&t[o..o + e]).into_owned() }
fn tmn(b: &[u8], no: usize, ao: usize, st: &[u8], ct: &[u8], ni: usize, p: &mut ScActionProfile, mut cm: Option<usize>) {
    let s = no + ni * 28; if s + 28 > b.len() { return; }
    let nb = &b[s..s+28]; let tag = rcs(st, u32::from_le_bytes(nb[0..4].try_into().unwrap()) as usize).to_lowercase(); if tag == "xboxone" || tag == "gamepad" { return; }
    let ac = u16::from_le_bytes(nb[8..10].try_into().unwrap()) as usize; let cc = u16::from_le_bytes(nb[10..12].try_into().unwrap()) as usize;
    let fat = u32::from_le_bytes(nb[16..20].try_into().unwrap()) as usize; let fct = u32::from_le_bytes(nb[20..24].try_into().unwrap()) as usize;
    let mut attrs = HashMap::new(); for i in 0..ac { let a = ao + (fat + i) * 8; if a + 8 > b.len() { continue; } let k = rcs(st, u32::from_le_bytes(b[a..a+4].try_into().unwrap()) as usize).to_lowercase(); let v = rcs(st, u32::from_le_bytes(b[a+4..a+8].try_into().unwrap()) as usize); attrs.insert(k, v); }
    if tag == "actionmap" { if let Some(n) = attrs.get("name") { cm = Some(p.action_maps.len()); p.action_maps.push(ScActionMap { name: n.clone(), bindings: vec![], actions: vec![] }); } }
    else if tag == "action" { if let (Some(idx), Some(n)) = (cm, attrs.get("name").or(attrs.get("id"))) { p.action_maps[idx].actions.push(ScAction { name: n.clone(), label: attrs.get("label").or(attrs.get("uilabel")).cloned() }); } }
    else if tag == "rebind" { if let (Some(idx), Some(i)) = (cm, attrs.get("input")) { if let Some(la) = p.action_maps[idx].actions.last() { let an = la.name.clone(); p.action_maps[idx].bindings.push(ScBinding { action_name: an, input: i.clone() }); } } }
    for i in 0..cc { let c = (fct + i) * 4; if c + 4 <= ct.len() { tmn(b, no, ao, st, ct, u32::from_le_bytes(ct[c..c+4].try_into().unwrap()) as usize, p, cm); } }
}

fn parse_cryxmlb_full(b: &[u8]) -> Result<ParsedActionMaps, String> {
    if !b.starts_with(b"CryXmlB") { return Err("Not CryXmlB".into()); }
    let no = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize; let ao = u32::from_le_bytes(b[20..24].try_into().unwrap()) as usize;
    let co = u32::from_le_bytes(b[28..32].try_into().unwrap()) as usize; let cc = u32::from_le_bytes(b[32..36].try_into().unwrap()) as usize;
    let so = u32::from_le_bytes(b[36..40].try_into().unwrap()) as usize; let ss = u32::from_le_bytes(b[40..44].try_into().unwrap()) as usize;
    let st = &b[so..so + ss]; let ct = &b[co..co + cc * 4];
    let mut p = ScActionProfile { profile_name: "master".into(), version: "1".into(), options_version: "2".into(), rebind_version: "2".into(), devices: vec![], device_options: vec![], action_maps: vec![] };
    tmn(b, no, ao, st, ct, 0, &mut p, None); Ok(ParsedActionMaps { version: "1".into(), profiles: vec![p] })
}

#[tauri::command] pub async fn read_user_cfg(gp: String, v: String) -> Result<String, String> { let p = sc_base_dir(&expand_tilde(&gp), &v).join("USER.cfg"); if !p.exists() { return Ok("".into()); } fs::read_to_string(p).map_err(|e| e.to_string()) }
#[tauri::command] pub async fn write_user_cfg(gp: String, v: String, c: String) -> Result<(), String> { let p = sc_base_dir(&expand_tilde(&gp), &v).join("USER.cfg"); if let Some(parent) = p.parent() { fs::create_dir_all(parent).ok(); } fs::write(p, c).map_err(|e| e.to_string()) }
#[tauri::command] pub async fn detect_sc_versions(gp: String) -> Result<Vec<ScVersionInfo>, String> {
    eprintln!("[detect_sc_versions] ========== START ==========");
    eprintln!("[detect_sc_versions] Input game_path: '{}'", gp);

    let exp = expand_tilde(&gp);
    eprintln!("[detect_sc_versions] Expanded path: '{}'", exp);

    // Try multiple possible paths
    let paths_to_try: Vec<PathBuf> = vec![
        // Wine prefix path (standard)
        Path::new(&exp).join("drive_c/Program Files/Roberts Space Industries/StarCitizen"),
        // Direct Linux installation
        Path::new(&exp).join("StarCitizen"),
        // The game path itself might be the StarCitizen folder
        Path::new(&exp).to_path_buf(),
    ];

    for base in paths_to_try.iter() {
        eprintln!("[detect_sc_versions] Checking path: {}", base.display());
        eprintln!("[detect_sc_versions]   Exists: {}, IsDir: {}", base.exists(), base.is_dir());

        if base.exists() && base.is_dir() {
            // List contents of directory for debugging
            if let Ok(entries) = fs::read_dir(base) {
                let entry_names: Vec<String> = entries.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .take(10)
                    .collect();
                eprintln!("[detect_sc_versions]   First 10 entries: {:?}", entry_names);

                let has_version_folders: bool = entry_names.iter().any(|name| {
                    let n = name.to_lowercase();
                    n == "live" || n == "ptu" || n == "eptu" || n == "hotfix"
                });

                if has_version_folders {
                    eprintln!("[detect_sc_versions] Found valid SC installation at: {}", base.display());
                    return detect_sc_versions_from_path(base);
                } else {
                    eprintln!("[detect_sc_versions]   No version folders found in this directory");
                }
            }
        }
    }

    // Return error with path info for debugging
    eprintln!("[detect_sc_versions] ========== END - NOT FOUND ==========");
    Err(format!("StarCitizen directory not found. Game path: '{}'", gp))
}

fn detect_sc_versions_from_path(base: &Path) -> Result<Vec<ScVersionInfo>, String> {
    let mut res = vec![];
    eprintln!("[detect_sc_versions] Reading directory: {}", base.display());

    match fs::read_dir(base) {
        Ok(es) => {
            for e in es.flatten() {
                let path = e.path();
                if !path.is_dir() {
                    continue;
                }
                let n = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                eprintln!("[detect_sc_versions] Found version folder: {}", n);

                // Check profiles path
                let profiles_path = path.join("user/client/0/Profiles/default");
                let has_usercfg = path.join("USER.cfg").exists();
                let has_attributes = profiles_path.join("attributes.xml").exists();
                let has_actionmaps = profiles_path.join("actionmaps.xml").exists();
                let has_exported_layouts = path.join("user/client/0/controls/mappings").is_dir();
                eprintln!("[detect_sc_versions]   has_usercfg: {}, has_attributes: {}, has_actionmaps: {}", has_usercfg, has_attributes, has_actionmaps);
                res.push(ScVersionInfo {
                    version: n,
                    path: path.to_string_lossy().into_owned(),
                    has_usercfg,
                    has_attributes,
                    has_actionmaps,
                    has_exported_layouts,
                });
            }
        }
        Err(e) => {
            eprintln!("[detect_sc_versions] Failed to read directory: {} - Error: {}", base.display(), e);
        }
    }

    res.sort_by_key(|v| match v.version.as_str() {
        "LIVE" => 0,
        "PTU" => 1,
        "HOTFIX" => 2,
        _ => 3
    });
    eprintln!("[detect_sc_versions] Returning {} versions", res.len());
    Ok(res)
}
#[tauri::command] pub async fn list_profiles(gp: String, v: String) -> Result<Vec<ScProfile>, String> { let p = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles"); if !p.is_dir() { return Ok(vec![]); } let mut res = vec![]; if let Ok(es) = fs::read_dir(p) { for e in es.flatten() { let path = e.path(); if !path.is_dir() || path.file_name().unwrap_or_default() == "frontend" { continue; } let n = path.file_name().unwrap_or_default().to_string_lossy().into_owned(); let mut last = 0; if let Ok(c) = fs::read_to_string(path.join("attributes.xml")) { if let Some(s) = c.find("lastPlayed=\"") { if let Some(e) = c[s+12..].find('"') { last = c[s+12..s+12+e].parse().unwrap_or(0); } } } res.push(ScProfile { name: n, last_played: last }); } } res.sort_by(|a, b| a.name.cmp(&b.name)); Ok(res) }
#[tauri::command] pub async fn export_profile(gp: String, v: String, dp: String) -> Result<(), String> { let src = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles/default"); let dest = Path::new(&dp); fs::create_dir_all(dest).map_err(|e| e.to_string())?; for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] { if src.join(f).exists() { fs::copy(src.join(f), dest.join(f)).ok(); } } Ok(()) }
#[tauri::command] pub async fn import_profile(gp: String, v: String, sp: String) -> Result<(), String> { let dest = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles/default"); let src = Path::new(&sp); fs::create_dir_all(&dest).map_err(|e| e.to_string())?; for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] { if src.join(f).exists() { fs::copy(src.join(f), dest.join(f)).ok(); } } Ok(()) }
#[tauri::command] pub async fn read_attributes(gp: String, v: String) -> Result<ScAttributes, String> {
    let p = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles/default/attributes.xml"); if !p.exists() { return Ok(ScAttributes::default()); }
    let c = fs::read_to_string(p).map_err(|e| e.to_string())?; let mut attrs = ScAttributes::default(); if let Some(s) = c.find("Version=\"").or(c.find("version=\"")) { let st = s + 9; if let Some(e) = c[st..].find('"') { attrs.version = c[st..st+e].to_string(); } }
    let mut pos = 0; while let Some(s) = c[pos..].find("<Attr ") { let st = pos + s; if let Some(ns) = c[st..].find("name=\"") { let nst = st + ns + 6; if let Some(ne) = c[nst..].find('"') { let n = c[nst..nst+ne].to_string(); if let Some(vs) = c[nst+ne..].find("value=\"") { let vst = nst + ne + vs + 7; if let Some(ve) = c[vst..].find('"') { attrs.attrs.push(ScAttribute { name: n, value: c[vst..vst+ve].to_string() }); } } } } pos = st + 1; } Ok(attrs)
}
#[tauri::command] pub async fn write_attributes(gp: String, v: String, attrs: ScAttributes) -> Result<(), String> { let p = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles/default/attributes.xml"); if let Some(parent) = p.parent() { fs::create_dir_all(parent).ok(); } let mut xml = format!("<Attributes Version=\"{}\">\n", attrs.version); for a in attrs.attrs { xml.push_str(&format!(" <Attr name=\"{}\" value=\"{}\"/>\n", a.name, a.value)); } xml.push_str("</Attributes>\n"); fs::write(p, xml).map_err(|e| e.to_string()) }
#[tauri::command] pub async fn parse_actionmaps(gp: String, v: String, source: Option<String>) -> Result<ParsedActionMaps, String> { let exp = expand_tilde(&gp); let p = match source { Some(f) => sc_base_dir(&exp, &v).join("user/client/0/controls/mappings").join(f), None => sc_base_dir(&exp, &v).join("user/client/0/Profiles/default/actionmaps.xml") }; if !p.exists() { return Err("Not found".into()); } parse_actionmaps_xml(&fs::read_to_string(p).map_err(|e| e.to_string())?) }
#[tauri::command] pub fn get_action_definitions() -> ActionDefinitions { ActionDefinitions::new() }
pub async fn get_master_bindings(gp: String, v: String) -> Result<ParsedActionMaps, String> {
    let pp = sc_p4k_path(&gp, &v)?; let cp = master_bindings_cache_path(&v)?; let meta = fs::metadata(&pp).map_err(|e| e.to_string())?; let (sz, modif) = (meta.len(), meta.modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs());
    if cp.exists() { if let Ok(c) = fs::read_to_string(&cp) { if let Ok(cached) = serde_json::from_str::<CachedMasterBindings>(&c) { if cached.p4k_size == sz && cached.p4k_modified == modif { return Ok(cached.data); } } } }
    let _g = MASTER_BINDINGS_LOCK.lock().await; let res: Result<ParsedActionMaps, String> = tokio::task::spawn_blocking(move || -> Result<ParsedActionMaps, String> { 
        let files = { let p = sc_base_dir(&gp, &v).join("Data.p4k"); if !p.exists() { return Err("No P4K".into()); }
            let mut f = File::open(&p).map_err(|e| e.to_string())?; let l = f.metadata().unwrap().len(); let (co, cs) = find_central_directory(&mut f, l)?; f.seek(SeekFrom::Start(co)).unwrap(); let mut cd = vec![0u8; cs as usize]; f.read_exact(&mut cd).unwrap();
            let mut fls = vec![]; let pat = "defaultProfile.xml".to_lowercase(); let mut po = 0;
            while po + 46 <= cd.len() { if &cd[po..po+4] != b"PK\x01\x02" { po += 1; continue; } let nl = u16::from_le_bytes([cd[po+28], cd[po+29]]) as usize; let name = String::from_utf8_lossy(&cd[po+46..po+46+nl]); if name.to_lowercase().contains(&pat) { fls.push(name.to_string()); } po += 46 + nl + u16::from_le_bytes([cd[po+30], cd[po+31]]) as usize + u16::from_le_bytes([cd[po+32], cd[po+33]]) as usize; } fls };
        let mp = files.iter().find(|f| f.ends_with("defaultProfile.xml")).ok_or("No master")?; let mr = read_p4k_file(&gp, &v, &mp)?; let data = parse_cryxmlb_full(&mr)?; let cached = CachedMasterBindings { p4k_size: sz, p4k_modified: modif, data: data.clone() }; 
        let cp_path = master_bindings_cache_path(&v)?; fs::create_dir_all(cp_path.parent().unwrap()).ok(); fs::write(cp_path, serde_json::to_string(&cached).unwrap()).ok(); Ok(data) }).await.unwrap(); res
}
#[tauri::command] pub async fn get_localization_labels(game_path: String, version: String, language: Option<String>) -> Result<HashMap<String, String>, String> {
    let pp = sc_p4k_path(&game_path, &version)?; let cp = localization_cache_path(&version)?; let meta = fs::metadata(&pp).unwrap(); let (sz, modif) = (meta.len(), meta.modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_secs());
    if cp.exists() { if let Ok(s) = fs::read_to_string(&cp) { if let Ok(cached) = serde_json::from_str::<CachedLocalization>(&s) { if cached.p4k_size == sz && cached.p4k_modified == modif { return Ok(cached.labels); } } } }
    let _g = LOCALIZATION_LOCK.lock().await; let l = language.unwrap_or("english".into());
    let res: Result<HashMap<String, String>, String> = tokio::task::spawn_blocking(move || -> Result<HashMap<String, String>, String> { let b = read_p4k_file(&game_path, &version, &format!("Localization/{}/global.ini", l)).or_else(|_| read_p4k_file(&game_path, &version, &format!("Data/Localization/{}/global.ini", l)))?; let c_str = if b.starts_with(&[0xFF, 0xFE]) { UTF_16LE.decode(&b[2..]).0.into_owned() } else { String::from_utf8_lossy(&b).into_owned() }; let labels = parse_global_ini(&c_str); let cached = CachedLocalization { p4k_size: sz, p4k_modified: modif, labels: labels.clone() }; let cp_path = localization_cache_path(&version)?; fs::create_dir_all(cp_path.parent().unwrap()).ok(); fs::write(cp_path, serde_json::to_string(&cached).unwrap()).ok(); Ok(labels) }).await.unwrap(); res
}
#[tauri::command] pub async fn get_complete_binding_list(gp: String, v: String) -> Result<BindingListResponse, String> {
    let labels = get_localization_labels(gp.clone(), v.clone(), None).await.unwrap_or_default(); let master = get_master_bindings(gp.clone(), v.clone()).await?; let master_profile = &master.profiles[0];
    let user_p = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles/default/actionmaps.xml"); let user_parsed = if user_p.exists() { parse_actionmaps_xml(&fs::read_to_string(user_p).unwrap_or_default()).ok() } else { None };
    let user_profile = user_parsed.as_ref().and_then(|up| up.profiles.iter().find(|p| p.profile_name == "default" || p.profile_name.is_empty()));
    let mut merged: HashMap<String, HashMap<String, (Vec<String>, Option<String>, bool)>> = HashMap::new();
    for am in &master_profile.action_maps { let map = merged.entry(am.name.clone()).or_insert_with(HashMap::new); for a in &am.actions { map.entry(a.name.clone()).or_insert_with(|| (vec![], a.label.clone(), false)); } for b in &am.bindings { let entry = map.entry(b.action_name.clone()).or_insert_with(|| (vec![], None, false)); if !entry.0.contains(&b.input) { entry.0.push(b.input.clone()); } } }
    if let Some(up) = user_profile { for am in &up.action_maps { let map = merged.entry(am.name.clone()).or_insert_with(HashMap::new); for b in &am.bindings { let entry = map.entry(b.action_name.clone()).or_insert_with(|| (vec![], None, false)); if !entry.0.contains(&b.input) { entry.0.push(b.input.clone()); } entry.2 = true; } } }
    let mut results = vec![]; let mut stats = BindingStats { total: 0, custom: 0 };
    for (cat_name, actions) in merged { let cat_label = labels.get(&format!("ui_Control{}", cat_name)).or(labels.get(&cat_name)).cloned().unwrap_or_else(|| cat_name.replace('_', " "));
        for (an, (inputs, alabel, is_custom)) in actions { stats.total += 1; if is_custom { stats.custom += 1; } let dn = alabel.as_ref().and_then(|l| labels.get(l.strip_prefix('@').unwrap_or(l))).or(labels.get(&format!("ui_Control{}", an))).or(labels.get(&an)).cloned().unwrap_or_else(|| an.replace('_', " "));
            if inputs.is_empty() { results.push(CompleteBinding { category: cat_name.clone(), category_label: cat_label.clone(), action_name: an.clone(), display_name: dn.clone(), current_input: "".into(), device_type: "none".into(), description: None, is_custom }); }
            else { for input in inputs { results.push(CompleteBinding { category: cat_name.clone(), category_label: cat_label.clone(), action_name: an.clone(), display_name: dn.clone(), current_input: input, device_type: "none".into(), description: None, is_custom }); } } }
    }
    results.sort_by(|a, b| a.category_label.to_lowercase().cmp(&b.category_label.to_lowercase()).then(a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()))); Ok(BindingListResponse { bindings: results, stats })
}
#[derive(Deserialize)] pub struct AssignBindingArgs { pub game_path: String, pub version: String, pub action_name: String, pub category: String, pub input: String, pub old_input: Option<String> }
#[tauri::command] pub async fn assign_binding(args: AssignBindingArgs) -> Result<(), String> {
    let p = sc_base_dir(&expand_tilde(&args.game_path), &args.version).join("user/client/0/Profiles/default/actionmaps.xml"); let mut parsed = if p.exists() { parse_actionmaps_xml(&fs::read_to_string(&p).unwrap())? } else { ParsedActionMaps { version: "1".into(), profiles: vec![ScActionProfile { profile_name: "default".into(), version: "1".into(), options_version: "2".into(), rebind_version: "2".into(), devices: vec![], device_options: vec![], action_maps: vec![] }] } };
    let profile = parsed.profiles.iter_mut().find(|pr| pr.profile_name == "default" || pr.profile_name.is_empty()).ok_or("No profile")?;
    let mut found = false; for am in &mut profile.action_maps { if am.name == args.category { if let Some(ref old) = args.old_input { if let Some(b) = am.bindings.iter_mut().find(|b| b.action_name == args.action_name && b.input == *old) { b.input = args.input.clone(); found = true; break; } } else if let Some(b) = am.bindings.iter_mut().find(|b| b.action_name == args.action_name) { b.input = args.input.clone(); found = true; break; } } }
    if !found { if let Some(am) = profile.action_maps.iter_mut().find(|am| am.name == args.category) { am.bindings.push(ScBinding { action_name: args.action_name.clone(), input: args.input }); } else { profile.action_maps.push(ScActionMap { name: args.category.clone(), bindings: vec![ScBinding { action_name: args.action_name.clone(), input: args.input }], actions: vec![ScAction { name: args.action_name, label: None }] }); } }
    write_actionmaps_xml(&p, &parsed)
}
#[derive(Deserialize)] pub struct RemoveBindingArgs { pub game_path: String, pub version: String, pub action_name: String, pub input: String, #[allow(dead_code)] pub category: String }
#[tauri::command] pub async fn remove_binding(args: RemoveBindingArgs) -> Result<(), String> {
    let p = sc_base_dir(&expand_tilde(&args.game_path), &args.version).join("user/client/0/Profiles/default/actionmaps.xml"); if !p.exists() { return Err("Not found".into()); }
    let mut parsed = parse_actionmaps_xml(&fs::read_to_string(&p).unwrap())?; let profile = parsed.profiles.iter_mut().find(|pr| pr.profile_name == "default" || pr.profile_name.is_empty()).ok_or("No profile")?;
    for am in &mut profile.action_maps { am.bindings.retain(|b| !(b.action_name == args.action_name && b.input == args.input)); }
    write_actionmaps_xml(&p, &parsed)
}
#[tauri::command] pub async fn read_p4k(game_path: String, version: String, file_path: String) -> Result<String, String> { Ok(String::from_utf8_lossy(&read_p4k_file(&game_path, &version, &file_path)?).into_owned()) }
#[tauri::command] pub async fn list_p4k(game_path: String, version: String, pattern: Option<String>) -> Result<Vec<String>, String> { 
    let p = sc_base_dir(&game_path, &version).join("Data.p4k"); if !p.exists() { return Err("No P4K".into()); }
    let mut f = File::open(&p).map_err(|e| e.to_string())?; let l = f.metadata().unwrap().len(); let (co, cs) = find_central_directory(&mut f, l)?; f.seek(SeekFrom::Start(co)).unwrap(); let mut cd = vec![0u8; cs as usize]; f.read_exact(&mut cd).unwrap();
    let mut fls = vec![]; let p_str = pattern.unwrap_or_default().to_lowercase(); let mut po = 0;
    while po + 46 <= cd.len() { if &cd[po..po+4] != b"PK\x01\x02" { po += 1; continue; } let nl = u16::from_le_bytes([cd[po+28], cd[po+29]]) as usize; let name = String::from_utf8_lossy(&cd[po+46..po+46+nl]); if name.to_lowercase().contains(&p_str) { fls.push(name.to_string()); } po += 46 + nl + u16::from_le_bytes([cd[po+30], cd[po+31]]) as usize + u16::from_le_bytes([cd[po+32], cd[po+33]]) as usize; }
    Ok(fls)
}
#[tauri::command] pub async fn get_localization_ini(gp: String, v: String, lang: Option<String>) -> Result<String, String> { let b = read_p4k_file(&gp, &v, &format!("Data/Localization/{}/global.ini", lang.unwrap_or("english".into())))?; Ok(String::from_utf8_lossy(&b).into_owned()) }
#[tauri::command] pub async fn list_localization_languages(gp: String, v: String) -> Result<Vec<String>, String> {
    let fs = { let p = sc_base_dir(&gp, &v).join("Data.p4k"); if !p.exists() { return Err("No P4K".into()); }
        let mut f = File::open(&p).map_err(|e| e.to_string())?; let l = f.metadata().unwrap().len(); let (co, cs) = find_central_directory(&mut f, l)?; f.seek(SeekFrom::Start(co)).unwrap(); let mut cd = vec![0u8; cs as usize]; f.read_exact(&mut cd).unwrap();
        let mut fls = vec![]; let mut po = 0; while po + 46 <= cd.len() { if &cd[po..po+4] != b"PK\x01\x02" { po += 1; continue; } let nl = u16::from_le_bytes([cd[po+28], cd[po+29]]) as usize; let name = String::from_utf8_lossy(&cd[po+46..po+46+nl]); if name.to_lowercase().contains("localization/") { fls.push(name.to_string()); } po += 46 + nl + u16::from_le_bytes([cd[po+30], cd[po+31]]) as usize + u16::from_le_bytes([cd[po+32], cd[po+33]]) as usize; } fls };
    let mut ls = HashSet::new(); for f in fs { if let Some(r) = f.strip_prefix("Data\\Localization\\").or(f.strip_prefix("Localization\\")) { if let Some(e) = r.find('\\') { let l = r[..e].to_string(); if !l.is_empty() { ls.insert(l); } } } }
    let mut res: Vec<String> = ls.into_iter().collect(); res.sort(); Ok(res)
}
#[tauri::command] pub async fn reorder_devices(gp: String, v: String, new_order: Vec<DeviceReorderEntry>) -> Result<(), String> {
    let p = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles/default/actionmaps.xml"); if !p.exists() { return Err("Not found".into()); }
    let mut c = fs::read_to_string(&p).map_err(|e| e.to_string())?;
    for e in &new_order { let o = format!("instance=\"{}\"", e.old_instance); let pl = format!("instance=\"__REMAP_{}__\"", e.new_instance); c = c.replace(&o, &pl); }
    for e in &new_order { let pl = format!("instance=\"__REMAP_{}__\"", e.new_instance); let f = format!("instance=\"{}\"", e.new_instance); c = c.replace(&pl, &f); }
    fs::write(p, c).map_err(|e| e.to_string())
}
#[tauri::command] pub async fn backup_profile_manual(gp: String, v: String, l: String) -> Result<BackupInfo, String> { backup_profile(gp, v, Some("manual".into()), Some(l)).await }
#[tauri::command] pub async fn backup_profile(gp: String, v: String, bt: Option<String>, l: Option<String>) -> Result<BackupInfo, String> {
    let now = Local::now(); let id = now.format("%Y-%m-%dT%H-%M-%S").to_string(); let bdir = backup_version_dir(&v)?; fs::create_dir_all(&bdir.join(&id)).ok(); let target = bdir.join(&id); let pdir = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles/default"); let mut fs_list = vec![];
    for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] { if pdir.join(f).exists() { fs::copy(pdir.join(f), target.join(f)).ok(); fs_list.push(f.to_string()); } }
    let info = BackupInfo { id, created_at: now.format("%Y-%m-%d %H:%M:%S").to_string(), timestamp: now.timestamp() as u64, version: v, backup_type: bt.unwrap_or("manual".into()), files: fs_list, label: l.unwrap_or_default() };
    fs::write(target.join("backup_meta.json"), serde_json::to_string_pretty(&info).unwrap()).ok(); Ok(info)
}
#[tauri::command] pub async fn restore_profile(gp: String, v: String, bid: String) -> Result<(), String> { let bdir = backup_version_dir(&v)?.join(bid); let pdir = sc_base_dir(&expand_tilde(&gp), &v).join("user/client/0/Profiles/default"); fs::create_dir_all(&pdir).ok(); for f in &["actionmaps.xml", "attributes.xml", "profile.xml"] { if bdir.join(f).exists() { fs::copy(bdir.join(f), pdir.join(f)).ok(); } } Ok(()) }
#[tauri::command] pub async fn list_backups(v: String) -> Result<Vec<BackupInfo>, String> { let d = backup_version_dir(&v)?; if !d.is_dir() { return Ok(vec![]); } let mut res = vec![]; if let Ok(es) = fs::read_dir(d) { for e in es.flatten() { if let Ok(c) = fs::read_to_string(e.path().join("backup_meta.json")) { if let Ok(i) = serde_json::from_str::<BackupInfo>(&c) { res.push(i); } } } } res.sort_by_key(|b| std::cmp::Reverse(b.timestamp)); Ok(res) }
#[tauri::command] pub async fn delete_backup(v: String, bid: String) -> Result<(), String> { let p = backup_version_dir(&v)?.join(bid); if p.is_dir() { fs::remove_dir_all(p).ok(); } Ok(()) }
#[tauri::command] pub async fn update_backup_label(v: String, bid: String, l: String) -> Result<(), String> { let p = backup_version_dir(&v)?.join(bid).join("backup_meta.json"); let mut i: BackupInfo = serde_json::from_str(&fs::read_to_string(&p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?; i.label = l; fs::write(p, serde_json::to_string_pretty(&i).unwrap()).map_err(|e| e.to_string()) }
#[tauri::command] pub async fn list_exported_layouts(game_path: String, version: String) -> Result<Vec<ExportedLayout>, String> { let d = sc_base_dir(&expand_tilde(&game_path), &version).join("user/client/0/controls/mappings"); if !d.is_dir() { return Ok(vec![]); } let mut res = vec![]; if let Ok(es) = fs::read_dir(d) { for e in es.flatten() { let path = e.path(); if path.extension().map_or(false, |ext| ext == "xml") { let f = path.file_name().unwrap_or_default().to_string_lossy().into_owned(); let m = path.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0); res.push(ExportedLayout { label: f.trim_end_matches(".xml").replace('_', " "), filename: f, modified: m }); } } } res.sort_by_key(|l| std::cmp::Reverse(l.modified)); Ok(res) }
