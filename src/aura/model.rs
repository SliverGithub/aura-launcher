//! JSON shapes emitted by Aurelia, plus small formatting helpers.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GameAssets {
    #[serde(default)]
    pub header: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Game {
    #[serde(default)]
    pub app_id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub playtime_forever_minutes: Option<u32>,
    #[serde(default)]
    pub is_installed: bool,
    #[serde(default)]
    pub install_path: Option<String>,
    #[serde(default)]
    pub update_available: bool,
    #[serde(default)]
    pub active_branch: Option<String>,
    #[serde(default)]
    pub is_owned: Option<bool>,
    #[serde(default)]
    pub is_family_shared: bool,
    #[serde(default)]
    pub online_required: Option<bool>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub from_windows_steam: bool,
    #[serde(default)]
    pub assets: GameAssets,
    #[serde(default)]
    pub store_url: Option<String>,
    #[serde(default)]
    pub collections: Vec<String>,
}

impl Game {
    pub fn owned(&self) -> bool {
        self.is_owned.unwrap_or(true)
    }

    pub fn status(&self) -> &'static str {
        if self.is_installed {
            if self.update_available {
                "update"
            } else {
                "installed"
            }
        } else {
            "not installed"
        }
    }

    pub fn playtime(&self) -> String {
        match self.playtime_forever_minutes {
            Some(0) | None => "-".to_string(),
            Some(minutes) => {
                let hours = minutes / 60;
                let mins = minutes % 60;
                if hours > 0 {
                    format!("{hours}h {mins:02}m")
                } else {
                    format!("{mins}m")
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Friend {
    #[serde(default)]
    pub steam_id: u64,
    #[serde(default)]
    pub persona_name: Option<String>,
    #[serde(default)]
    pub persona_state: Option<u32>,
    #[serde(default)]
    pub game_app_id: Option<u32>,
    #[serde(default)]
    pub game_name: Option<String>,
}

impl Friend {
    pub fn is_individual(&self) -> bool {
        // Individual SteamIDs start at 76561197960265728; clan/group IDs are
        // in the 103582791... range.
        self.steam_id >= 76_561_197_960_265_728 && self.steam_id < 103_582_791_429_521_408
    }

    pub fn name(&self) -> String {
        match &self.persona_name {
            Some(name) if !name.trim().is_empty() && name != &self.steam_id.to_string() => {
                name.clone()
            }
            Some(_) => "Unnamed profile".to_string(),
            None => "Unknown name".to_string(),
        }
    }

    pub fn online(&self) -> bool {
        self.persona_state.unwrap_or(0) != 0
    }

    pub fn state(&self) -> &'static str {
        persona_state_label(self.persona_state)
    }
}

pub fn persona_state_label(state: Option<u32>) -> &'static str {
    match state.unwrap_or(0) {
        1 => "online",
        2 => "busy",
        3 => "away",
        4 => "snooze",
        5 => "trade",
        6 => "play",
        _ => "offline",
    }
}

/// Human-friendly age for a Unix timestamp: days, then months, then years.
/// Used for account age and "last seen" instead of raw epoch values.
pub fn humanize_age(timestamp: u64, now: u64) -> String {
    if timestamp == 0 {
        return "unknown".to_string();
    }
    humanize_seconds(now.saturating_sub(timestamp))
}

pub fn humanize_ago(timestamp: u64, now: u64) -> String {
    if timestamp == 0 {
        return "unknown".to_string();
    }
    if timestamp > now {
        return format!("in {}", humanize_seconds(timestamp - now));
    }
    let seconds = now - timestamp;
    let age = humanize_seconds(seconds);
    if seconds < 86_400 {
        age
    } else {
        format!("{age} ago")
    }
}

fn humanize_seconds(seconds: u64) -> String {
    let days = seconds / 86_400;
    if days < 1 {
        "today".to_string()
    } else if days < 30 {
        plural(days, "day")
    } else if days < 365 {
        plural(days / 30, "month")
    } else {
        plural(days / 365, "year")
    }
}

fn plural(value: u64, unit: &str) -> String {
    if value == 1 {
        format!("1 {unit}")
    } else {
        format!("{value} {unit}s")
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ChatMessage {
    #[serde(default)]
    pub from_self: bool,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub timestamp: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct UserProfile {
    #[serde(default)]
    pub steam_id: u64,
    #[serde(default)]
    pub game_app_id: Option<u32>,
    #[serde(default)]
    pub persona_name: Option<String>,
    #[serde(default)]
    pub profile_url: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub is_public: bool,
    #[serde(default)]
    pub is_limited: bool,
    #[serde(default)]
    pub created: Option<u64>,
    #[serde(default)]
    pub real_name: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub headline: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub persona_state: Option<u32>,
    #[serde(default)]
    pub game_name: Option<String>,
    #[serde(default)]
    pub last_logoff: Option<u64>,
    #[serde(default)]
    pub ban_expires: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Account {
    #[serde(default)]
    pub steam_id: u64,
    #[serde(default)]
    pub account_name: String,
    #[serde(default)]
    pub persona_name: Option<String>,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub email_validated: bool,
    #[serde(default)]
    pub authed_machines: u32,
    #[serde(default)]
    pub vac_bans: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct StoreRow {
    #[serde(default)]
    pub app_id: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub is_free: bool,
    #[serde(default)]
    pub price: Option<String>,
    #[serde(default)]
    pub original_price: Option<String>,
    #[serde(default)]
    pub discount_pct: i32,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub reviews: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub priority: Option<u32>,
}

impl StoreRow {
    pub fn name(&self) -> String {
        self.name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| format!("app {}", self.app_id))
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MarketItem {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub market_hash_name: String,
    #[serde(default)]
    pub app_id: u32,
    #[serde(default)]
    pub app_name: Option<String>,
    #[serde(default)]
    pub sell_listings: u32,
    #[serde(default)]
    pub sell_price_text: Option<String>,
}

impl MarketItem {
    pub fn name(&self) -> String {
        self.name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| self.market_hash_name.clone())
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Listing {
    #[serde(default)]
    pub listing_id: String,
    #[serde(default)]
    pub market_hash_name: String,
    #[serde(default)]
    pub price: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct BuyOrder {
    #[serde(default)]
    pub buy_order_id: String,
    #[serde(default)]
    pub market_hash_name: String,
    #[serde(default)]
    pub price: u64,
    #[serde(default)]
    pub quantity: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MarketSearchPage {
    #[serde(default)]
    pub total_count: u32,
    #[serde(default)]
    pub start: u32,
    #[serde(default)]
    pub results: Vec<MarketItem>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MarketState {
    #[serde(default)]
    pub listings: Vec<Listing>,
    #[serde(default)]
    pub buy_orders: Vec<BuyOrder>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct InventoryItem {
    #[serde(default)]
    pub asset_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub market_hash_name: String,
    #[serde(default)]
    pub item_type: String,
    #[serde(default)]
    pub amount: u64,
    #[serde(default)]
    pub tradable: bool,
    #[serde(default)]
    pub marketable: bool,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub app_id: u32,
    #[serde(default)]
    pub context_id: u32,
    #[serde(default)]
    pub app_name: String,
    #[serde(default)]
    pub context_name: String,
}

impl InventoryItem {
    pub fn is_card(&self) -> bool {
        self.item_type.to_ascii_lowercase().contains("card")
    }

    /// Emoticons use a `:code:` as their internal name; prefer the descriptive
    /// market name when that happens.
    pub fn display_name(&self) -> String {
        let internal = self.name.starts_with(':') && self.name.ends_with(':');
        if self.name.trim().is_empty() || internal {
            self.market_hash_name.clone()
        } else {
            self.name.clone()
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProtonRuntime {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub location: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProtonList {
    #[serde(default)]
    pub default: String,
    #[serde(default)]
    pub installed: Vec<ProtonRuntime>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct GameConfig {
    #[serde(default)]
    pub forced_proton_version: Option<String>,
    #[serde(default)]
    pub platform_preference: Option<String>,
    #[serde(default)]
    pub runner: String,
    #[serde(default)]
    pub launch_script: Option<String>,
    #[serde(default)]
    pub steam_runtime_policy: String,
    #[serde(default)]
    pub steam_prefix_mode: String,
    #[serde(default)]
    pub steam_launch_options: Option<String>,
    #[serde(default)]
    pub steam_compat_tool: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Branches {
    #[serde(default)]
    pub branches: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct InstallRow {
    #[serde(default)]
    pub app_id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub downloaded_bytes: u64,
    #[serde(default)]
    pub total_bytes: u64,
    #[serde(default)]
    pub percent: f64,
    #[serde(default)]
    pub status: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Achievement {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub unlocked: bool,
    #[serde(default)]
    pub rarity: f64,
    #[serde(default)]
    pub date_unlocked: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Achievements {
    #[serde(default)]
    pub app_id: u32,
    #[serde(default)]
    pub unlocked: u32,
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub achievements: Vec<Achievement>,
}

/// A parsed `{"event":"progress", ...}` NDJSON line from Aurelia (stderr).
#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub state: String,
    pub percent: f64,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub speed_bps: f64,
    pub eta_seconds: Option<u64>,
    pub file: String,
}

impl Progress {
    pub fn from_value(value: &Value) -> Option<Self> {
        if value.get("event").and_then(Value::as_str)? != "progress" {
            return None;
        }
        Some(Self {
            state: value
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("working")
                .to_string(),
            percent: value.get("percent").and_then(Value::as_f64).unwrap_or(0.0),
            bytes_downloaded: value
                .get("bytes_downloaded")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            total_bytes: value
                .get("total_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            speed_bps: value
                .get("speed_bps")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            eta_seconds: value.get("eta_seconds").and_then(Value::as_u64),
            file: value
                .get("file")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
    }
}

pub fn deserialize<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).context("unexpected JSON from Aurelia")
}

pub fn games(value: Value) -> Result<Vec<Game>> {
    deserialize(value)
}

pub fn friends(value: Value) -> Result<Vec<Friend>> {
    deserialize(value)
}

pub fn chat_history(value: Value) -> Result<Vec<ChatMessage>> {
    deserialize(value)
}

/// Pull the row array out of `search`/`deals` (`results`) or `wishlist` (`items`).
pub fn store_rows(value: Value, key: &str) -> Result<Vec<StoreRow>> {
    let rows = value.get(key).cloned().unwrap_or(Value::Array(Vec::new()));
    deserialize(rows)
}

pub fn install_rows(value: Value) -> Result<Vec<InstallRow>> {
    deserialize(value)
}

pub fn inventory_items(value: Value) -> Result<Vec<InventoryItem>> {
    deserialize(value)
}

pub fn vstr(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn vf64(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

pub fn vbool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn vlist(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| match item {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(_) => vstr(item, "name"),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn human_speed(bps: f64) -> String {
    if bps <= 0.0 {
        "-".to_string()
    } else {
        format!("{}/s", human_bytes(bps.round() as u64))
    }
}

pub fn human_eta(seconds: Option<u64>) -> String {
    match seconds {
        Some(0) | None => "-".to_string(),
        Some(s) if s < 60 => format!("{s}s"),
        Some(s) if s < 3600 => format!("{}m {:02}s", s / 60, s % 60),
        Some(s) => format!("{}h {:02}m", s / 3600, (s % 3600) / 60),
    }
}

pub fn hhmm(timestamp: u64) -> String {
    if timestamp == 0 {
        return "--:--".to_string();
    }
    let secs = timestamp % 86_400;
    format!("{:02}:{:02}", secs / 3600, (secs % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_library_rows_with_assets() {
        let value = serde_json::json!([{
            "app_id": 730,
            "name": "Counter-Strike 2",
            "is_installed": true,
            "update_available": false,
            "playtime_forever_minutes": 125,
            "is_owned": true,
            "assets": { "header": "https://example/header.jpg" },
            "collections": ["Favorites"]
        }]);
        let games = games(value).unwrap();
        assert_eq!(games[0].app_id, 730);
        assert_eq!(games[0].playtime(), "2h 05m");
        assert_eq!(games[0].status(), "installed");
        assert_eq!(
            games[0].assets.header.as_deref(),
            Some("https://example/header.jpg")
        );
    }

    #[test]
    fn parses_store_rows_and_null_names() {
        let value = serde_json::json!({
            "results": [
                {"app_id": 10, "name": "Game", "price": "$9.99", "discount_pct": 25},
                {"app_id": 11, "name": null}
            ]
        });
        let rows = store_rows(value, "results").unwrap();
        assert_eq!(rows[0].name(), "Game");
        assert_eq!(rows[1].name(), "app 11");
        assert_eq!(rows[0].discount_pct, 25);
    }

    #[test]
    fn parses_progress_event() {
        let value = serde_json::json!({
            "event": "progress",
            "state": "downloading",
            "percent": 42.5,
            "bytes_downloaded": 1024,
            "total_bytes": 4096,
            "speed_bps": 2048.0,
            "eta_seconds": 3,
            "file": "pak0.pak"
        });
        let progress = Progress::from_value(&value).unwrap();
        assert_eq!(progress.state, "downloading");
        assert_eq!(progress.percent, 42.5);
        assert_eq!(human_eta(progress.eta_seconds), "3s");
    }

    #[test]
    fn humanizes_account_ages() {
        let now = 1_000_000_000;
        assert_eq!(humanize_age(now - 5 * 86_400, now), "5 days");
        assert_eq!(humanize_age(now - 35 * 86_400, now), "1 month");
        assert_eq!(humanize_age(now - 95 * 86_400, now), "3 months");
        assert_eq!(humanize_age(now - 800 * 86_400, now), "2 years");
        assert_eq!(humanize_age(now - 3_600, now), "today");
        assert_eq!(humanize_ago(now - 3_600, now), "today");
        assert_eq!(humanize_ago(now + 40 * 86_400, now), "in 1 month");
    }

    #[test]
    fn non_progress_event_is_ignored() {
        assert!(Progress::from_value(&serde_json::json!({"event": "result"})).is_none());
    }
}
