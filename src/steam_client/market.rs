//! Steam Community Market, inventory, and wallet — read-only (Phase 1).
//!
//! These are **web** endpoints (`steamcommunity.com`), so they need a logged-in web
//! session rather than the CM connection. [`SteamClient::web_session`] mints that
//! session (Phase 0): it exchanges the CM session's refresh token for a short-lived
//! **web access token** via `Authentication.GenerateAccessTokenForApp`, then builds
//! the `steamLoginSecure`/`sessionid` cookies (see [`crate::web::web_session`]).
//!
//! Public lookups (item price, market search) need no session and are free functions;
//! account-scoped reads (inventory, wallet, my listings) are `SteamClient` methods.
use super::*;
use crate::web::web_session::{ECON_IMAGE_BASE, WebSession};
use serde_json::Value;
use std::sync::LazyLock;
use steam_vent_proto::steammessages_auth_steamclient::{
    CAuthentication_AccessToken_GenerateForApp_Request,
    CAuthentication_AccessToken_GenerateForApp_Response,
};

/// One item in an account inventory.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InventoryItem {
    /// Unique asset id (per item instance).
    pub asset_id: String,
    /// Economy class id (shared by identical items).
    pub class_id: String,
    /// Display name.
    pub name: String,
    /// Canonical market name used by the market endpoints.
    pub market_hash_name: String,
    /// Item type/category text (e.g. "Trading Card").
    pub item_type: String,
    /// Stack size for this asset.
    pub amount: u64,
    /// Whether the item can be traded.
    pub tradable: bool,
    /// Whether the item can be sold on the Community Market.
    pub marketable: bool,
    /// Full icon URL, if the item has one.
    pub icon_url: Option<String>,
    /// App this inventory belongs to.
    pub app_id: u32,
    /// Inventory context inside the app.
    pub context_id: u32,
    /// Human-readable game/app name.
    pub app_name: String,
    /// Human-readable context name.
    pub context_name: String,
}

/// One inventory context discovered on the account's inventory page.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InventoryContext {
    pub app_id: u32,
    pub context_id: u32,
    pub app_name: String,
    pub context_name: String,
    pub asset_count: u32,
}

/// Market price summary for an item (`priceoverview`). Prices are Steam-formatted
/// currency strings.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MarketPrice {
    pub market_hash_name: String,
    pub lowest_price: Option<String>,
    pub median_price: Option<String>,
    /// Number of items sold in the last 24h.
    pub volume: Option<String>,
}

/// One result from a market search.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MarketSearchResult {
    pub name: String,
    pub market_hash_name: String,
    pub app_id: u32,
    pub app_name: String,
    /// Number of active sell listings.
    pub sell_listings: u32,
    /// Lowest sell price, in the currency's minor units (cents).
    pub sell_price: u64,
    /// Pre-formatted lowest sell price.
    pub sell_price_text: String,
}

/// One of the account's active sell listings.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MyListing {
    pub listing_id: String,
    pub market_hash_name: String,
    /// Listed price in the currency's minor units (cents), as reported by Steam.
    pub price: u64,
}

/// One of the account's open buy orders.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MyBuyOrder {
    pub buy_order_id: String,
    pub market_hash_name: String,
    /// Per-item price in the currency's minor units (cents).
    pub price: u64,
    /// Quantity still wanted.
    pub quantity: u64,
}

/// The account's market listings and open buy orders.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MyMarketState {
    pub listings: Vec<MyListing>,
    pub buy_orders: Vec<MyBuyOrder>,
}

/// The account's Steam Wallet balance.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WalletBalance {
    /// Balance in the currency's minor units (cents).
    pub balance_cents: i64,
    /// Steam currency id.
    pub currency: u32,
    /// Wallet country code.
    pub country: String,
    /// Human-readable balance (e.g. "12.34 EUR").
    pub formatted: String,
}

impl SteamClient {
    /// **Phase 0:** mint a Steam web session from the current CM session.
    ///
    /// Exchanges the session's refresh token (held by the live connection as its
    /// "access token") for a fresh, web-scoped access token via
    /// `Authentication.GenerateAccessTokenForApp`, then builds the cookie set. This
    /// reuses the one shared connection — no extra logon.
    pub async fn web_session(&self) -> Result<WebSession> {
        let connection = self.require_connection()?;
        let steam_id = u64::from(connection.steam_id());
        let refresh_token = connection
            .access_token()
            .context("no Steam token available for a web session; run `aurelia login`")?
            .to_string();

        let mut req = CAuthentication_AccessToken_GenerateForApp_Request::new();
        req.set_refresh_token(refresh_token);
        req.set_steamid(steam_id);

        let resp: CAuthentication_AccessToken_GenerateForApp_Response = connection
            .service_method(req)
            .await
            .map_err(|e| anyhow!("failed to mint a web access token: {e}"))?;
        let token = resp.access_token();
        if token.is_empty() {
            bail!("Steam returned an empty web access token");
        }
        WebSession::new(steam_id, token, connection.ip_country_code().as_deref())
    }
}

// The web-surface fetchers take any [`WebSession`] — minted off the CM session
// (methods above) or built from a pasted browser token (`login --web-token`) —
// so these features work without a client login.

/// List an account's inventory for an app over an existing web session.
pub async fn inventory_via(
    web: &WebSession,
    app_id: u32,
    context_id: u32,
) -> Result<Vec<InventoryItem>> {
    let app_name = format!("App {app_id}");
    inventory_via_named(web, app_id, context_id, &app_name, "").await
}

/// Like [`inventory_via`], but carries the app/context labels into each item so
/// callers can group a combined inventory by game.
pub async fn inventory_via_named(
    web: &WebSession,
    app_id: u32,
    context_id: u32,
    app_name: &str,
    context_name: &str,
) -> Result<Vec<InventoryItem>> {
    let steam_id = web.steam_id();
    let url = format!(
        "https://steamcommunity.com/inventory/{steam_id}/{app_id}/{context_id}?l=english&count=2000"
    );
    let mut items = parse_inventory(&web.get_text(&url).await?)?;
    for item in &mut items {
        item.app_id = app_id;
        item.context_id = context_id;
        item.app_name = app_name.to_string();
        item.context_name = context_name.to_string();
    }
    Ok(items)
}

/// Discover every inventory app/context from the account's inventory page, then
/// fetch and combine them. Trading cards are sorted first; the rest are grouped
/// by game and name.
pub async fn inventory_all_via(web: &WebSession) -> Result<Vec<InventoryItem>> {
    let url = format!(
        "https://steamcommunity.com/profiles/{}/inventory/",
        web.steam_id()
    );
    // Prefer a public fetch: an account without market access can get a
    // restriction page when the authenticated cookies are attached, while the
    // inventory page itself is still public.
    let html = match public_text(&url).await {
        Ok(html) => html,
        Err(_) => web.get_text(&url).await?,
    };
    let contexts = parse_inventory_contexts(&html)?;
    let mut items = Vec::new();
    for context in contexts {
        if context.asset_count == 0 {
            continue;
        }
        match inventory_via_named(
            web,
            context.app_id,
            context.context_id,
            &context.app_name,
            &context.context_name,
        )
        .await
        {
            Ok(mut page) => items.append(&mut page),
            Err(error) => {
                tracing::warn!(
                    app_id = context.app_id,
                    context_id = context.context_id,
                    "could not load inventory context: {error:#}"
                );
            }
        }
    }
    sort_inventory(&mut items);
    Ok(items)
}

pub fn sort_inventory(items: &mut [InventoryItem]) {
    items.sort_by(|a, b| {
        let (a_card, b_card) = (is_card(a), is_card(b));
        a_card
            .cmp(&b_card)
            .reverse()
            .then_with(|| {
                if a_card && b_card {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                } else {
                    a.app_name
                        .to_lowercase()
                        .cmp(&b.app_name.to_lowercase())
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                }
            })
            .then_with(|| a.name.cmp(&b.name))
    });
}

fn is_card(item: &InventoryItem) -> bool {
    item.item_type.to_ascii_lowercase().contains("card")
}

/// Extract `g_rgAppContextData` from the inventory page HTML.
fn parse_inventory_contexts(html: &str) -> Result<Vec<InventoryContext>> {
    let marker = "g_rgAppContextData = ";
    let start = html
        .find(marker)
        .context("inventory page did not contain app/context metadata")?
        + marker.len();
    let rest = &html[start..];
    let end =
        json_object_end(rest).context("inventory page contained malformed app/context metadata")?;
    let value: Value = serde_json::from_str(&rest[..end])
        .context("could not parse inventory app/context metadata")?;
    let mut contexts = Vec::new();
    if let Some(apps) = value.as_object() {
        for app in apps.values() {
            let app_id = app.get("appid").and_then(as_u64_loose).unwrap_or(0) as u32;
            if app_id == 0 {
                continue;
            }
            let app_name = app
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unknown game")
                .to_string();
            let load_failed = app.get("load_failed").and_then(as_u64_loose).unwrap_or(0) != 0;
            if load_failed {
                continue;
            }
            if let Some(rg_contexts) = app.get("rgContexts").and_then(Value::as_object) {
                for context in rg_contexts.values() {
                    let context_id = context.get("id").and_then(as_u64_loose).unwrap_or(0) as u32;
                    let asset_count = context
                        .get("asset_count")
                        .and_then(as_u64_loose)
                        .unwrap_or(0) as u32;
                    if context_id == 0 || asset_count == 0 {
                        continue;
                    }
                    contexts.push(InventoryContext {
                        app_id,
                        context_id,
                        app_name: app_name.clone(),
                        context_name: context
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        asset_count,
                    });
                }
            }
        }
    }
    contexts.sort_by(|a, b| {
        a.app_name
            .to_lowercase()
            .cmp(&b.app_name.to_lowercase())
            .then_with(|| a.context_id.cmp(&b.context_id))
    });
    Ok(contexts)
}

/// Find the end of a JSON object at the start of `text`, ignoring braces in strings.
fn json_object_end(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

/// Fetch the account's active market listings and open buy orders over an
/// existing web session.
pub async fn my_listings_via(web: &WebSession) -> Result<MyMarketState> {
    let body = web
        .get_text("https://steamcommunity.com/market/mylistings/?norender=1&count=100")
        .await?;
    parse_my_listings(&body)
}

/// Fetch the account's Steam Wallet balance over an existing web session.
pub async fn wallet_via(web: &WebSession) -> Result<WalletBalance> {
    let mut web = web.clone();
    let body = web
        .get_market_text("https://steamcommunity.com/market/")
        .await?;
    parse_wallet(&body)
}

/// A `reqwest` client for public (no-auth) market lookups.
fn public_client() -> Result<reqwest::Client> {
    crate::core::net::steam_web_client()
}

async fn public_text(url: &str) -> Result<String> {
    let client = public_client()?;
    let request = client.get(url);
    let response = crate::core::net::send_with_retry(&client, request).await?;
    Ok(response.text().await?)
}

/// Look up an item's market price (`priceoverview`). Public — no session required.
/// `currency` is the Steam currency id (1 = USD, 2 = GBP, 3 = EUR, …).
pub async fn market_price(
    app_id: u32,
    market_hash_name: &str,
    currency: u32,
) -> Result<MarketPrice> {
    let client = public_client()?;
    let request = client
        .get("https://steamcommunity.com/market/priceoverview/")
        .query(&[
            ("appid", app_id.to_string()),
            ("currency", currency.to_string()),
            ("market_hash_name", market_hash_name.to_string()),
        ]);
    let resp = crate::core::net::send_with_retry(&client, request)
        .await
        .context("market price request failed")?;
    let v: Value = resp
        .json()
        .await
        .context("invalid price-overview response")?;
    if v.get("success").and_then(Value::as_bool) != Some(true) {
        bail!(
            "no market data for '{market_hash_name}' (it may not be marketable, or the name/app id \
             is wrong — names are case-sensitive and exact)"
        );
    }
    Ok(MarketPrice {
        market_hash_name: market_hash_name.to_string(),
        lowest_price: v
            .get("lowest_price")
            .and_then(Value::as_str)
            .map(String::from),
        median_price: v
            .get("median_price")
            .and_then(Value::as_str)
            .map(String::from),
        volume: v.get("volume").and_then(Value::as_str).map(String::from),
    })
}

/// Search the Community Market. Public — no session required. Returns
/// `(total_count, page_of_results)`.
pub async fn market_search(
    query: Option<&str>,
    app_id: Option<u32>,
    start: u32,
    count: u32,
) -> Result<(u32, Vec<MarketSearchResult>)> {
    let mut params: Vec<(&str, String)> = vec![
        ("norender", "1".to_string()),
        ("start", start.to_string()),
        ("count", count.to_string()),
    ];
    if let Some(q) = query {
        params.push(("query", q.to_string()));
    }
    if let Some(a) = app_id {
        params.push(("appid", a.to_string()));
    }

    let client = public_client()?;
    let request = client
        .get("https://steamcommunity.com/market/search/render/")
        .query(&params);
    let resp = crate::core::net::send_with_retry(&client, request)
        .await
        .context("market search request failed")?;
    let v: Value = resp
        .json()
        .await
        .context("invalid market-search response")?;
    let total = v.get("total_count").and_then(Value::as_u64).unwrap_or(0) as u32;
    let results = parse_json_array(&v, "results", parse_search_result);
    Ok((total, results))
}

/// Read `key` as a JSON array and map each element through `f`, dropping `None`s.
/// A missing key or non-array value yields an empty `Vec`.
fn parse_json_array<T>(v: &Value, key: &str, f: impl FnMut(&Value) -> Option<T>) -> Vec<T> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(f).collect())
        .unwrap_or_default()
}

fn parse_search_result(v: &Value) -> Option<MarketSearchResult> {
    Some(MarketSearchResult {
        name: v.get("name")?.as_str()?.to_string(),
        market_hash_name: v
            .get("hash_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        app_id: v
            .get("asset_description")
            .and_then(|a| a.get("appid"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        app_name: v
            .get("app_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        sell_listings: v.get("sell_listings").and_then(Value::as_u64).unwrap_or(0) as u32,
        sell_price: v.get("sell_price").and_then(Value::as_u64).unwrap_or(0),
        sell_price_text: v
            .get("sell_price_text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

/// Coerce a JSON value that may be a number or a numeric string into i64.
fn as_i64_loose(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// Coerce a JSON value that may be a number or numeric string into u64.
fn as_u64_loose(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn parse_inventory(body: &str) -> Result<Vec<InventoryItem>> {
    let v: Value = serde_json::from_str(body).context(
        "could not parse the inventory response (the web session may be invalid — try \
         `aurelia login --reconnect`)",
    )?;
    // An explicit failure (e.g. private inventory) comes back as success:0 + error.
    if v.get("success").and_then(Value::as_i64) == Some(0) {
        let err = v
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("inventory unavailable");
        bail!("{err}");
    }
    let Some(assets) = v.get("assets").and_then(Value::as_array) else {
        return Ok(Vec::new()); // empty inventory
    };

    // Index descriptions by (classid, instanceid) so each asset can be enriched.
    let mut descriptions: HashMap<(String, String), &Value> = HashMap::new();
    if let Some(arr) = v.get("descriptions").and_then(Value::as_array) {
        for d in arr {
            let cid = d
                .get("classid")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let iid = d
                .get("instanceid")
                .and_then(Value::as_str)
                .unwrap_or("0")
                .to_string();
            descriptions.insert((cid, iid), d);
        }
    }

    let mut items = Vec::with_capacity(assets.len());
    for a in assets {
        let class_id = a
            .get("classid")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let instance_id = a
            .get("instanceid")
            .and_then(Value::as_str)
            .unwrap_or("0")
            .to_string();
        let asset_id = a
            .get("assetid")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let amount = a.get("amount").and_then(as_u64_loose).unwrap_or(1);

        let desc = descriptions.get(&(class_id.clone(), instance_id));
        let get = |k: &str| {
            desc.and_then(|d| d.get(k).and_then(Value::as_str))
                .unwrap_or_default()
        };
        let flag = |k: &str| {
            desc.and_then(|d| d.get(k).and_then(Value::as_i64))
                .unwrap_or(0)
                == 1
        };
        let icon = desc
            .and_then(|d| d.get("icon_url").and_then(Value::as_str))
            .filter(|s| !s.is_empty())
            .map(|s| format!("{ECON_IMAGE_BASE}{s}"));

        items.push(InventoryItem {
            asset_id,
            class_id,
            name: get("name").to_string(),
            market_hash_name: get("market_hash_name").to_string(),
            item_type: get("type").to_string(),
            amount,
            tradable: flag("tradable"),
            marketable: flag("marketable"),
            icon_url: icon,
            app_id: 0,
            context_id: 0,
            app_name: String::new(),
            context_name: String::new(),
        });
    }
    Ok(items)
}

fn parse_my_listings(body: &str) -> Result<MyMarketState> {
    let v: Value = serde_json::from_str(body).context(
        "could not parse your market listings (the web session may be invalid — try \
         `aurelia login --reconnect`)",
    )?;
    // `success:false` means the request wasn't authenticated (vs. an authed account
    // that simply has no listings, which returns `success:true` + empty arrays).
    if matches!(v.get("success").and_then(Value::as_bool), Some(false))
        || matches!(v.get("success").and_then(Value::as_i64), Some(0))
    {
        bail!(
            "the market session was rejected (no authenticated response). Run \
             `aurelia login --reconnect`."
        );
    }
    let listings = parse_json_array(&v, "listings", parse_my_listing);
    let buy_orders = parse_json_array(&v, "buy_orders", parse_my_buy_order);
    Ok(MyMarketState {
        listings,
        buy_orders,
    })
}

fn parse_my_listing(v: &Value) -> Option<MyListing> {
    let listing_id = v.get("listingid")?.as_str()?.to_string();
    let asset = v.get("asset");
    let name = asset
        .and_then(|a| {
            a.get("market_hash_name")
                .or_else(|| a.get("name"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default()
        .to_string();
    let price = v.get("price").and_then(as_u64_loose).unwrap_or(0);
    Some(MyListing {
        listing_id,
        market_hash_name: name,
        price,
    })
}

fn parse_my_buy_order(v: &Value) -> Option<MyBuyOrder> {
    let buy_order_id = v.get("buy_orderid")?.as_str()?.to_string();
    Some(MyBuyOrder {
        buy_order_id,
        market_hash_name: v
            .get("hash_name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        price: v.get("price").and_then(as_u64_loose).unwrap_or(0),
        quantity: v.get("quantity").and_then(as_u64_loose).unwrap_or(0),
    })
}

fn parse_wallet(html: &str) -> Result<WalletBalance> {
    // The market page embeds `g_rgWalletInfo = {…};` as an inline script var.
    static RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"g_rgWalletInfo\s*=\s*(\{.*?\})\s*;").unwrap());
    let caps = RE.captures(html).context(
        "could not read your wallet (no `g_rgWalletInfo` on the market page). Make sure you are \
         logged in and the account is market-eligible.",
    )?;
    let json: Value = serde_json::from_str(&caps[1]).context("failed to parse wallet info")?;

    let currency = json
        .get("wallet_currency")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    let country = json
        .get("wallet_country")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let balance_cents = json
        .get("wallet_balance")
        .and_then(as_i64_loose)
        .unwrap_or(0);

    Ok(WalletBalance {
        formatted: format_currency(balance_cents, currency),
        balance_cents,
        currency,
        country,
    })
}

/// Format a minor-unit (cents) amount with the currency's code. Falls back to the
/// numeric currency id for unmapped currencies.
pub fn format_currency(cents: i64, currency: u32) -> String {
    let code = currency_code(currency);
    format!("{:.2} {code}", cents as f64 / 100.0)
}

/// Map a Steam currency id to its ISO-ish code (common subset).
fn currency_code(currency: u32) -> String {
    match currency {
        1 => "USD",
        2 => "GBP",
        3 => "EUR",
        4 => "CHF",
        5 => "RUB",
        6 => "PLN",
        7 => "BRL",
        8 => "JPY",
        9 => "NOK",
        13 => "SGD",
        16 => "KRW",
        17 => "TRY",
        18 => "UAH",
        19 => "MXN",
        20 => "CAD",
        21 => "AUD",
        22 => "NZD",
        23 => "CNY",
        24 => "INR",
        28 => "ZAR",
        29 => "HKD",
        _ => return format!("(currency {currency})"),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, app_name: &str, item_type: &str) -> InventoryItem {
        InventoryItem {
            asset_id: String::new(),
            class_id: String::new(),
            name: name.to_string(),
            market_hash_name: name.to_string(),
            item_type: item_type.to_string(),
            amount: 1,
            tradable: true,
            marketable: true,
            icon_url: None,
            app_id: 0,
            context_id: 0,
            app_name: app_name.to_string(),
            context_name: String::new(),
        }
    }

    #[test]
    fn parses_inventory_contexts_from_page() {
        let html = r#"<script>var g_rgAppContextData = {"753":{"appid":753,"name":"Steam","load_failed":0,"rgContexts":{"6":{"id":"6","name":"Community","asset_count":160}}},"730":{"appid":730,"name":"Counter-Strike 2","load_failed":0,"rgContexts":{"2":{"id":"2","name":"Inventory","asset_count":4}}},"99900":{"appid":99900,"name":"Broken","load_failed":1,"rgContexts":[]}};</script>"#;
        let contexts = parse_inventory_contexts(html).unwrap();
        assert_eq!(contexts.len(), 2);
        assert_eq!(contexts[0].app_name, "Counter-Strike 2");
        assert_eq!(contexts[0].asset_count, 4);
        assert_eq!(contexts[1].app_name, "Steam");
    }

    #[test]
    fn sorts_cards_first_then_by_game_and_name() {
        let mut items = vec![
            item("Zeta", "Game B", "Emoticon"),
            item("Alpha Card", "Steam", "Trading Card"),
            item("Alpha", "Game A", "Emoticon"),
            item("Beta", "Game A", "Emoticon"),
            item("Beta Card", "Steam", "Foil Trading Card"),
        ];
        sort_inventory(&mut items);
        let names: Vec<&str> = items.iter().map(|item| item.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Alpha Card", "Beta Card", "Alpha", "Beta", "Zeta"]
        );
    }

    #[test]
    fn finds_json_object_end_inside_html() {
        let text = r#"{"a":"}","b":{"c":1}}; trailing"#;
        assert_eq!(json_object_end(text), Some(text.find("};").unwrap() + 1));
    }
}
