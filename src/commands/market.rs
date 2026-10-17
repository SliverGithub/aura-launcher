//! `market` command handlers.

use crate::commands::common::*;

use anyhow::Result;

/// `aurelia inventory [appid] [--all]`: list one game's inventory, or every
/// inventory found on the account's inventory page.
pub(crate) async fn cmd_inventory(
    app_id: Option<u32>,
    context: u32,
    all: bool,
    json: bool,
) -> Result<()> {
    let web = web_access().await?;
    if all || app_id.is_none() {
        let items = aurelia::steam_client::inventory_all_via(&web).await?;
        if json {
            print_json(&items);
            return Ok(());
        }
        print_inventory(&items, true);
        return Ok(());
    }
    let app_id = app_id.expect("checked above");
    let items = aurelia::steam_client::inventory_via(&web, app_id, context).await?;
    if json {
        print_json(&items);
        return Ok(());
    }
    if items.is_empty() {
        cli_println!("No items in your inventory for app {app_id} (context {context}).");
        return Ok(());
    }
    print_inventory(&items, false);
    Ok(())
}

fn print_inventory(items: &[aurelia::steam_client::InventoryItem], grouped: bool) {
    if items.is_empty() {
        cli_println!("No inventory items found.");
        return;
    }
    if grouped {
        let mut current = "";
        for item in items {
            if item.app_name != current {
                current = &item.app_name;
                let suffix = if item.context_name.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", item.context_name)
                };
                cli_println!("\n{} {current}{suffix}", "──");
            }
            cli_println!(
                "{:>6}  {:<5}  {:<5}  {}",
                item.amount,
                yesno(item.tradable),
                yesno(item.marketable),
                item.name
            );
        }
    } else {
        cli_println!("{:>6}  {:<5}  {:<5}  NAME", "AMOUNT", "TRADE", "MKT");
        for it in items {
            cli_println!(
                "{:>6}  {:<5}  {:<5}  {}",
                it.amount,
                yesno(it.tradable),
                yesno(it.marketable),
                it.name
            );
        }
    }
    cli_println!("\n{} item stack(s).", items.len());
}

/// `aurelia wallet`: show the account's Steam Wallet balance.
pub(crate) async fn cmd_wallet(json: bool) -> Result<()> {
    let web = web_access().await?;
    let w = aurelia::steam_client::wallet_via(&web).await?;
    if json {
        print_json(&serde_json::json!({
            "balance_cents": w.balance_cents,
            "currency": w.currency,
            "country": w.country,
            "formatted": w.formatted,
        }));
    } else {
        cli_println!("Wallet : {}", w.formatted);
        if !w.country.is_empty() {
            cli_println!("Country: {}", w.country);
        }
    }
    Ok(())
}

/// `aurelia market price <appid> <name>`: look up an item's price (no login needed).
pub(crate) async fn cmd_market_price(
    app_id: u32,
    name: String,
    currency: u32,
    json: bool,
) -> Result<()> {
    let price = aurelia::steam_client::market_price(app_id, &name, currency).await?;
    if json {
        print_json(&serde_json::json!({
            "market_hash_name": price.market_hash_name,
            "lowest_price": price.lowest_price,
            "median_price": price.median_price,
            "volume": price.volume,
        }));
    } else {
        cli_println!("Item   : {}", price.market_hash_name);
        cli_println!("Lowest : {}", price.lowest_price.as_deref().unwrap_or("—"));
        cli_println!("Median : {}", price.median_price.as_deref().unwrap_or("—"));
        cli_println!(
            "Volume : {} sold in 24h",
            price.volume.as_deref().unwrap_or("0")
        );
    }
    Ok(())
}

/// `aurelia market search [query]`: search the Community Market (no login needed).
pub(crate) async fn cmd_market_search(
    query: Option<String>,
    app_id: Option<u32>,
    count: u32,
    start: u32,
    json: bool,
) -> Result<()> {
    let (total, results) =
        aurelia::steam_client::market_search(query.as_deref(), app_id, start, count).await?;
    if json {
        print_json(&serde_json::json!({
            "total_count": total,
            "start": start,
            "results": results,
        }));
        return Ok(());
    }
    if results.is_empty() {
        cli_println!("No market results.");
        return Ok(());
    }
    cli_println!("{:>12}  {:>5}  NAME", "PRICE", "LIST");
    for r in &results {
        cli_println!(
            "{:>12}  {:>5}  {} [{}]",
            r.sell_price_text,
            r.sell_listings,
            r.name,
            r.app_name
        );
    }
    cli_println!("\nShowing {} of {} result(s).", results.len(), total);
    Ok(())
}

/// `aurelia market listings`: the account's active listings and open buy orders.
pub(crate) async fn cmd_market_listings(json: bool) -> Result<()> {
    let web = web_access().await?;
    let state = aurelia::steam_client::my_listings_via(&web).await?;
    if json {
        print_json(&state);
        return Ok(());
    }
    if state.listings.is_empty() && state.buy_orders.is_empty() {
        cli_println!("No active listings or open buy orders.");
        return Ok(());
    }
    if !state.listings.is_empty() {
        cli_println!("Active listings:");
        for l in &state.listings {
            cli_println!(
                "  [{}] {} — {} (minor units)",
                l.listing_id,
                l.market_hash_name,
                l.price
            );
        }
    }
    if !state.buy_orders.is_empty() {
        cli_println!("Buy orders:");
        for b in &state.buy_orders {
            cli_println!(
                "  [{}] {} x{} @ {} (minor units)",
                b.buy_order_id,
                b.market_hash_name,
                b.quantity,
                b.price
            );
        }
    }
    Ok(())
}
