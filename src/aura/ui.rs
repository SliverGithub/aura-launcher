//! Rendering for every screen.

use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};
use ratatui_image::{FilterType, Resize, StatefulImage};

use super::app::{App, Chat, Manage, ManageMode, MarketRow, MarketView, Screen};
use super::markdown;
use super::model::{self, Friend, Game, UserProfile};
use serde_json::Value;

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const OK: Color = Color::Green;
const WARN: Color = Color::Yellow;
const BAD: Color = Color::Red;
const TEXT: Color = Color::Gray;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(6),
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);
    draw_body(frame, app, chunks[1]);
    draw_footer(frame, app, chunks[2]);

    if let Some(manage) = &app.manage {
        draw_manage(frame, app, manage, area);
    }
    if app.help {
        draw_help(frame, area);
    }
    if let Some(confirm) = &app.confirm {
        draw_confirm(frame, area, &confirm.prompt);
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = Vec::new();
    for (index, screen) in Screen::ALL.iter().enumerate() {
        let active = *screen == app.screen;
        spans.push(Span::styled(
            format!(" {} {} ", index + 1, screen.title()),
            if active {
                Style::default()
                    .fg(Color::Black)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            },
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled("│ ", Style::default().fg(DIM)));
    spans.push(Span::styled(app.session_label(), Style::default().fg(OK)));
    if let Some(label) = app.pending_label() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(format!("{label}…"), Style::default().fg(WARN)));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(
            " Aura Launcher · terminal Steam client ",
            Style::default().fg(ACCENT),
        ));
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let keys = match app.screen {
        Screen::Library => {
            "Enter play/install · i install · u update · v verify · x uninstall · S stop · c cloud · m manage · g store · a achievements · w workshop · d DLC · s store search · f installed · / filter · r reload · ? help · q quit"
        }
        Screen::Store => {
            "d deals · v wishlist · / search · Enter store page · i install · f add free license · a wishlist · x remove · o browser · r reload · ? help · q quit"
        }
        Screen::Community => {
            "Enter chat · i type · Esc close chat · v profile · m my profile · a add friend · x remove friend · o open profile · r reload · / filter · l login · ? help · q quit"
        }
        Screen::Market => {
            "v listings · / search · i all inventory · I one game · w wallet · Enter price · o open · r reload · ? help"
        }
        Screen::Downloads => {
            "x stop selected · c clear finished · g go to game · r refresh installs · 1-6 tabs · ? help · q quit"
        }
        Screen::Account => {
            "l login · L logout · R reconnect session · r refresh · c config · o profile · ? help · q quit"
        }
    };
    let mut lines = vec![Line::from(Span::styled(keys, Style::default().fg(DIM)))];
    if let Some(error) = &app.error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(BAD),
        )));
    } else if let Some(notice) = &app.notice {
        lines.push(Line::from(Span::styled(
            notice.clone(),
            Style::default().fg(OK),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Steam CM, CDN, Proton, Cloud, Workshop and community services via Aurelia.",
            Style::default().fg(DIM),
        )));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM));
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_body(frame: &mut Frame, app: &App, area: Rect) {
    match app.screen {
        Screen::Library => draw_library(frame, app, area),
        Screen::Store => draw_store(frame, app, area),
        Screen::Community => draw_community(frame, app, area),
        Screen::Market => draw_market(frame, app, area),
        Screen::Downloads => draw_downloads(frame, app, area),
        Screen::Account => draw_account(frame, app, area),
    }
}

// ------------------------------------------------------------------- library

fn draw_library(frame: &mut Frame, app: &App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);
    let games = app.filtered_games();
    let mut items = Vec::new();
    for game in &games {
        items.push(ListItem::new(game_line(game)));
    }
    let filter = if app.filter.is_empty() {
        String::new()
    } else {
        format!(" · filter: {}", app.filter)
    };
    let installed = if app.only_installed {
        " · installed"
    } else {
        ""
    };
    let title = format!(
        " Library {}/{}{}{} ",
        games.len(),
        app.games.len(),
        installed,
        filter
    );
    render_list(
        frame,
        panes[0],
        items,
        app.game_sel,
        title,
        &app.library_list,
    );
    draw_game_details(frame, app, panes[1], games.get(app.game_sel).copied());
}

fn game_line(game: &Game) -> Line<'static> {
    let (marker, color) = if game.update_available {
        ("▲", WARN)
    } else if game.is_installed {
        ("●", OK)
    } else {
        ("·", DIM)
    };
    let name_style = if game.is_installed {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT)
    };
    Line::from(vec![
        Span::styled(format!("{marker} "), Style::default().fg(color)),
        Span::styled(format!("{:<34}", truncate(&game.name, 34)), name_style),
        Span::styled(format!(" {:>5}", game.app_id), Style::default().fg(DIM)),
    ])
}

fn draw_game_details(frame: &mut Frame, app: &App, area: Rect, game: Option<&Game>) {
    let Some(game) = game else {
        frame.render_widget(
            Paragraph::new("No games loaded. Press r to refresh, or l to sign in.")
                .block(block(" Details ")),
            area,
        );
        return;
    };
    let mut lines = vec![
        Line::from(Span::styled(
            game.name.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        kv("App ID", game.app_id.to_string()),
        kv("Status", game.status()),
        kv("Playtime", game.playtime()),
        kv(
            "Platform",
            game.platform.clone().unwrap_or_else(|| "-".into()),
        ),
        kv(
            "Online",
            match game.online_required {
                Some(true) => "required".to_string(),
                Some(false) => "not required".to_string(),
                None => "unknown".to_string(),
            },
        ),
        kv(
            "License",
            if game.is_family_shared {
                "Family Shared".to_string()
            } else if game.owned() {
                "Owned".to_string()
            } else {
                "Unavailable".to_string()
            },
        ),
    ];
    if !game.collections.is_empty() {
        lines.push(kv("Collections", game.collections.join(", ")));
    }
    if game.from_windows_steam {
        lines.push(kv("Install", "in-Wine Steam runtime".to_string()));
    }
    if let Some(path) = &game.install_path {
        lines.push(kv("Install path", path.clone()));
    }
    if let Some(branch) = &game.active_branch {
        lines.push(kv("Branch", branch.clone()));
    }
    lines.push(kv(
        "Store",
        game.store_url
            .clone()
            .unwrap_or_else(|| format!("https://store.steampowered.com/app/{}/", game.app_id)),
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Actions: Enter play/install · i install · u update · v verify · x uninstall",
        Style::default().fg(DIM),
    )));
    lines.push(Line::from(Span::styled(
        "         c cloud sync · g store page · a achievements · w workshop · d DLC",
        Style::default().fg(DIM),
    )));

    if let Some((app_id, value)) = &app.info
        && *app_id == game.app_id
    {
        lines.push(Line::from(""));
        lines.push(section("Store page"));
        lines.extend(store_detail_lines(*app_id, value));
    }
    if let Some(achievements) = &app.achievements
        && achievements.app_id == game.app_id
    {
        lines.push(Line::from(""));
        lines.push(section(&format!(
            "Achievements {}/{}",
            achievements.unlocked, achievements.total
        )));
        for achievement in achievements.achievements.iter().take(8) {
            let marker = if achievement.unlocked { "✔" } else { "·" };
            let color = if achievement.unlocked { OK } else { DIM };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), Style::default().fg(color)),
                Span::raw(truncate(&achievement.name, 48)),
                Span::styled(
                    format!("  {:.1}%", achievement.rarity),
                    Style::default().fg(DIM),
                ),
                Span::styled(
                    achievement
                        .date_unlocked
                        .clone()
                        .map(|date| format!("  {date}"))
                        .unwrap_or_default(),
                    Style::default().fg(DIM),
                ),
            ]));
            if !achievement.description.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("   {}", truncate(&achievement.description, 90)),
                    Style::default().fg(DIM),
                )));
            }
        }
    }
    if let Some((app_id, value)) = &app.workshop
        && *app_id == game.app_id
    {
        lines.push(Line::from(""));
        lines.push(section("Workshop"));
        if let Some(items) = value.as_array() {
            if items.is_empty() {
                lines.push(Line::from(Span::styled(
                    "No subscribed Workshop items.",
                    Style::default().fg(DIM),
                )));
            }
            for item in items.iter().take(6) {
                let title =
                    model::vstr(item, "title").unwrap_or_else(|| "Workshop item".to_string());
                lines.push(Line::from(Span::styled(
                    truncate(&title, 80),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )));
                if let Some(description) = model::vstr(item, "description") {
                    push_markdown(&mut lines, &description, 8);
                }
                if let Some(preview) = model::vstr(item, "preview_url") {
                    lines.push(Line::from(Span::styled(preview, Style::default().fg(DIM))));
                }
                lines.push(Line::from(""));
            }
        } else {
            lines.push(Line::from(truncate(&compact_value(value), 300)));
        }
    }
    if let Some((app_id, value)) = &app.dlc
        && *app_id == game.app_id
    {
        lines.push(Line::from(""));
        lines.push(section("DLC"));
        lines.push(Line::from(truncate(&compact_value(value), 300)));
    }

    let (image_area, text_area) = detail_areas(area, app);
    if let Some(image_area) = image_area {
        render_artwork(frame, app, image_area);
    }
    let block = block(" Details ");
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        text_area,
    );
}

// --------------------------------------------------------------------- store

fn draw_store(frame: &mut Frame, app: &App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(area);
    let query = if app.store.query.is_empty() {
        String::new()
    } else {
        format!(" · {}", app.store.query)
    };
    let title = format!(
        " Store · {} ({}){} ",
        app.store.view.title(),
        app.store.rows.len(),
        query
    );
    let mut items = Vec::new();
    for row in &app.store.rows {
        let price = row.price.clone().unwrap_or_else(|| {
            if row.is_free {
                "Free".to_string()
            } else {
                "-".to_string()
            }
        });
        let discount = if row.discount_pct > 0 {
            format!("-{}%", row.discount_pct)
        } else {
            String::new()
        };
        let kind = if row.kind.is_empty() || row.kind == "Game" {
            String::new()
        } else {
            format!(" [{}]", row.kind)
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("{:<12}", truncate(&price, 12)),
                Style::default().fg(if row.discount_pct > 0 { OK } else { TEXT }),
            ),
            Span::styled(
                format!("{:<7}", discount),
                Style::default().fg(WARN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<44}", truncate(&row.name(), 44)),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!(" {:>8}", row.app_id), Style::default().fg(DIM)),
            Span::styled(kind, Style::default().fg(DIM)),
        ])));
    }
    if app.input == super::app::Input::StoreSearch {
        items.insert(
            0,
            ListItem::new(Line::from(Span::styled(
                format!("search: {}█", app.store.query),
                Style::default().fg(ACCENT),
            ))),
        );
    }
    render_list(
        frame,
        panes[0],
        items,
        app.store.sel,
        title,
        &app.store_list,
    );

    draw_store_details(frame, app, panes[1], app.store.rows.get(app.store.sel));
}

fn draw_store_details(frame: &mut Frame, app: &App, area: Rect, row: Option<&model::StoreRow>) {
    let Some(row) = row else {
        frame.render_widget(
            Paragraph::new("No store rows. Press d for deals or / to search.")
                .block(block(" Store page ")),
            area,
        );
        return;
    };
    let mut lines = vec![
        Line::from(Span::styled(
            row.name(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        kv("App ID", row.app_id.to_string()),
        kv(
            "Price",
            row.price.clone().unwrap_or_else(|| {
                if row.is_free {
                    "Free".into()
                } else {
                    "-".into()
                }
            }),
        ),
    ];
    if row.discount_pct > 0 {
        lines.push(kv(
            "Discount",
            format!(
                "-{}% (was {})",
                row.discount_pct,
                row.original_price.clone().unwrap_or_else(|| "-".into())
            ),
        ));
    }
    if let Some(release) = &row.release_date {
        lines.push(kv("Release", release.clone()));
    }
    if let Some(reviews) = &row.reviews {
        lines.push(kv("Reviews", reviews.clone()));
    }
    if !row.kind.is_empty() {
        lines.push(kv("Type", row.kind.clone()));
    }
    if let Some(country) = &row.country {
        lines.push(kv("Region", country.clone()));
    }
    if let Some(priority) = row.priority.filter(|priority| *priority > 0) {
        lines.push(kv("Wishlist rank", priority.to_string()));
    }
    lines.push(kv(
        "Store",
        format!("https://store.steampowered.com/app/{}/", row.app_id),
    ));

    if let Some((app_id, value)) = &app.info {
        if *app_id == row.app_id {
            lines.push(Line::from(""));
            lines.push(section("From the storefront"));
            lines.extend(store_detail_lines(*app_id, value));
        } else {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Press Enter to load the full store page.",
                Style::default().fg(DIM),
            )));
        }
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press Enter to load the full store page, o to open it in a browser.",
            Style::default().fg(DIM),
        )));
    }

    let (image_area, text_area) = detail_areas(area, app);
    if let Some(image_area) = image_area {
        render_artwork(frame, app, image_area);
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block(" Store page "))
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        text_area,
    );
}

fn store_detail_lines(app_id: u32, value: &Value) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(kind) = model::vstr(value, "app_type") {
        lines.push(kv("Type", kind));
    }
    if model::vbool(value, "is_free") {
        lines.push(kv("Price", "Free to play".to_string()));
    } else if let Some(price) = model::vstr(value, "price") {
        let discount = model::vf64(value, "discount_pct").unwrap_or(0.0) as i32;
        let original = model::vstr(value, "original_price");
        let text = match (discount, original) {
            (d, Some(original)) if d > 0 => format!("{price}  (-{d}%, was {original})"),
            _ => price,
        };
        lines.push(kv("Price", text));
    }
    if let Some(release) = model::vstr(value, "release_date") {
        lines.push(kv("Release", release));
    }
    if let Some(reviews) = model::vstr(value, "review_summary") {
        lines.push(kv("Reviews", reviews));
    }
    let developers = model::vlist(value, "developers");
    if !developers.is_empty() {
        lines.push(kv("Developer", developers.join(", ")));
    }
    let publishers = model::vlist(value, "publishers");
    if !publishers.is_empty() {
        lines.push(kv("Publisher", publishers.join(", ")));
    }
    let platforms = model::vlist(value, "platforms");
    if !platforms.is_empty() {
        lines.push(kv("Platforms", platforms.join(", ")));
    }
    let tags = model::vlist(value, "tags");
    if !tags.is_empty() {
        lines.push(kv(
            "Tags",
            tags.iter().take(8).cloned().collect::<Vec<_>>().join(", "),
        ));
    }
    if let Some(short) = model::vstr(value, "short_description") {
        lines.push(Line::from(""));
        push_markdown(&mut lines, &short, 12);
    }
    if let Some(full) = model::vstr(value, "full_description")
        && model::vstr(value, "short_description").as_deref() != Some(full.as_str())
    {
        lines.push(Line::from(""));
        push_markdown(&mut lines, &full, 160);
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("store.steampowered.com/app/{app_id}/"),
        Style::default().fg(DIM),
    )));
    lines
}

// ----------------------------------------------------------------- community

fn draw_community(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(chat) = &app.chat {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
            .split(area);
        draw_friend_list(frame, app, panes[0]);
        draw_chat(frame, app, chat, panes[1]);
        return;
    }

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    draw_friend_list(frame, app, panes[0]);

    let mut lines = Vec::new();
    if let Some(profile) = &app.profile {
        lines.extend(profile_lines(profile, app));
    } else if let Some(friend) = app.selected_friend() {
        lines.push(Line::from(Span::styled(
            app.friend_name(&friend),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(kv("Status", friend.state().to_string()));
        let game = app
            .friend_game_label(&friend)
            .unwrap_or_else(|| "not in a game".to_string());
        lines.push(kv("Playing", truncate(&game, 48)));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press v to load the full profile.",
            Style::default().fg(DIM),
        )));
        lines.push(Line::from(Span::styled(
            "Enter: chat · v: profile · a: add friend · x: remove friend · o: browser",
            Style::default().fg(DIM),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "No friends loaded. Press r to refresh, or l to sign in.",
            Style::default().fg(DIM),
        )));
    }

    let (image_area, text_area) = detail_areas(panes[1], app);
    if let Some(image_area) = image_area {
        render_artwork(frame, app, image_area);
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block(" Community "))
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        text_area,
    );
}

fn draw_friend_list(frame: &mut Frame, app: &App, area: Rect) {
    let friends = app.filtered_friends();
    let mut items = Vec::new();
    for friend in &friends {
        let color = if friend.online() { OK } else { DIM };
        let marker = if friend.online() { "●" } else { "○" };
        let game = app.friend_game_label(friend).unwrap_or_else(|| {
            if friend.game_app_id.is_some() {
                "in game".to_string()
            } else {
                String::new()
            }
        });
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{marker} "), Style::default().fg(color)),
            Span::styled(
                format!("{:<18}", truncate(&app.friend_name(friend), 18)),
                Style::default().fg(if friend.online() { Color::White } else { TEXT }),
            ),
            Span::styled(
                format!(" {:<8}", friend.state()),
                Style::default().fg(color),
            ),
            Span::styled(truncate(&game, 22), Style::default().fg(DIM)),
        ])));
    }
    if app.input == super::app::Input::Filter {
        items.insert(
            0,
            ListItem::new(Line::from(Span::styled(
                format!("filter: {}█", app.filter),
                Style::default().fg(ACCENT),
            ))),
        );
    }
    let title = format!(" Friends ({}) ", friends.len());
    render_list(frame, area, items, app.friend_sel, title, &app.friend_list);
}

fn draw_chat(frame: &mut Frame, app: &App, chat: &Chat, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(area);

    let visible = chunks[0].height.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = chat
        .messages
        .iter()
        .map(|message| {
            let who = if message.from_self { "me" } else { &chat.name };
            let color = if message.from_self { OK } else { Color::White };
            Line::from(vec![
                Span::styled(
                    format!("{} ", model::hhmm(message.timestamp)),
                    Style::default().fg(DIM),
                ),
                Span::styled(format!("{who}: "), Style::default().fg(color)),
                Span::raw(truncate(&message.text, 200)),
            ])
        })
        .collect();
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            if chat.ready {
                "Connected. Type a message and press Enter."
            } else {
                "Connecting to Steam chat…"
            },
            Style::default().fg(DIM),
        )));
    }
    if chat.typing {
        lines.push(Line::from(Span::styled(
            format!("{} is typing…", chat.name),
            Style::default().fg(DIM),
        )));
    }
    if let Some(error) = &chat.error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(BAD),
        )));
    }
    let start = lines.len().saturating_sub(visible);
    let shown: Vec<Line> = lines.into_iter().skip(start).collect();
    let title = format!(
        " Chat · {}{} ",
        chat.name,
        if chat.ready { "" } else { " (connecting)" }
    );
    frame.render_widget(
        Paragraph::new(Text::from(shown))
            .block(block(title))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    let editing = app.input == super::app::Input::Chat;
    let style = if editing {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(DIM)
    };
    let hint = if editing {
        format!("> {}█", chat.input)
    } else {
        format!("> {}   (i to type, Esc to close)", chat.input)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, style))).block(block(" Message ")),
        chunks[1],
    );
}

fn profile_lines(profile: &UserProfile, app: &App) -> Vec<Line<'static>> {
    let now = now_secs();
    let display = match &profile.persona_name {
        Some(name) if !name.trim().is_empty() && name != &profile.steam_id.to_string() => {
            name.clone()
        }
        _ => {
            if !profile.is_public {
                "Private profile".to_string()
            } else {
                profile
                    .real_name
                    .clone()
                    .unwrap_or_else(|| "Unnamed profile".to_string())
            }
        }
    };
    let mut lines = vec![
        Line::from(Span::styled(
            display,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    if let Some(real) = &profile.real_name {
        lines.push(kv("Real name", real.clone()));
    }
    if let Some(headline) = &profile.headline {
        lines.push(Line::from(Span::styled(
            headline.clone(),
            Style::default().add_modifier(Modifier::ITALIC),
        )));
        lines.push(Line::from(""));
    }
    let game = profile
        .game_name
        .clone()
        .or_else(|| profile.game_app_id.and_then(|id| app.app_name(id)));
    if let Some(game) = game {
        lines.push(kv("Playing", truncate(&game, 48)));
    }
    lines.push(kv(
        "Presence",
        model::persona_state_label(profile.persona_state).to_string(),
    ));
    if let Some(created) = profile.created {
        lines.push(kv("Account age", model::humanize_age(created, now)));
    }
    if let Some(last) = profile.last_logoff {
        lines.push(kv("Last seen", model::humanize_ago(last, now)));
    }
    if let Some(location) = &profile.location {
        lines.push(kv("Location", location.clone()));
    }
    if profile.is_public {
        lines.push(kv("Visibility", "public".to_string()));
    } else {
        lines.push(kv("Visibility", "private or friends-only".to_string()));
    }
    if profile.is_limited {
        lines.push(kv("Account", "limited".to_string()));
    }
    if let Some(ban) = profile.ban_expires.filter(|ban| *ban > 0) {
        lines.push(kv("Ban", model::humanize_ago(ban, now)));
    }
    if let Some(summary) = &profile.summary {
        lines.push(Line::from(""));
        push_markdown(&mut lines, summary, 60);
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        profile.profile_url.clone(),
        Style::default().fg(DIM),
    )));
    lines
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

// -------------------------------------------------------------------- market

fn draw_market(frame: &mut Frame, app: &App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    app.market_list_height
        .set(panes[0].height.saturating_sub(2));
    let mut items = Vec::new();
    for row in &app.market_rows {
        if let MarketRow::Header(title) = row {
            items.push(ListItem::new(Line::from(Span::styled(
                format!("  ── {}", truncate(title, 52)),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ))));
            continue;
        }
        let (marker, color) = match row {
            MarketRow::Header(_) => ("─", ACCENT),
            MarketRow::Item(_) => ("◈", ACCENT),
            MarketRow::Listing(_) => ("$", OK),
            MarketRow::Buy(_) => ("¤", WARN),
            MarketRow::Inventory(_) => ("▣", TEXT),
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{marker} "), Style::default().fg(color)),
            Span::styled(
                format!("{:<46}", truncate(&row.title(), 46)),
                Style::default().fg(Color::White),
            ),
            Span::styled(row.subtitle(), Style::default().fg(color)),
        ])));
    }
    if app.input == super::app::Input::MarketSearch {
        items.insert(
            0,
            ListItem::new(Line::from(Span::styled(
                format!("search: {}█", app.market_query),
                Style::default().fg(ACCENT),
            ))),
        );
    }
    if app.input == super::app::Input::InventoryApp {
        items.insert(
            0,
            ListItem::new(Line::from(Span::styled(
                format!("inventory app id: {}█", app.prompt),
                Style::default().fg(ACCENT),
            ))),
        );
    }
    let title = if app.market_view == MarketView::Search {
        if app.market_total > 0 {
            format!(
                " Market · Search ({} results · {} loaded{}) ",
                app.market_total,
                app.market_rows.len(),
                if app.market_loading {
                    " · loading"
                } else {
                    ""
                }
            )
        } else {
            " Market · Search ".to_string()
        }
    } else {
        format!(
            " Market · {} ({}) ",
            app.market_view.title(),
            app.market_rows.len()
        )
    };
    render_list(
        frame,
        panes[0],
        items,
        app.market_sel,
        title,
        &app.market_list,
    );

    let mut lines = Vec::new();
    if let Some(wallet) = &app.wallet {
        lines.push(section("Wallet"));
        lines.push(kv(
            "Balance",
            model::vstr(wallet, "formatted").unwrap_or_else(|| "-".into()),
        ));
        if let Some(country) = model::vstr(wallet, "country") {
            lines.push(kv("Country", country));
        }
        lines.push(Line::from(""));
    } else if let Some(error) = &app.wallet_error {
        lines.push(section("Wallet"));
        lines.push(Line::from(Span::styled(
            truncate(error, 240),
            Style::default().fg(BAD),
        )));
        lines.push(Line::from(""));
    }
    if let Some(row) = app.selected_market_row() {
        lines.push(Line::from(Span::styled(
            row.title(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        match &row {
            MarketRow::Header(title) => {
                lines.push(kv("Group", title.clone()));
            }
            MarketRow::Item(item) => {
                let app = item
                    .app_name
                    .clone()
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or_else(|| item.market_hash_name.clone());
                lines.push(kv("Game", truncate(&app, 40)));
                lines.push(kv(
                    "Price",
                    item.sell_price_text.clone().unwrap_or_else(|| "-".into()),
                ));
                lines.push(kv("Listings", item.sell_listings.to_string()));
                lines.push(kv("Hash name", item.market_hash_name.clone()));
            }
            MarketRow::Listing(listing) => {
                lines.push(kv("Listing", listing.listing_id.clone()));
                lines.push(kv("Price", format!("{} (minor units)", listing.price)));
                lines.push(kv("Hash name", listing.market_hash_name.clone()));
            }
            MarketRow::Buy(order) => {
                lines.push(kv("Buy order", order.buy_order_id.clone()));
                lines.push(kv("Price", format!("{} (minor units)", order.price)));
                lines.push(kv("Quantity", order.quantity.to_string()));
                lines.push(kv("Hash name", order.market_hash_name.clone()));
            }
            MarketRow::Inventory(item) => {
                let game = if item.app_name.trim().is_empty() {
                    "Unknown game".to_string()
                } else {
                    item.app_name.clone()
                };
                lines.push(kv("Game", truncate(&game, 40)));
                if !item.context_name.is_empty() {
                    lines.push(kv("Context", item.context_name.clone()));
                } else if item.context_id != 0 {
                    lines.push(kv("Context", item.context_id.to_string()));
                }
                if !item.item_type.is_empty() {
                    lines.push(kv("Type", item.item_type.clone()));
                }
                lines.push(kv("Amount", item.amount.to_string()));
                lines.push(kv("Tradable", yesno(item.tradable)));
                lines.push(kv("Marketable", yesno(item.marketable)));
                lines.push(kv("Hash name", item.market_hash_name.clone()));
                if !item.asset_id.is_empty() {
                    lines.push(kv("Asset", item.asset_id.clone()));
                }
                if let Some(icon) = &item.icon_url {
                    lines.push(kv("Icon", icon.clone()));
                }
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Enter: price · /: search · i: all inventory · I: one game · o: open · scroll for more",
            Style::default().fg(DIM),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "No market rows. v: listings · /: search · i: all inventory · w: wallet",
            Style::default().fg(DIM),
        )));
    }
    if let Some(price) = &app.market_price {
        lines.push(Line::from(""));
        lines.push(section("Price overview"));
        lines.push(kv(
            "Item",
            model::vstr(price, "market_hash_name").unwrap_or_else(|| "-".into()),
        ));
        lines.push(kv(
            "Lowest",
            model::vstr(price, "lowest_price").unwrap_or_else(|| "-".into()),
        ));
        lines.push(kv(
            "Median",
            model::vstr(price, "median_price").unwrap_or_else(|| "-".into()),
        ));
        lines.push(kv(
            "Volume",
            model::vstr(price, "volume").unwrap_or_else(|| "-".into()),
        ));
    }

    let (image_area, text_area) = detail_areas(panes[1], app);
    if let Some(image_area) = image_area {
        render_artwork(frame, app, image_area);
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block(" Market details "))
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        text_area,
    );
}

// ----------------------------------------------------------------- downloads

fn draw_downloads(frame: &mut Frame, app: &App, area: Rect) {
    let has_installs = !app.installs.is_empty();
    let chunks = if has_installs {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4), Constraint::Length(7)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4)])
            .split(area)
    };

    let mut items = Vec::new();
    for download in &app.downloads {
        let closed = download.done && download.state == "closed";
        let color = if closed {
            WARN
        } else if download.done {
            if download.success { OK } else { BAD }
        } else {
            WARN
        };
        let state = if closed {
            "closed"
        } else if download.done {
            if download.success { "done" } else { "failed" }
        } else {
            download.state.as_str()
        };
        let rate = if download.speed_bps > 0.0 {
            format!("  {}", model::human_speed(download.speed_bps))
        } else {
            String::new()
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("{:<10} ", state),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<8} ", download.action),
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                format!("{:<38}", truncate(&download.name, 38)),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                format!("{:>6.1}%", download.percent),
                Style::default().fg(color),
            ),
            Span::styled(
                format!(
                    "  {}/{}",
                    model::human_bytes(download.downloaded_bytes),
                    model::human_bytes(download.total_bytes.max(download.downloaded_bytes))
                ),
                Style::default().fg(DIM),
            ),
            Span::styled(rate, Style::default().fg(DIM)),
            Span::styled(
                format!("  ETA {}", model::human_eta(download.eta_seconds)),
                Style::default().fg(DIM),
            ),
            Span::styled(
                format!("  {}", truncate(&download.message, 40)),
                Style::default().fg(DIM),
            ),
        ])));
    }
    if app.downloads.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "No active downloads. Install or update a game from the Library or Store.",
            Style::default().fg(DIM),
        ))));
    }
    let title = format!(" Downloads ({}) ", app.downloads.len());
    render_list(
        frame,
        chunks[0],
        items,
        app.download_sel,
        title,
        &app.download_list,
    );

    if has_installs {
        let mut install_items = Vec::new();
        for install in &app.installs {
            install_items.push(ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>6.1}% ", install.percent),
                    Style::default().fg(WARN),
                ),
                Span::styled(format!("{:<8} ", install.app_id), Style::default().fg(DIM)),
                Span::styled(
                    format!("{:<30}", truncate(&install.name, 30)),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    format!(
                        "{}/{} ",
                        model::human_bytes(install.downloaded_bytes),
                        model::human_bytes(install.total_bytes.max(install.downloaded_bytes))
                    ),
                    Style::default().fg(DIM),
                ),
                Span::styled(install.status.clone(), Style::default().fg(DIM)),
            ])));
        }
        frame.render_widget(
            List::new(install_items).block(block(" In-flight installs (daemon) ")),
            chunks[1],
        );
    }
}

// ------------------------------------------------------------------- account

fn draw_account(frame: &mut Frame, app: &App, area: Rect) {
    let lines = account_stats_lines(app);
    let (image_area, text_area) =
        if app.images_enabled && app.image.borrow().is_some() && area.height >= 10 {
            let height = (area.height / 4)
                .clamp(4, 10)
                .min(area.height.saturating_sub(4));
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(height), Constraint::Min(1)])
                .split(area);
            (Some(chunks[0]), chunks[1])
        } else {
            (None, area)
        };
    if let Some(image_area) = image_area {
        render_artwork(frame, app, image_area);
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block(" Account & stats "))
            .wrap(Wrap { trim: false })
            .scroll((app.detail_scroll, 0)),
        text_area,
    );
}

fn account_stats_lines(app: &App) -> Vec<Line<'static>> {
    let now = now_secs();
    let mut lines = Vec::new();

    if let Some(account) = &app.account {
        let display = app
            .own_profile
            .as_ref()
            .and_then(|profile| profile.persona_name.clone())
            .filter(|name| name != &account.steam_id.to_string())
            .or_else(|| account.persona_name.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| account.account_name.clone());
        lines.push(Line::from(Span::styled(
            display,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!("@{}", account.account_name),
            Style::default().fg(DIM),
        )));
        if let Some(name) = app
            .own_profile
            .as_ref()
            .and_then(|profile| profile.real_name.clone())
        {
            lines.push(Line::from(Span::styled(name, Style::default().fg(TEXT))));
        }
        lines.push(Line::from(""));

        lines.push(section("Account"));
        if let Some(created) = app.own_profile.as_ref().and_then(|profile| profile.created) {
            lines.push(kv("Account age", model::humanize_age(created, now)));
        }
        if !account.country.is_empty() {
            lines.push(kv("Country", account.country.clone()));
        }
        if let Some(location) = app
            .own_profile
            .as_ref()
            .and_then(|profile| profile.location.clone())
        {
            lines.push(kv("Location", location));
        }
        let email = if account.email.is_empty() || account.email.eq_ignore_ascii_case("hidden") {
            "hidden".to_string()
        } else if account.email_validated {
            "verified".to_string()
        } else {
            "unverified".to_string()
        };
        lines.push(kv("Email", email));
        lines.push(kv(
            "VAC bans",
            if account.vac_bans == 0 {
                "none".to_string()
            } else {
                account.vac_bans.to_string()
            },
        ));
        lines.push(kv("Devices", account.authed_machines.to_string()));
        if app
            .own_profile
            .as_ref()
            .is_some_and(|profile| profile.is_limited)
        {
            lines.push(kv("Account", "limited".to_string()));
        }
        lines.push(Line::from(""));

        lines.push(section("Library"));
        let owned = app.games.iter().filter(|game| game.owned()).count();
        let shared = app
            .games
            .iter()
            .filter(|game| game.is_family_shared && !game.owned())
            .count();
        let installed = app.games.iter().filter(|game| game.is_installed).count();
        let total_minutes: u64 = app
            .games
            .iter()
            .filter(|game| game.owned())
            .filter_map(|game| game.playtime_forever_minutes.map(u64::from))
            .sum();
        let played = app
            .games
            .iter()
            .filter(|game| game.playtime_forever_minutes.unwrap_or(0) > 0)
            .count();
        lines.push(kv("Owned games", owned.to_string()));
        if shared > 0 {
            lines.push(kv("Family Shared", shared.to_string()));
        }
        lines.push(kv("Installed", installed.to_string()));
        lines.push(kv("Playtime", format_minutes(total_minutes)));
        lines.push(kv("Games played", played.to_string()));

        let mut top: Vec<&Game> = app
            .games
            .iter()
            .filter(|game| game.playtime_forever_minutes.unwrap_or(0) > 0)
            .collect();
        top.sort_by_key(|game| std::cmp::Reverse(game.playtime_forever_minutes.unwrap_or(0)));
        if !top.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Most played",
                Style::default().fg(ACCENT),
            )));
            for game in top.iter().take(5) {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {:<44}", truncate(&game.name, 44)),
                        Style::default().fg(TEXT),
                    ),
                    Span::styled(
                        format!("{:>8}", game.playtime()),
                        Style::default().fg(ACCENT),
                    ),
                ]));
            }
        }
        lines.push(Line::from(""));

        lines.push(section("Community"));
        let friends: Vec<&Friend> = app
            .friends
            .iter()
            .filter(|friend| friend.is_individual())
            .collect();
        let online = friends.iter().filter(|friend| friend.online()).count();
        lines.push(kv(
            "Friends",
            format!("{} ({} online)", friends.len(), online),
        ));
        lines.push(Line::from(""));

        lines.push(section("Session"));
        if let Some(health) = &app.health {
            lines.push(kv(
                "Status",
                if health
                    .get("logged_in")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    "authenticated".to_string()
                } else {
                    "not authenticated".to_string()
                },
            ));
            if let Some(daemon) = health.get("daemon").and_then(Value::as_bool) {
                lines.push(kv(
                    "Connection",
                    if daemon {
                        "shared daemon session"
                    } else {
                        "standalone"
                    },
                ));
            }
            if health
                .get("web_token")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                lines.push(kv("Web access", "web token stored".to_string()));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "Not signed in. Press l to run `aurelia login`, or R to reconnect a stale session.",
            Style::default().fg(DIM),
        )));
    }

    if let Some(config) = &app.config {
        lines.push(Line::from(""));
        lines.push(section("Launcher config"));
        lines.push(Line::from(truncate(&pretty_value(config), 1600)));
    }
    lines
}

fn format_minutes(minutes: u64) -> String {
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours == 0 {
        format!("{mins}m")
    } else if mins == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {mins}m")
    }
}

// -------------------------------------------------------------------- common

/// Split a details pane into an artwork area and the text below it. Returns
/// no image area when support is unavailable or the pane is too short.
fn detail_areas(area: Rect, app: &App) -> (Option<Rect>, Rect) {
    if app.manage.is_some()
        || !app.images_enabled
        || app.image.borrow().is_none()
        || area.height < 8
    {
        return (None, area);
    }
    let height = (area.height / 3)
        .clamp(4, 14)
        .min(area.height.saturating_sub(3));
    if height < 4 {
        return (None, area);
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(height), Constraint::Min(1)])
        .split(area);
    (Some(chunks[0]), chunks[1])
}

fn render_artwork(frame: &mut Frame, app: &App, area: Rect) {
    let mut state = app.image.borrow_mut();
    if let Some(protocol) = state.as_mut() {
        frame.render_stateful_widget(
            StatefulImage::new().resize(Resize::Fit(Some(FilterType::Triangle))),
            area,
            protocol,
        );
    }
}

fn render_list(
    frame: &mut Frame,
    area: Rect,
    items: Vec<ListItem>,
    selected: usize,
    title: String,
    state: &RefCell<ListState>,
) {
    let list = List::new(items)
        .block(block(title))
        .highlight_style(
            Style::default()
                .bg(Color::Indexed(236))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    let mut state = state.borrow_mut();
    if !list.is_empty() {
        state.select(Some(selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn block(title: impl Into<String>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(title.into(), Style::default().fg(ACCENT)))
}

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("── {title} "),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    ))
}

fn kv(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<14}"), Style::default().fg(ACCENT)),
        Span::styled(value.into(), Style::default().fg(TEXT)),
    ])
}

fn push_markdown(lines: &mut Vec<Line<'static>>, text: &str, max_lines: usize) {
    let mut rendered = markdown::lines(text);
    rendered.truncate(max_lines);
    lines.extend(rendered);
}

fn yesno(value: bool) -> String {
    if value { "yes" } else { "no" }.to_string()
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn compact_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            let names: Vec<String> = items
                .iter()
                .filter_map(|item| match item {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(_) => model::vstr(item, "title")
                        .or_else(|| model::vstr(item, "name"))
                        .or_else(|| model::vstr(item, "app_id")),
                    _ => None,
                })
                .collect();
            if names.is_empty() {
                serde_json::to_string(value).unwrap_or_default()
            } else {
                names.join(", ")
            }
        }
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn pretty_value(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn draw_manage(frame: &mut Frame, app: &App, manage: &Manage, area: Rect) {
    let popup = centered(area, 86, 88);
    frame.render_widget(Clear, popup);

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" Manage · {} ", truncate(&manage.game.name, 60)),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = outer.inner(popup);
    frame.render_widget(outer, popup);

    let image_height = if app.images_enabled && app.image.borrow().is_some() {
        (inner.height / 4).clamp(4, 9)
    } else {
        0
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(image_height),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(inner);
    if image_height > 0 {
        render_artwork(frame, app, chunks[0]);
    }

    match manage.mode {
        ManageMode::Main => {
            let items = manage_main_items(manage);
            render_manage_list(frame, chunks[1], items, manage.sel);
        }
        ManageMode::Proton
        | ManageMode::Platform
        | ManageMode::Runner
        | ManageMode::Runtime
        | ManageMode::Prefix
        | ManageMode::Branch => {
            let items = manage_choice_items(manage);
            render_manage_list(frame, chunks[1], items, manage.sel);
        }
        ManageMode::Script => {
            let text = vec![
                Line::from(Span::styled(
                    "Launch script path",
                    Style::default().fg(ACCENT),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    format!("{}█", manage.prompt),
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Enter saves. Leave empty to clear and use auto-detection.",
                    Style::default().fg(DIM),
                )),
            ];
            frame.render_widget(
                Paragraph::new(Text::from(text))
                    .block(block(" Script "))
                    .wrap(Wrap { trim: false }),
                chunks[1],
            );
        }
        ManageMode::SteamLaunch | ManageMode::SteamTool => {
            let (title, hint) = if manage.mode == ManageMode::SteamLaunch {
                (
                    "Steam launch options",
                    "Enter saves to Steam. Leave empty to clear them.",
                )
            } else {
                (
                    "Steam compatibility tool",
                    "Raw Steam tool name, e.g. proton_10. Empty clears the mapping.",
                )
            };
            let text = vec![
                Line::from(Span::styled(title, Style::default().fg(ACCENT))),
                Line::from(""),
                Line::from(Span::styled(
                    format!("{}█", manage.prompt),
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
                Line::from(Span::styled(hint, Style::default().fg(DIM))),
                Line::from(Span::styled(
                    "This writes Steam's files and restarts Steam if it is running.",
                    Style::default().fg(DIM),
                )),
            ];
            frame.render_widget(
                Paragraph::new(Text::from(text))
                    .block(block(" Steam settings "))
                    .wrap(Wrap { trim: false }),
                chunks[1],
            );
        }
        ManageMode::LaunchOptions => {
            let mut lines = Vec::new();
            if let Some(options) = manage
                .launch_options
                .as_ref()
                .and_then(|value| value.get("launch_options"))
                .and_then(Value::as_array)
            {
                for option in options {
                    let os = model::vstr(option, "oslist").unwrap_or_else(|| "any".into());
                    let executable = model::vstr(option, "executable").unwrap_or_default();
                    let arguments = model::vstr(option, "arguments").unwrap_or_default();
                    let description = model::vstr(option, "description").unwrap_or_default();
                    lines.push(Line::from(vec![
                        Span::styled(format!("{os:<8}"), Style::default().fg(ACCENT)),
                        Span::styled(executable, Style::default().fg(Color::White)),
                        Span::styled(format!(" {arguments}"), Style::default().fg(DIM)),
                    ]));
                    if !description.is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!("         {description}"),
                            Style::default().fg(DIM),
                        )));
                    }
                }
            }
            if lines.is_empty() {
                lines.push(Line::from(Span::styled(
                    "Loading launch options…",
                    Style::default().fg(DIM),
                )));
            }
            frame.render_widget(
                Paragraph::new(Text::from(lines))
                    .block(block(" Launch options "))
                    .wrap(Wrap { trim: false })
                    .scroll((manage.scroll, 0)),
                chunks[1],
            );
        }
    }

    let status = manage.status.clone().unwrap_or_else(|| match manage.mode {
        ManageMode::Main => "Enter: change · Esc: close · r: refresh".to_string(),
        ManageMode::Script => "Enter: save · Esc: cancel".to_string(),
        ManageMode::SteamLaunch | ManageMode::SteamTool => {
            "Enter: save to Steam · Esc: cancel".to_string()
        }
        ManageMode::LaunchOptions => "PgUp/PgDn: scroll · Esc: back".to_string(),
        _ => "Enter: apply · Esc: back".to_string(),
    });
    let mut footer = vec![Line::from(Span::styled(status, Style::default().fg(WARN)))];
    if let Some(error) = &app.error {
        footer.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(BAD),
        )));
    } else {
        footer.push(Line::from(Span::styled(
            "Aura settings are local; Steam launch/tool writes update Steam and may restart it.",
            Style::default().fg(DIM),
        )));
    }
    frame.render_widget(
        Paragraph::new(Text::from(footer)).wrap(Wrap { trim: true }),
        chunks[2],
    );
}

fn manage_main_items(manage: &Manage) -> Vec<ListItem<'static>> {
    let config = manage.config.as_ref();
    let proton = config
        .and_then(|config| config.forced_proton_version.clone())
        .unwrap_or_else(|| {
            format!(
                "global default ({})",
                manage
                    .protons
                    .as_ref()
                    .map(|protons| protons.default.clone())
                    .unwrap_or_else(|| "?".to_string())
            )
        });
    let platform = config
        .and_then(|config| config.platform_preference.clone())
        .unwrap_or_else(|| "auto".to_string());
    let runner = match config
        .map(|config| config.runner.as_str())
        .unwrap_or("auto")
    {
        "luxtorpeda" => "luxtorpeda",
        "umu" => "umu",
        _ => "auto",
    };
    let runtime = match config
        .map(|config| config.steam_runtime_policy.as_str())
        .unwrap_or("Auto")
    {
        "Enabled" => "on",
        "Disabled" => "off",
        _ => "auto",
    };
    let prefix = match config
        .map(|config| config.steam_prefix_mode.as_str())
        .unwrap_or("Shared")
    {
        "PerGame" => "per-game",
        _ => "shared",
    };
    let script = config
        .and_then(|config| config.launch_script.clone())
        .unwrap_or_else(|| "(auto / none)".to_string());
    let branch = manage
        .game
        .active_branch
        .clone()
        .unwrap_or_else(|| "public".to_string());
    let steam_launch = config
        .and_then(|config| config.steam_launch_options.clone())
        .unwrap_or_else(|| "(none)".to_string());
    let steam_tool = config
        .and_then(|config| config.steam_compat_tool.clone())
        .unwrap_or_else(|| "(Steam default)".to_string());

    let rows = [
        ("Proton runtime", proton),
        ("Platform", platform),
        ("Runner", runner.to_string()),
        ("Steam runtime", runtime.to_string()),
        ("Prefix mode", prefix.to_string()),
        ("Launch script", script),
        ("Beta branch", branch),
        ("Launch options", "view".to_string()),
        ("Steam launch", truncate(&steam_launch, 46)),
        ("Steam tool", steam_tool),
        ("Verify files", "integrity scan".to_string()),
        ("Update", "download latest".to_string()),
        ("Cloud sync", "sync save games".to_string()),
        ("Uninstall", "remove game".to_string()),
    ];
    rows.iter()
        .enumerate()
        .map(|(index, (label, value))| {
            let value_style = if index == rows.len() - 1 {
                Style::default().fg(BAD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{label:<18}"), Style::default().fg(ACCENT)),
                Span::styled(value.clone(), value_style),
            ]))
        })
        .collect()
}

fn manage_choice_items(manage: &Manage) -> Vec<ListItem<'static>> {
    let config = manage.config.as_ref();
    let mut items: Vec<(String, bool)> = Vec::new();
    match manage.mode {
        ManageMode::Proton => {
            let current = config.and_then(|config| config.forced_proton_version.clone());
            items.push((
                format!(
                    "Global default ({})",
                    manage
                        .protons
                        .as_ref()
                        .map(|protons| protons.default.clone())
                        .unwrap_or_else(|| "?".to_string())
                ),
                current.is_none(),
            ));
            if let Some(protons) = &manage.protons {
                for runtime in &protons.installed {
                    items.push((
                        format!("{}  [{}]", runtime.name, runtime.location),
                        current.as_deref() == Some(runtime.name.as_str()),
                    ));
                }
            }
        }
        ManageMode::Platform => {
            let current = config.and_then(|config| config.platform_preference.clone());
            items.push((
                "Auto (detect installed payload)".to_string(),
                current.is_none(),
            ));
            items.push((
                "Windows (run through Proton/Wine)".to_string(),
                current.as_deref() == Some("windows"),
            ));
            items.push((
                "Linux (native)".to_string(),
                current.as_deref() == Some("linux"),
            ));
        }
        ManageMode::Runner => {
            let current = config
                .map(|config| config.runner.as_str())
                .unwrap_or("auto");
            items.push(("Auto (native or Proton)".to_string(), current == "auto"));
            items.push((
                "Luxtorpeda (native engine)".to_string(),
                current == "luxtorpeda",
            ));
            items.push((
                "umu (Proton via umu-launcher)".to_string(),
                current == "umu",
            ));
        }
        ManageMode::Runtime => {
            let current = match config
                .map(|config| config.steam_runtime_policy.as_str())
                .unwrap_or("Auto")
            {
                "Enabled" => "on",
                "Disabled" => "off",
                _ => "auto",
            };
            items.push(("Auto".to_string(), current == "auto"));
            items.push((
                "On (start Windows Steam runtime)".to_string(),
                current == "on",
            ));
            items.push(("Off".to_string(), current == "off"));
        }
        ManageMode::Prefix => {
            let current = match config
                .map(|config| config.steam_prefix_mode.as_str())
                .unwrap_or("Shared")
            {
                "PerGame" => "per-game",
                _ => "shared",
            };
            items.push((
                "Shared (master Steam prefix)".to_string(),
                current == "shared",
            ));
            items.push((
                "Per-game (copy Steam into the game prefix)".to_string(),
                current == "per-game",
            ));
        }
        ManageMode::Branch => {
            let current = manage
                .game
                .active_branch
                .clone()
                .unwrap_or_else(|| "public".to_string());
            if manage.branches.is_empty() {
                items.push(("Loading branches…".to_string(), false));
            } else {
                for branch in &manage.branches {
                    items.push((branch.clone(), branch == &current));
                }
            }
        }
        _ => {}
    }
    items
        .into_iter()
        .map(|(label, current)| {
            ListItem::new(Line::from(vec![
                Span::styled(if current { "* " } else { "  " }, Style::default().fg(OK)),
                Span::styled(label, Style::default().fg(Color::White)),
            ]))
        })
        .collect()
}

fn render_manage_list(
    frame: &mut Frame,
    area: Rect,
    items: Vec<ListItem<'static>>,
    selected: usize,
) {
    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(Color::Indexed(236))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    if !list.is_empty() {
        state.select(Some(selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let popup = centered(area, 78, 80);
    frame.render_widget(Clear, popup);
    let lines = vec![
        Line::from(Span::styled(
            "Aura Launcher — a terminal Steam client",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("1-6 / Tab        switch screen"),
        Line::from("Up/Down, j/k     move selection\nPgUp/PgDn        scroll the details pane"),
        Line::from("Enter            primary action (play/install, chat, store page, price)"),
        Line::from("/                filter or search the current screen"),
        Line::from("r                reload the current screen"),
        Line::from("l / L            log in / log out"),
        Line::from("?                close this help"),
        Line::from("q                quit"),
        Line::from(""),
        Line::from(Span::styled(
            "Library: i install · u update · v verify · x uninstall · S stop · c cloud sync · m manage",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            "         g store page · a achievements · w workshop · d DLC · s store search",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            "Store:   d deals · v wishlist · f add free license · a add · x remove · o browser",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            "Friends: Enter chat · i type · Esc close · v profile · a add · x remove",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            "Market:  v listings · / search · i all inventory · I one game · w wallet · Enter price",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            "Downloads: x stop · c clear finished · g go to game · r refresh daemon installs",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            "Account: l login · L logout · R reconnect a stale session · c config",
            Style::default().fg(DIM),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Descriptions are rendered as Markdown; artwork uses Kitty/Sixel/iTerm2",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            "or coloured half-blocks when available, and is omitted otherwise.",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            "Aurelia handles Steam login, CDN downloads, Proton launches,",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            "Cloud, Workshop, friends/chat, store and Community Market.",
            Style::default().fg(DIM),
        )),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block(" Help "))
            .wrap(Wrap { trim: true }),
        popup,
    );
}

fn draw_confirm(frame: &mut Frame, area: Rect, prompt: &str) {
    let popup = centered(area, 60, 20);
    frame.render_widget(Clear, popup);
    let text = vec![
        Line::from(Span::styled(
            prompt.to_string(),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "[y] yes    [n] no",
            Style::default().fg(ACCENT),
        )),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(text))
            .alignment(Alignment::Center)
            .block(block(" Confirm "))
            .wrap(Wrap { trim: true }),
        popup,
    );
}

fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
