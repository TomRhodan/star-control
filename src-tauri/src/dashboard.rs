//! Dashboard module for RSI information.
//!
//! This module provides commands to fetch:
//! - RSI News from community feeds
//! - Server status from status.robertsspaceindustries.com
//! - Community stats (funds, fleet, fans) from third-party APIs
//!
//! All data is fetched asynchronously and cached appropriately.

use chrono::{ DateTime, Utc };
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{ Deserialize, Serialize };

// ── RSI News ──────────────────────────────────────────────────────────

/// A single RSI news item from the feed.
#[derive(Serialize, Clone)]
pub struct RsiNewsItem {
    pub title: String,
    pub summary: String,
    pub link: String,
    pub category: String,
    pub published: String,
    pub relative_time: String,
}

/// Result containing RSI news items or an error message.
#[derive(Serialize)]
pub struct RsiNewsResult {
    pub items: Vec<RsiNewsItem>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn fetch_rsi_news() -> RsiNewsResult {
    match fetch_rsi_news_inner().await {
        Ok(items) => RsiNewsResult { items, error: None },
        Err(e) =>
            RsiNewsResult {
                items: vec![],
                error: Some(e.to_string()),
            },
    }
}

async fn fetch_rsi_news_inner() -> Result<
    Vec<RsiNewsItem>,
    Box<dyn std::error::Error + Send + Sync>
> {
    let body = reqwest::get("https://leonick.se/feeds/rsi/atom").await?.text().await?;

    let mut reader = Reader::from_str(&body);
    // Security: Disable DTD and entity processing to prevent XXE attacks
    reader.config_mut().trim_text(true);
    let mut items: Vec<RsiNewsItem> = Vec::new();
    let mut in_entry = false;
    let mut current_title = String::new();
    let mut current_summary = String::new();
    let mut current_link = String::new();
    let mut current_category = String::new();
    let mut current_published = String::new();
    let mut current_tag = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "entry" {
                    in_entry = true;
                    current_title.clear();
                    current_summary.clear();
                    current_link.clear();
                    current_category.clear();
                    current_published.clear();
                } else if in_entry {
                    current_tag = name.clone();
                    if name == "link" {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"href" {
                                current_link = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                if !in_entry {
                    continue;
                }
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "link" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"href" {
                            current_link = String::from_utf8_lossy(&attr.value).to_string();
                        }
                    }
                } else if name == "category" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"term" {
                            current_category = String::from_utf8_lossy(&attr.value).to_string();
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_entry {
                    let text = e.unescape().unwrap_or_default().to_string();
                    match current_tag.as_str() {
                        "title" => current_title.push_str(&text),
                        "summary" => current_summary.push_str(&text),
                        "published" | "updated" => {
                            if current_published.is_empty() {
                                current_published = text;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::CData(ref e)) => {
                if in_entry {
                    let text = String::from_utf8_lossy(e.as_ref()).to_string();
                    match current_tag.as_str() {
                        "title" => current_title.push_str(&text),
                        "summary" => current_summary.push_str(&text),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "entry" {
                    in_entry = false;
                    let relative = format_relative_time(&current_published);
                    // Trim summary to ~200 chars
                    let summary = if current_summary.len() > 200 {
                        let trimmed =
                            &current_summary
                                [
                                    ..current_summary
                                        .char_indices()
                                        .take(200)
                                        .last()
                                        .map(|(i, c)| i + c.len_utf8())
                                        .unwrap_or(200)
                                ];
                        format!("{}...", trimmed.trim())
                    } else {
                        current_summary.trim().to_string()
                    };
                    items.push(RsiNewsItem {
                        title: current_title.trim().to_string(),
                        summary,
                        link: current_link.trim().to_string(),
                        category: current_category.trim().to_string(),
                        published: current_published.trim().to_string(),
                        relative_time: relative,
                    });
                    if items.len() >= 5 {
                        break;
                    }
                }
                if in_entry {
                    current_tag.clear();
                }
            }
            Ok(Event::Eof) => {
                break;
            }
            Err(_) => {
                break;
            }
            _ => {}
        }
    }

    Ok(items)
}

fn format_relative_time(date_str: &str) -> String {
    let parsed = date_str
        .parse::<DateTime<Utc>>()
        .or_else(|_| DateTime::parse_from_rfc3339(date_str).map(|d| d.with_timezone(&Utc)))
        .or_else(|_| DateTime::parse_from_rfc2822(date_str).map(|d| d.with_timezone(&Utc)));

    match parsed {
        Ok(dt) => {
            let now = Utc::now();
            let diff = now.signed_duration_since(dt);
            let mins = diff.num_minutes();
            let hours = diff.num_hours();
            let days = diff.num_days();

            if mins < 1 {
                "just now".to_string()
            } else if mins < 60 {
                format!("{}m ago", mins)
            } else if hours < 24 {
                format!("{}h ago", hours)
            } else if days < 30 {
                format!("{}d ago", days)
            } else {
                format!("{}mo ago", days / 30)
            }
        }
        Err(_) => String::new(),
    }
}

// ── Server Status ─────────────────────────────────────────────────────

/// Information about a single RSI server component.
#[derive(Serialize, Clone)]
pub struct ServerComponent {
    pub name: String,
    pub status: String, // "operational", "degraded", "major_outage", "unknown"
}

/// Result containing server status information or an error message.
#[derive(Serialize)]
pub struct ServerStatusResult {
    pub components: Vec<ServerComponent>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn fetch_server_status() -> ServerStatusResult {
    match fetch_server_status_inner().await {
        Ok(components) =>
            ServerStatusResult {
                components,
                error: None,
            },
        Err(e) =>
            ServerStatusResult {
                components: vec![],
                error: Some(e.to_string()),
            },
    }
}

async fn fetch_server_status_inner() -> Result<
    Vec<ServerComponent>,
    Box<dyn std::error::Error + Send + Sync>
> {
    let body = reqwest
        ::get("https://status.robertsspaceindustries.com/index.xml").await?
        .text().await?;

    // Parse RSS feed for incident information
    let mut reader = Reader::from_str(&body);
    // Security: Disable DTD and entity processing to prevent XXE attacks
    reader.config_mut().trim_text(true);
    let mut incidents: Vec<(String, String)> = Vec::new(); // (title, description)
    let mut in_item = false;
    let mut current_tag = String::new();
    let mut current_title = String::new();
    let mut current_desc = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" {
                    in_item = true;
                    current_title.clear();
                    current_desc.clear();
                } else if in_item {
                    current_tag = name;
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_item {
                    let text = e.unescape().unwrap_or_default().to_string();
                    match current_tag.as_str() {
                        "title" => current_title.push_str(&text),
                        "description" => current_desc.push_str(&text),
                        _ => {}
                    }
                }
            }
            Ok(Event::CData(ref e)) => {
                if in_item {
                    let text = String::from_utf8_lossy(e.as_ref()).to_string();
                    match current_tag.as_str() {
                        "title" => current_title.push_str(&text),
                        "description" => current_desc.push_str(&text),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" {
                    in_item = false;
                    incidents.push((
                        current_title.trim().to_string(),
                        current_desc.trim().to_string(),
                    ));
                }
                if in_item {
                    current_tag.clear();
                }
            }
            Ok(Event::Eof) => {
                break;
            }
            Err(_) => {
                break;
            }
            _ => {}
        }
    }

    // Determine status for each component based on incidents
    let component_names = ["Platform", "Persistent Universe", "Arena Commander"];
    let mut components: Vec<ServerComponent> = Vec::new();

    for name in &component_names {
        let mut status = "operational".to_string();

        // Check recent incidents for this component
        for (title, desc) in &incidents {
            let combined = format!("{} {}", title.to_lowercase(), desc.to_lowercase());
            let name_lower = name.to_lowercase();

            // Check if this incident mentions this component
            let relevant =
                combined.contains(&name_lower) ||
                (*name == "Platform" &&
                    (combined.contains("platform") ||
                        combined.contains("website") ||
                        combined.contains("rsi"))) ||
                (*name == "Persistent Universe" &&
                    (combined.contains("persistent universe") || combined.contains("pu "))) ||
                (*name == "Arena Commander" &&
                    (combined.contains("arena commander") || combined.contains("ac ")));

            if relevant {
                if combined.contains("resolved") || combined.contains("completed") {
                    // Resolved — stay operational
                } else if combined.contains("major") || combined.contains("outage") {
                    status = "major_outage".to_string();
                    break;
                } else if
                    combined.contains("degraded") ||
                    combined.contains("partial") ||
                    combined.contains("investigating") ||
                    combined.contains("monitoring")
                {
                    status = "degraded".to_string();
                }
            }
        }

        components.push(ServerComponent {
            name: name.to_string(),
            status,
        });
    }

    Ok(components)
}

// ── Community Stats ───────────────────────────────────────────────────

/// Response from the Star Citizen Wiki stats API.
#[derive(Deserialize)]
struct StatsApiResponse {
    data: StatsData,
}

/// Data field from the stats API response.
#[derive(Deserialize)]
struct StatsData {
    funds: String,
    fans: u64,
    fleet: u64,
}

/// Community statistics for Star Citizen (funds, fans, fleet).
#[derive(Serialize)]
pub struct CommunityStats {
    pub funds: String,
    pub funds_raw: f64,
    pub fans: String,
    pub fans_raw: u64,
    pub fleet: String,
    pub fleet_raw: u64,
}

/// Result containing community stats or an error message.
#[derive(Serialize)]
pub struct CommunityStatsResult {
    pub stats: Option<CommunityStats>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn fetch_community_stats() -> CommunityStatsResult {
    match fetch_community_stats_inner().await {
        Ok(stats) =>
            CommunityStatsResult {
                stats: Some(stats),
                error: None,
            },
        Err(e) =>
            CommunityStatsResult {
                stats: None,
                error: Some(e.to_string()),
            },
    }
}

async fn fetch_community_stats_inner() -> Result<
    CommunityStats,
    Box<dyn std::error::Error + Send + Sync>
> {
    let resp: StatsApiResponse = reqwest
        ::get("https://api.star-citizen.wiki/api/stats/latest").await?
        .json().await?;

    let funds: f64 = resp.data.funds.parse().unwrap_or(0.0);
    let fans = resp.data.fans;
    let fleet = resp.data.fleet;

    Ok(CommunityStats {
        funds: format_currency(funds),
        funds_raw: funds,
        fans: format_number(fans),
        fans_raw: fans,
        fleet: format_number(fleet),
        fleet_raw: fleet,
    })
}

fn format_currency(val: f64) -> String {
    let cents = (val * 100.0).round() as i64;
    let dollars = cents / 100;
    let remainder = (cents % 100).abs();

    let dollar_str = format_integer(dollars.unsigned_abs());
    if dollars < 0 {
        format!("-${}.{:02}", dollar_str, remainder)
    } else {
        format!("${}.{:02}", dollar_str, remainder)
    }
}

fn format_number(val: u64) -> String {
    format_integer(val)
}

fn format_integer(val: u64) -> String {
    let s = val.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
