//! TUI state, key handling, and background requests to Aurelia.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use image::DynamicImage;
use ratatui::widgets::ListState;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use serde_json::Value;

use super::backend::{Backend, ChatProcess, Stream};
use super::model::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Library,
    Store,
    Community,
    Market,
    Downloads,
    Account,
}

impl Screen {
    pub const ALL: [Screen; 6] = [
        Screen::Library,
        Screen::Store,
        Screen::Community,
        Screen::Market,
        Screen::Downloads,
        Screen::Account,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Library => "Library",
            Screen::Store => "Store",
            Screen::Community => "Community",
            Screen::Market => "Market",
            Screen::Downloads => "Downloads",
            Screen::Account => "Account",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreView {
    Deals,
    Search,
    Wishlist,
}

impl StoreView {
    pub fn title(self) -> &'static str {
        match self {
            StoreView::Deals => "Deals",
            StoreView::Search => "Search",
            StoreView::Wishlist => "Wishlist",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketView {
    Listings,
    Search,
    Inventory,
}

impl MarketView {
    pub fn title(self) -> &'static str {
        match self {
            MarketView::Listings => "Listings",
            MarketView::Search => "Search",
            MarketView::Inventory => "Inventory",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    None,
    Filter,
    StoreSearch,
    MarketSearch,
    AddFriend,
    InventoryApp,
    Chat,
}

#[derive(Clone)]
pub struct StoreState {
    pub view: StoreView,
    pub rows: Vec<StoreRow>,
    pub sel: usize,
    pub query: String,
    pub details: Option<Value>,
    pub loaded: bool,
}

impl Default for StoreState {
    fn default() -> Self {
        Self {
            view: StoreView::Deals,
            rows: Vec::new(),
            sel: 0,
            query: String::new(),
            details: None,
            loaded: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum MarketRow {
    Header(String),
    Item(MarketItem),
    Listing(Listing),
    Buy(BuyOrder),
    Inventory(InventoryItem),
}

impl MarketRow {
    pub fn title(&self) -> String {
        match self {
            MarketRow::Header(title) => title.clone(),
            MarketRow::Item(item) => item.name(),
            MarketRow::Listing(listing) => listing.market_hash_name.clone(),
            MarketRow::Buy(order) => order.market_hash_name.clone(),
            MarketRow::Inventory(item) => item.display_name(),
        }
    }

    pub fn subtitle(&self) -> String {
        match self {
            MarketRow::Header(_) => String::new(),
            MarketRow::Item(item) => item
                .sell_price_text
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            MarketRow::Listing(listing) => format!("{} (minor)", listing.price),
            MarketRow::Buy(order) => format!("{} x{} (minor)", order.price, order.quantity),
            MarketRow::Inventory(item) => format!("x{}", item.amount),
        }
    }

    pub fn price_query(&self, inventory_app_id: Option<u32>) -> Option<(u32, String)> {
        match self {
            MarketRow::Item(item) => Some((item.app_id, item.market_hash_name.clone())),
            MarketRow::Inventory(item) => Some((
                if item.app_id != 0 {
                    item.app_id
                } else {
                    inventory_app_id?
                },
                item.market_hash_name.clone(),
            )),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChatLine {
    pub from_self: bool,
    pub text: String,
    pub timestamp: u64,
}

pub struct Chat {
    pub steam_id: u64,
    pub name: String,
    pub messages: Vec<ChatLine>,
    pub input: String,
    pub process: Option<ChatProcess>,
    pub ready: bool,
    pub typing: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManageMode {
    Main,
    Proton,
    Platform,
    Runner,
    Runtime,
    Prefix,
    Branch,
    Script,
    LaunchOptions,
}

pub struct Manage {
    pub game: Game,
    pub config: Option<GameConfig>,
    pub protons: Option<ProtonList>,
    pub branches: Vec<String>,
    pub launch_options: Option<Value>,
    pub mode: ManageMode,
    pub sel: usize,
    pub prompt: String,
    pub scroll: u16,
    pub status: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Download {
    pub id: u64,
    pub app_id: u32,
    pub action: String,
    pub name: String,
    pub state: String,
    pub percent: f64,
    pub speed_bps: f64,
    pub eta_seconds: Option<u64>,
    pub file: String,
    pub message: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub done: bool,
    pub success: bool,
}

pub struct Confirm {
    pub prompt: String,
    pub action: ConfirmAction,
}

#[derive(Clone, Debug)]
pub enum ConfirmAction {
    Install(u32),
    Uninstall(u32),
    RemoveFriend(u64),
    Logout,
    StopInstall(u32),
    AddWishlist(u32),
    RemoveWishlist(u32),
}

pub enum Msg {
    Health(Result<Value>),
    Library(Result<Vec<Game>>),
    Store(StoreView, Result<Vec<StoreRow>>),
    StoreDetail(u32, Result<Value>),
    Friends(Result<Vec<Friend>>),
    Profile(u64, Result<UserProfile>),
    ChatHistory(u64, Result<Vec<ChatMessage>>),
    ChatRaw(u64, String, bool),
    ChatEnded(u64, Option<String>),
    Account(Result<Account>),
    OwnProfile(Result<UserProfile>),
    Achievements(u32, Result<Achievements>),
    Workshop(u32, Result<Value>),
    Dlc(u32, Result<Value>),
    Config(Result<Value>),
    MarketListings(Result<MarketState>),
    MarketSearch(Result<MarketSearchPage>),
    Inventory(Result<Vec<InventoryItem>>),
    MarketPrice(String, Result<Value>),
    Wallet(Result<Value>),
    Image(String, Result<DynamicImage>),
    InstallList(Result<Vec<InstallRow>>),
    JobLine {
        id: u64,
        text: String,
    },
    JobDone {
        id: u64,
        success: bool,
        error: Option<String>,
    },
    FriendName(u64, Result<UserProfile>),
    ManageConfig(u32, Result<GameConfig>),
    ManageProtons(Result<ProtonList>),
    ManageBranches(u32, Result<Branches>),
    ManageLaunchOptions(u32, Result<Value>),
    ManageSaved(u32, String, Result<Value>),
    Reconnected(Result<Value>),
    AppNames(Result<Value>),
    LoggedOut,
    Notice(String),
    Error(String),
}

pub struct ViewOptions {
    pub country: Option<String>,
    pub lang: Option<String>,
    pub only_installed: bool,
    pub picker: Picker,
    pub images_enabled: bool,
}

pub struct App {
    pub backend: Backend,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub country: Option<String>,
    pub lang: Option<String>,

    pub screen: Screen,
    pub quit: bool,
    pub login_requested: bool,
    pub help: bool,
    pub input: Input,
    pub filter: String,
    pub prompt: String,
    pub only_installed: bool,
    pub detail_scroll: u16,

    pub games: Vec<Game>,
    pub library_list: RefCell<ListState>,
    pub game_sel: usize,
    pub health: Option<Value>,
    pub account: Option<Account>,
    pub own_profile: Option<UserProfile>,
    pub store: StoreState,
    pub store_list: RefCell<ListState>,
    pub friends: Vec<Friend>,
    pub friend_list: RefCell<ListState>,
    pub friend_sel: usize,
    pub profile: Option<UserProfile>,
    pub chat: Option<Chat>,
    pub manage: Option<Manage>,
    pub market: MarketState,
    pub market_rows: Vec<MarketRow>,
    pub market_list: RefCell<ListState>,
    pub market_view: MarketView,
    pub market_sel: usize,
    pub market_query: String,
    pub market_start: u32,
    pub market_total: u32,
    pub market_app_id: Option<u32>,
    pub market_price: Option<Value>,
    pub wallet: Option<Value>,
    pub wallet_error: Option<String>,
    pub installs: Vec<InstallRow>,
    pub downloads: Vec<Download>,
    pub download_list: RefCell<ListState>,
    pub download_sel: usize,
    pub next_job: u64,
    pub picker: Picker,
    pub images_enabled: bool,
    pub image_key: Option<String>,
    pub image_loading: Option<String>,
    pub image_failed: HashSet<String>,
    pub image: RefCell<Option<StatefulProtocol>>,
    pub info: Option<(u32, Value)>,
    pub app_names: HashMap<u32, String>,
    pub profile_requested: Option<u64>,
    pub profile_pending: Option<(u64, u8)>,
    pub name_requests: HashSet<u64>,
    pub name_attempted: HashSet<u64>,
    pub achievements: Option<Achievements>,
    pub workshop: Option<(u32, Value)>,
    pub dlc: Option<(u32, Value)>,
    pub config: Option<Value>,
    pub confirm: Option<Confirm>,
    pub notice: Option<String>,
    pub error: Option<String>,
    pub loading: Option<String>,
    pub tick: u64,
}

impl App {
    /// Create the app and kick off the session health check and library scan.
    pub fn new(backend: Backend, tx: Sender<Msg>, rx: Receiver<Msg>, options: ViewOptions) -> Self {
        let mut app = Self {
            backend,
            tx,
            rx,
            country: options.country,
            lang: options.lang,
            screen: Screen::Library,
            quit: false,
            login_requested: false,
            help: false,
            input: Input::None,
            filter: String::new(),
            prompt: String::new(),
            only_installed: options.only_installed,
            detail_scroll: 0,
            games: Vec::new(),
            library_list: RefCell::new(ListState::default()),
            game_sel: 0,
            health: None,
            account: None,
            own_profile: None,
            store: StoreState::default(),
            store_list: RefCell::new(ListState::default()),
            friends: Vec::new(),
            friend_list: RefCell::new(ListState::default()),
            friend_sel: 0,
            profile: None,
            chat: None,
            manage: None,
            market: MarketState::default(),
            market_rows: Vec::new(),
            market_list: RefCell::new(ListState::default()),
            market_view: MarketView::Listings,
            market_sel: 0,
            market_query: String::new(),
            market_start: 0,
            market_total: 0,
            market_app_id: None,
            market_price: None,
            wallet: None,
            wallet_error: None,
            installs: Vec::new(),
            downloads: Vec::new(),
            download_list: RefCell::new(ListState::default()),
            download_sel: 0,
            next_job: 1,
            picker: options.picker,
            images_enabled: options.images_enabled,
            image_key: None,
            image_loading: None,
            image_failed: HashSet::new(),
            image: RefCell::new(None),
            info: None,
            app_names: HashMap::new(),
            profile_requested: None,
            profile_pending: None,
            name_requests: HashSet::new(),
            name_attempted: HashSet::new(),
            achievements: None,
            workshop: None,
            dlc: None,
            config: None,
            confirm: None,
            notice: None,
            error: None,
            loading: None,
            tick: 0,
        };
        app.refresh_health();
        app.refresh_library();
        app
    }

    pub fn poll(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.handle(msg);
        }
    }

    pub fn tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.clamp_selection();
        self.sync_image();
        if self.screen == Screen::Community
            && self.chat.is_none()
            && let Some((steam_id, wait)) = self.profile_pending
        {
            if wait == 0 {
                self.profile_pending = None;
                self.load_profile(steam_id);
            } else {
                self.profile_pending = Some((steam_id, wait - 1));
            }
        }
    }

    /// Pick the artwork for the current screen/selection. The URL doubles as the
    /// cache key, so loading a richer store image replaces the header image.
    fn desired_image(&self) -> Option<String> {
        if let Some(manage) = &self.manage {
            return Some(
                manage
                    .game
                    .assets
                    .header
                    .clone()
                    .unwrap_or_else(|| game_header_url(manage.game.app_id)),
            );
        }
        match self.screen {
            Screen::Library => {
                let game = self.selected_game()?;
                game.assets
                    .header
                    .clone()
                    .or_else(|| Some(game_header_url(game.app_id)))
            }
            Screen::Store => {
                let row = self.selected_store_row()?;
                if let Some((app_id, value)) = &self.info
                    && *app_id == row.app_id
                {
                    return asset_url(value, "header")
                        .or_else(|| asset_url(value, "capsule"))
                        .or_else(|| asset_url(value, "hero"))
                        .or_else(|| Some(game_header_url(row.app_id)));
                }
                Some(game_header_url(row.app_id))
            }
            Screen::Community => self
                .profile
                .as_ref()
                .and_then(|profile| profile.avatar_url.clone()),
            Screen::Market => match self.selected_market_row().as_ref() {
                Some(MarketRow::Inventory(item)) => item.icon_url.clone(),
                _ => None,
            },
            Screen::Account => self
                .own_profile
                .as_ref()
                .and_then(|profile| profile.avatar_url.clone()),
            Screen::Downloads => None,
        }
    }

    fn sync_image(&mut self) {
        if !self.images_enabled {
            return;
        }
        let desired = self.desired_image();
        if desired == self.image_key {
            return;
        }
        self.image_key = desired.clone();
        self.image_loading = None;
        *self.image.borrow_mut() = None;

        let Some(url) = desired else {
            return;
        };
        if self.image_failed.contains(&url) {
            return;
        }
        self.image_loading = Some(url.clone());
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = super::backend::fetch_image(&url);
            let _ = tx.send(Msg::Image(url, result));
        });
    }

    fn clear_session_data(&mut self) {
        self.health = None;
        self.account = None;
        self.own_profile = None;
        self.games.clear();
        self.friends.clear();
        self.store = StoreState::default();
        self.market = MarketState::default();
        self.market_rows.clear();
        self.market_price = None;
        self.wallet = None;
        self.wallet_error = None;
        self.installs.clear();
        self.info = None;
        self.app_names.clear();
        self.profile = None;
        self.profile_requested = None;
        self.profile_pending = None;
        self.name_requests.clear();
        self.name_attempted.clear();
        self.achievements = None;
        self.workshop = None;
        self.dlc = None;
        self.config = None;
        self.manage = None;
        self.image_key = None;
        self.image_loading = None;
        self.image_failed.clear();
        *self.image.borrow_mut() = None;
        self.close_chat();
    }

    fn clamp_selection(&mut self) {
        match self.screen {
            Screen::Library => {
                let len = self.filtered_games().len();
                clamp(&mut self.game_sel, len);
            }
            Screen::Store => clamp(&mut self.store.sel, self.store.rows.len()),
            Screen::Community => {
                let len = self.filtered_friends().len();
                clamp(&mut self.friend_sel, len);
            }
            Screen::Market => clamp(&mut self.market_sel, self.market_rows.len()),
            Screen::Downloads => clamp(&mut self.download_sel, self.downloads.len()),
            Screen::Account => {}
        }
    }

    pub fn login_finished(&mut self, success: bool) {
        self.login_requested = false;
        self.loading = None;
        if success {
            self.notice = Some("Signed in".to_string());
            self.clear_session_data();
            self.refresh_health();
            self.refresh_library();
        } else {
            self.error = Some("Login did not complete".to_string());
        }
    }

    // ------------------------------------------------------------------ data

    fn spawn_json(
        &mut self,
        label: &str,
        args: Vec<String>,
        make: impl FnOnce(Result<Value>) -> Msg + Send + 'static,
    ) {
        self.loading = Some(label.to_string());
        self.error = None;
        let backend = self.backend.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = backend.json_vec(&args);
            let _ = tx.send(make(result));
        });
    }

    fn with_region(&self, args: &mut Vec<String>) {
        if let Some(country) = &self.country {
            args.push("--country".to_string());
            args.push(country.clone());
        }
        if let Some(lang) = &self.lang {
            args.push("--lang".to_string());
            args.push(lang.clone());
        }
    }

    pub fn refresh_health(&mut self) {
        self.spawn_json(
            "checking session",
            vec!["login".into(), "--health".into()],
            Msg::Health,
        );
    }

    pub fn refresh_library(&mut self) {
        let label = if self.games.is_empty() {
            "loading library"
        } else {
            "refreshing library"
        };
        let mut args = vec!["list".to_string()];
        if self.only_installed {
            args.push("--installed".to_string());
        }
        self.spawn_json(label, args, |v| Msg::Library(v.and_then(games)));
    }

    fn load_store(&mut self, view: StoreView) {
        *self.store_list.borrow_mut() = ListState::default();
        self.store.view = view;
        self.store.sel = 0;
        self.store.details = None;
        self.store.loaded = false;
        let mut args = match view {
            StoreView::Deals => vec!["deals".to_string(), "-n".to_string(), "50".to_string()],
            StoreView::Search => {
                if self.store.query.trim().is_empty() {
                    return;
                }
                vec![
                    "search".to_string(),
                    self.store.query.clone(),
                    "-n".to_string(),
                    "50".to_string(),
                ]
            }
            StoreView::Wishlist => vec!["wishlist".to_string(), "-n".to_string(), "50".to_string()],
        };
        self.with_region(&mut args);
        let key = if view == StoreView::Wishlist {
            "items"
        } else {
            "results"
        };
        let msg_view = view;
        self.spawn_json("loading store", args, move |v| {
            Msg::Store(msg_view, v.and_then(|v| store_rows(v, key)))
        });
    }

    fn load_store_detail(&mut self, app_id: u32) {
        let mut args = vec!["info".to_string(), app_id.to_string()];
        self.with_region(&mut args);
        self.spawn_json("loading store page", args, move |v| {
            Msg::StoreDetail(app_id, v)
        });
    }

    fn load_friends(&mut self) {
        self.spawn_json("loading friends", vec!["friends".into()], |v| {
            Msg::Friends(v.and_then(friends))
        });
    }

    fn load_profile(&mut self, steam_id: u64) {
        self.profile_requested = Some(steam_id);
        self.profile_pending = None;
        let ident = if steam_id == 0 {
            "me".to_string()
        } else {
            steam_id.to_string()
        };
        self.spawn_json("loading profile", vec!["user".into(), ident], move |v| {
            Msg::Profile(steam_id, v.and_then(deserialize::<UserProfile>))
        });
    }

    fn maybe_load_selected_profile(&mut self) {
        if self.chat.is_some() {
            return;
        }
        let Some(friend) = self.selected_friend() else {
            return;
        };
        let steam_id = friend.steam_id;
        if self.profile.as_ref().map(|profile| profile.steam_id) == Some(steam_id) {
            return;
        }
        if self.profile_requested == Some(steam_id) {
            return;
        }
        self.load_profile(steam_id);
    }

    fn resolve_friend_games(&mut self) {
        let mut ids: Vec<u32> = self
            .filtered_friends()
            .iter()
            .filter_map(|friend| friend.game_app_id.filter(|id| self.app_name(*id).is_none()))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return;
        }
        let mut args = vec!["info".to_string()];
        args.extend(ids.into_iter().map(|id| id.to_string()));
        self.spawn_json("resolving game names", args, Msg::AppNames);
    }

    pub fn app_name(&self, app_id: u32) -> Option<String> {
        self.games
            .iter()
            .find(|game| game.app_id == app_id)
            .map(|game| game.name.clone())
            .or_else(|| self.app_names.get(&app_id).cloned())
    }

    pub fn friend_game_label(&self, friend: &Friend) -> Option<String> {
        friend
            .game_name
            .clone()
            .or_else(|| friend.game_app_id.and_then(|id| self.app_name(id)))
    }

    pub fn friend_name(&self, friend: &Friend) -> String {
        match &friend.persona_name {
            Some(name) if !name.trim().is_empty() && name != &friend.steam_id.to_string() => {
                name.clone()
            }
            Some(_) => "Unnamed profile".to_string(),
            None if !self.name_attempted.contains(&friend.steam_id)
                || self.name_requests.contains(&friend.steam_id) =>
            {
                "Loading name…".to_string()
            }
            None => "Unknown name".to_string(),
        }
    }

    fn resolve_friend_names(&mut self) {
        let mut pending: Vec<u64> = self
            .filtered_friends()
            .iter()
            .filter(|friend| friend.persona_name.is_none())
            .map(|friend| friend.steam_id)
            .filter(|id| !self.name_requests.contains(id) && !self.name_attempted.contains(id))
            .take(8)
            .collect();
        pending.sort_unstable();
        for steam_id in pending {
            if !self.name_requests.insert(steam_id) {
                continue;
            }
            self.spawn_json(
                "resolving friend name",
                vec!["user".into(), steam_id.to_string()],
                move |value| Msg::FriendName(steam_id, value.and_then(deserialize::<UserProfile>)),
            );
        }
    }

    fn load_account(&mut self) {
        self.spawn_json("loading account", vec!["account".into()], |v| {
            Msg::Account(v.and_then(deserialize::<Account>))
        });
    }

    fn load_own_profile(&mut self) {
        self.spawn_json(
            "loading profile",
            vec!["user".into(), "me".into()],
            |value| Msg::OwnProfile(value.and_then(deserialize::<UserProfile>)),
        );
    }

    fn load_installs(&mut self) {
        self.spawn_json(
            "loading installs",
            vec!["install".into(), "list".into()],
            |v| Msg::InstallList(v.and_then(install_rows)),
        );
    }

    fn load_market_listings(&mut self) {
        *self.market_list.borrow_mut() = ListState::default();
        self.market_view = MarketView::Listings;
        self.market_sel = 0;
        self.spawn_json(
            "loading listings",
            vec!["market".into(), "listings".into()],
            |v| Msg::MarketListings(v.and_then(deserialize::<MarketState>)),
        );
    }

    fn load_wallet(&mut self) {
        self.spawn_json("loading wallet", vec!["wallet".into()], Msg::Wallet);
    }

    fn load_inventory(&mut self, app_id: u32) {
        *self.market_list.borrow_mut() = ListState::default();
        self.market_view = MarketView::Inventory;
        self.market_app_id = Some(app_id);
        self.market_sel = 0;
        self.spawn_json(
            "loading inventory",
            vec!["inventory".into(), app_id.to_string()],
            |v| Msg::Inventory(v.and_then(inventory_items)),
        );
    }

    fn load_inventory_all(&mut self) {
        *self.market_list.borrow_mut() = ListState::default();
        self.market_view = MarketView::Inventory;
        self.market_app_id = None;
        self.market_sel = 0;
        self.spawn_json(
            "loading inventory",
            vec!["inventory".into(), "--all".into()],
            |v| Msg::Inventory(v.and_then(inventory_items)),
        );
    }

    fn start_market_search(&mut self) {
        *self.market_list.borrow_mut() = ListState::default();
        let query = self.market_query.trim().to_string();
        let mut args = vec!["market".to_string(), "search".to_string()];
        if !query.is_empty() {
            args.push(query);
        }
        args.push("-n".to_string());
        args.push("10".to_string());
        args.push("--start".to_string());
        args.push(self.market_start.to_string());
        self.market_view = MarketView::Search;
        self.market_sel = 0;
        self.spawn_json("searching market", args, |v| {
            Msg::MarketSearch(v.and_then(deserialize::<MarketSearchPage>))
        });
    }

    fn market_next_page(&mut self) {
        if self.market_view != MarketView::Search {
            return;
        }
        if self.market_start.saturating_add(10) >= self.market_total {
            self.notice = Some("Already on the last page".to_string());
            return;
        }
        self.market_start += 10;
        self.start_market_search();
    }

    fn market_previous_page(&mut self) {
        if self.market_view != MarketView::Search {
            return;
        }
        if self.market_start < 10 {
            self.notice = Some("Already on the first page".to_string());
            return;
        }
        self.market_start -= 10;
        self.start_market_search();
    }

    fn load_market_price(&mut self, app_id: u32, name: String) {
        self.spawn_json(
            "loading price",
            vec![
                "market".into(),
                "price".into(),
                app_id.to_string(),
                name.clone(),
            ],
            move |v| Msg::MarketPrice(name, v),
        );
    }

    fn load_config(&mut self) {
        self.spawn_json(
            "loading config",
            vec!["config".into(), "show".into()],
            Msg::Config,
        );
    }

    // ---------------------------------------------------------- manage window

    fn open_manage(&mut self, game: Game) {
        let id = game.app_id;
        self.manage = Some(Manage {
            game,
            config: None,
            protons: None,
            branches: Vec::new(),
            launch_options: None,
            mode: ManageMode::Main,
            sel: 0,
            prompt: String::new(),
            scroll: 0,
            status: Some("Loading settings…".to_string()),
        });
        self.load_manage_config(id);
        self.load_manage_protons();
    }

    fn load_manage_config(&mut self, app_id: u32) {
        self.spawn_json(
            "loading game settings",
            vec!["config".into(), "game".into(), app_id.to_string()],
            move |value| Msg::ManageConfig(app_id, value.and_then(deserialize::<GameConfig>)),
        );
    }

    fn load_manage_protons(&mut self) {
        self.spawn_json(
            "loading Proton runtimes",
            vec!["proton".into(), "list".into(), "--installed".into()],
            |value| Msg::ManageProtons(value.and_then(deserialize::<ProtonList>)),
        );
    }

    fn load_manage_branches(&mut self, app_id: u32) {
        self.spawn_json(
            "loading branches",
            vec!["branches".into(), app_id.to_string()],
            move |value| Msg::ManageBranches(app_id, value.and_then(deserialize::<Branches>)),
        );
    }

    fn load_manage_launch_options(&mut self, app_id: u32) {
        self.spawn_json(
            "loading launch options",
            vec!["launch-options".into(), app_id.to_string()],
            move |value| Msg::ManageLaunchOptions(app_id, value),
        );
    }

    fn manage_length(&self) -> usize {
        let Some(manage) = &self.manage else {
            return 0;
        };
        match manage.mode {
            ManageMode::Main => 12,
            ManageMode::Proton => {
                1 + manage
                    .protons
                    .as_ref()
                    .map(|protons| protons.installed.len())
                    .unwrap_or(0)
            }
            ManageMode::Platform | ManageMode::Runner | ManageMode::Runtime => 3,
            ManageMode::Prefix => 2,
            ManageMode::Branch => manage.branches.len().max(1),
            ManageMode::Script | ManageMode::LaunchOptions => 0,
        }
    }

    fn manage_move(&mut self, delta: isize) {
        let len = self.manage_length();
        if let Some(manage) = &mut self.manage {
            move_index(&mut manage.sel, len, delta);
        }
    }

    fn handle_manage_key(&mut self, key: KeyEvent) {
        let Some(mode) = self.manage.as_ref().map(|manage| manage.mode) else {
            return;
        };
        match mode {
            ManageMode::Main => match key.code {
                KeyCode::Esc => self.manage = None,
                KeyCode::Up | KeyCode::Char('k') => self.manage_move(-1),
                KeyCode::Down | KeyCode::Char('j') => self.manage_move(1),
                KeyCode::Enter => self.manage_enter_main(),
                KeyCode::Char('r') => {
                    if let Some(manage) = &self.manage {
                        let id = manage.game.app_id;
                        if let Some(manage) = &mut self.manage {
                            manage.status = Some("Refreshing…".to_string());
                        }
                        self.load_manage_config(id);
                        self.load_manage_protons();
                    }
                }
                _ => {}
            },
            ManageMode::Proton
            | ManageMode::Platform
            | ManageMode::Runner
            | ManageMode::Runtime
            | ManageMode::Prefix
            | ManageMode::Branch => match key.code {
                KeyCode::Esc => {
                    if let Some(manage) = &mut self.manage {
                        manage.mode = ManageMode::Main;
                        manage.sel = 0;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => self.manage_move(-1),
                KeyCode::Down | KeyCode::Char('j') => self.manage_move(1),
                KeyCode::Enter => self.manage_apply_choice(),
                _ => {}
            },
            ManageMode::Script => match key.code {
                KeyCode::Esc => {
                    if let Some(manage) = &mut self.manage {
                        manage.mode = ManageMode::Main;
                        manage.sel = 0;
                    }
                }
                KeyCode::Enter => self.manage_save_script(),
                KeyCode::Backspace => {
                    if let Some(manage) = &mut self.manage {
                        manage.prompt.pop();
                    }
                }
                KeyCode::Char(c) => {
                    if let Some(manage) = &mut self.manage {
                        manage.prompt.push(c);
                    }
                }
                _ => {}
            },
            ManageMode::LaunchOptions => match key.code {
                KeyCode::Esc => {
                    if let Some(manage) = &mut self.manage {
                        manage.mode = ManageMode::Main;
                        manage.sel = 0;
                        manage.scroll = 0;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if let Some(manage) = &mut self.manage {
                        manage.scroll = manage.scroll.saturating_sub(1);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if let Some(manage) = &mut self.manage {
                        manage.scroll = manage.scroll.saturating_add(1);
                    }
                }
                KeyCode::PageUp => {
                    if let Some(manage) = &mut self.manage {
                        manage.scroll = manage.scroll.saturating_sub(8);
                    }
                }
                KeyCode::PageDown => {
                    if let Some(manage) = &mut self.manage {
                        manage.scroll = manage.scroll.saturating_add(8);
                    }
                }
                _ => {}
            },
        }
    }

    fn manage_enter_main(&mut self) {
        let Some(manage) = &self.manage else {
            return;
        };
        let index = manage.sel;
        let app_id = manage.game.app_id;
        let needs_protons = manage.protons.is_none();
        let needs_branches = manage.branches.is_empty();
        let mode = match index {
            0 => Some(ManageMode::Proton),
            1 => Some(ManageMode::Platform),
            2 => Some(ManageMode::Runner),
            3 => Some(ManageMode::Runtime),
            4 => Some(ManageMode::Prefix),
            5 => Some(ManageMode::Script),
            6 => Some(ManageMode::Branch),
            7 => Some(ManageMode::LaunchOptions),
            _ => None,
        };
        if let Some(mode) = mode {
            if mode == ManageMode::Proton && needs_protons {
                self.load_manage_protons();
            }
            if mode == ManageMode::Branch && needs_branches {
                self.load_manage_branches(app_id);
            }
            if mode == ManageMode::LaunchOptions {
                self.load_manage_launch_options(app_id);
            }
            if let Some(manage) = &mut self.manage {
                manage.mode = mode;
                manage.sel = 0;
                manage.scroll = 0;
                if mode == ManageMode::Script {
                    manage.prompt = manage
                        .config
                        .as_ref()
                        .and_then(|config| config.launch_script.clone())
                        .unwrap_or_default();
                }
            }
            return;
        }
        match index {
            8 => {
                self.manage = None;
                self.job_selected("verify", vec!["verify".into()]);
            }
            9 => {
                self.manage = None;
                self.job_selected("update", vec!["update".into()]);
            }
            10 => {
                self.manage = None;
                self.job_selected("cloud sync", vec!["cloud".into(), "sync".into()]);
            }
            11 => {
                if let Some(manage) = self.manage.take() {
                    self.confirm = Some(Confirm {
                        prompt: format!("Uninstall {}?", manage.game.name),
                        action: ConfirmAction::Uninstall(manage.game.app_id),
                    });
                }
            }
            _ => {}
        }
    }

    fn manage_apply_choice(&mut self) {
        let Some(manage) = &self.manage else {
            return;
        };
        let app_id = manage.game.app_id;
        let index = manage.sel;
        let status: String;
        let mut args: Vec<String> = vec!["config".into(), "game".into(), app_id.to_string()];
        match manage.mode {
            ManageMode::Proton => {
                let name = if index == 0 {
                    None
                } else {
                    manage
                        .protons
                        .as_ref()
                        .and_then(|protons| protons.installed.get(index - 1))
                        .map(|runtime| runtime.name.clone())
                };
                match name {
                    Some(name) => {
                        args.push("--proton".into());
                        args.push(name.clone());
                        status = format!("Proton set to {name}");
                    }
                    None => {
                        args.push("--clear-proton".into());
                        status = "Using the global Proton default".to_string();
                    }
                }
            }
            ManageMode::Platform => {
                match index {
                    0 => args.push("--no-platform".into()),
                    1 => {
                        args.push("--platform".into());
                        args.push("windows".into());
                    }
                    _ => {
                        args.push("--platform".into());
                        args.push("linux".into());
                    }
                }
                status = "Platform preference saved".to_string();
            }
            ManageMode::Runner => {
                match index {
                    0 => args.push("--no-native-engine".into()),
                    1 => args.push("--native-engine".into()),
                    _ => args.push("--umu".into()),
                }
                status = "Runner saved".to_string();
            }
            ManageMode::Runtime => {
                let value = match index {
                    0 => "auto",
                    1 => "on",
                    _ => "off",
                };
                args.push("--steam-runtime".into());
                args.push(value.into());
                status = "Steam runtime policy saved".to_string();
            }
            ManageMode::Prefix => {
                let value = if index == 0 { "shared" } else { "per-game" };
                args.push("--steam-prefix-mode".into());
                args.push(value.into());
                status = "Prefix mode saved".to_string();
            }
            ManageMode::Branch => {
                let Some(branch) = manage.branches.get(index).cloned() else {
                    return;
                };
                args = vec!["set-branch".into(), app_id.to_string(), branch.clone()];
                status = format!("Branch set to {branch}");
                if let Some(manage) = &mut self.manage {
                    manage.game.active_branch = Some(branch);
                }
            }
            _ => return,
        }
        self.save_manage(app_id, status, args);
        if let Some(manage) = &mut self.manage {
            manage.mode = ManageMode::Main;
            manage.sel = 0;
            manage.status = Some("Saving…".to_string());
        }
    }

    fn manage_save_script(&mut self) {
        let Some(manage) = &self.manage else {
            return;
        };
        let app_id = manage.game.app_id;
        let path = manage.prompt.trim().to_string();
        let mut args = vec!["config".into(), "game".into(), app_id.to_string()];
        if path.is_empty() {
            args.push("--no-launch-script".into());
        } else {
            args.push("--launch-script".into());
            args.push(path);
        }
        self.save_manage(app_id, "Launch script saved".to_string(), args);
        if let Some(manage) = &mut self.manage {
            manage.mode = ManageMode::Main;
            manage.sel = 0;
            manage.status = Some("Saving…".to_string());
        }
    }

    fn save_manage(&mut self, app_id: u32, status: String, args: Vec<String>) {
        self.spawn_json("saving game settings", args, move |result| {
            Msg::ManageSaved(app_id, status, result)
        });
    }

    fn reload_screen(&mut self) {
        self.error = None;
        self.image_key = None;
        self.image_loading = None;
        self.image_failed.clear();
        *self.image.borrow_mut() = None;
        match self.screen {
            Screen::Library => {
                self.refresh_library();
                self.refresh_health();
            }
            Screen::Store => self.load_store(self.store.view),
            Screen::Community => {
                self.profile = None;
                self.profile_requested = None;
                self.profile_pending = None;
                self.load_friends();
            }
            Screen::Market => {
                match self.market_view {
                    MarketView::Listings => self.load_market_listings(),
                    MarketView::Search => self.start_market_search(),
                    MarketView::Inventory => {
                        if let Some(app_id) = self.market_app_id {
                            self.load_inventory(app_id);
                        } else {
                            self.load_inventory_all();
                        }
                    }
                }
                self.wallet_error = None;
                self.load_wallet();
            }
            Screen::Downloads => {
                clamp(&mut self.download_sel, self.downloads.len());
                self.load_installs();
            }
            Screen::Account => {
                self.load_account();
                self.refresh_health();
                self.load_own_profile();
                if self.games.is_empty() {
                    self.refresh_library();
                }
            }
        }
    }

    fn switch_screen(&mut self, screen: Screen) {
        self.screen = screen;
        self.input = Input::None;
        self.filter.clear();
        self.detail_scroll = 0;
        self.error = None;
        match screen {
            Screen::Store => {
                if !self.store.loaded {
                    self.load_store(self.store.view);
                }
            }
            Screen::Community => {
                if self.friends.is_empty() {
                    self.load_friends();
                } else {
                    self.profile_pending = None;
                    self.maybe_load_selected_profile();
                }
            }
            Screen::Market => {
                if self.market_rows.is_empty() {
                    self.load_market_listings();
                }
                if self.wallet.is_none() && self.wallet_error.is_none() {
                    self.load_wallet();
                }
            }
            Screen::Downloads => self.load_installs(),
            Screen::Account => {
                if self.account.is_none() {
                    self.load_account();
                }
                if self.own_profile.is_none() {
                    self.load_own_profile();
                }
                if self.games.is_empty() {
                    self.refresh_library();
                }
                self.refresh_health();
            }
            Screen::Library => {
                if self.games.is_empty() {
                    self.refresh_library();
                }
            }
        }
    }

    // -------------------------------------------------------------- selection

    pub fn filtered_games(&self) -> Vec<&Game> {
        let needle = self.filter.to_ascii_lowercase();
        self.games
            .iter()
            .filter(|game| !self.only_installed || game.is_installed)
            .filter(|game| {
                needle.is_empty()
                    || game.name.to_ascii_lowercase().contains(&needle)
                    || game.app_id.to_string().contains(&needle)
            })
            .collect()
    }

    pub fn filtered_friends(&self) -> Vec<&Friend> {
        let needle = self.filter.to_ascii_lowercase();
        self.friends
            .iter()
            .filter(|friend| friend.is_individual())
            .filter(|friend| {
                let name = friend.name().to_ascii_lowercase();
                needle.is_empty()
                    || name.contains(&needle)
                    || friend.steam_id.to_string().contains(&needle)
            })
            .collect()
    }

    pub fn selected_game(&self) -> Option<Game> {
        self.filtered_games()
            .get(self.game_sel)
            .map(|g| (*g).clone())
    }

    pub fn selected_friend(&self) -> Option<Friend> {
        self.filtered_friends()
            .get(self.friend_sel)
            .map(|f| (*f).clone())
    }

    pub fn selected_store_row(&self) -> Option<StoreRow> {
        self.store.rows.get(self.store.sel).cloned()
    }

    pub fn selected_market_row(&self) -> Option<MarketRow> {
        self.market_rows.get(self.market_sel).cloned()
    }

    pub fn selected_download(&self) -> Option<Download> {
        self.downloads.get(self.download_sel).cloned()
    }

    fn move_market_selection(&mut self, delta: isize) {
        let len = self.market_rows.len();
        if len == 0 {
            self.market_sel = 0;
            return;
        }
        let mut index = self.market_sel as isize;
        for _ in 0..len {
            index = (index + delta).clamp(0, (len - 1) as isize);
            if !matches!(self.market_rows[index as usize], MarketRow::Header(_)) {
                self.market_sel = index as usize;
                return;
            }
            if (index == 0 && delta < 0) || (index == (len - 1) as isize && delta > 0) {
                return;
            }
        }
    }

    fn select_game(&mut self, app_id: u32) {
        self.screen = Screen::Library;
        self.input = Input::None;
        self.filter.clear();
        if let Some(index) = self
            .filtered_games()
            .iter()
            .position(|game| game.app_id == app_id)
        {
            self.game_sel = index;
        }
    }

    fn move_selection(&mut self, delta: isize) {
        self.detail_scroll = 0;
        match self.screen {
            Screen::Library => {
                let len = self.filtered_games().len();
                move_index(&mut self.game_sel, len, delta);
            }
            Screen::Store => move_index(&mut self.store.sel, self.store.rows.len(), delta),
            Screen::Community => {
                let len = self.filtered_friends().len();
                let before = self.friend_sel;
                move_index(&mut self.friend_sel, len, delta);
                if self.friend_sel != before && self.chat.is_none() {
                    self.profile = None;
                    self.profile_requested = None;
                    self.profile_pending =
                        self.selected_friend().map(|friend| (friend.steam_id, 3));
                }
            }
            Screen::Market => self.move_market_selection(delta),
            Screen::Downloads => move_index(&mut self.download_sel, self.downloads.len(), delta),
            Screen::Account => {}
        }
    }

    // ------------------------------------------------------------- key events

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if self.help {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                self.help = false;
            }
            return;
        }
        if let Some(confirm) = self.confirm.take() {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => self.apply_confirm(confirm.action),
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {}
                _ => self.confirm = Some(confirm),
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => self.quit = true,
                KeyCode::Char(c @ '1'..='6') => {
                    let index = c as usize - '1' as usize;
                    self.switch_screen(Screen::ALL[index]);
                }
                _ => {}
            }
            return;
        }

        if self.manage.is_some() {
            return self.handle_manage_key(key);
        }

        match self.input {
            Input::Filter => return self.handle_filter_key(key),
            Input::StoreSearch => return self.handle_store_search_key(key),
            Input::MarketSearch => return self.handle_market_search_key(key),
            Input::AddFriend => return self.handle_add_friend_key(key),
            Input::InventoryApp => return self.handle_inventory_key(key),
            Input::Chat => return self.handle_chat_key(key),
            Input::None => {}
        }

        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                if self.chat.is_some() {
                    self.close_chat();
                } else {
                    self.filter.clear();
                    self.error = None;
                }
            }
            KeyCode::Char('?') => self.help = true,
            KeyCode::Tab => self.switch_screen(next_screen(self.screen)),
            KeyCode::BackTab => self.switch_screen(previous_screen(self.screen)),
            KeyCode::Char('r') | KeyCode::Char('R') => self.reload_screen(),
            KeyCode::Char('l') => self.login_requested = true,
            KeyCode::Char('L') => {
                self.confirm = Some(Confirm {
                    prompt: "Sign out of Steam?".to_string(),
                    action: ConfirmAction::Logout,
                });
            }
            KeyCode::Char(c @ '1'..='6') => {
                let index = c as usize - '1' as usize;
                self.switch_screen(Screen::ALL[index]);
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.scroll_details(-5),
            KeyCode::PageDown => self.scroll_details(5),
            KeyCode::Home => {
                self.move_selection(-(self.selection_len() as isize));
            }
            KeyCode::End => {
                self.move_selection(self.selection_len() as isize);
            }
            KeyCode::Enter => self.primary_action(),
            KeyCode::Char(c) => self.screen_char(c),
            _ => {}
        }
    }

    fn scroll_details(&mut self, delta: i32) {
        let next = i32::from(self.detail_scroll) + delta;
        self.detail_scroll = next.max(0) as u16;
    }

    fn selection_len(&self) -> usize {
        match self.screen {
            Screen::Library => self.filtered_games().len(),
            Screen::Store => self.store.rows.len(),
            Screen::Community => self.filtered_friends().len(),
            Screen::Market => self.market_rows.len(),
            Screen::Downloads => self.downloads.len(),
            Screen::Account => 0,
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match self.screen {
            Screen::Library => *self.library_list.borrow_mut() = ListState::default(),
            Screen::Community => *self.friend_list.borrow_mut() = ListState::default(),
            _ => {}
        }
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.input = Input::None;
            }
            KeyCode::Enter => self.input = Input::None,
            KeyCode::Backspace => {
                self.filter.pop();
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::PageUp => self.scroll_details(-5),
            KeyCode::PageDown => self.scroll_details(5),
            KeyCode::Char(c) => self.filter.push(c),
            _ => {}
        }
    }

    fn handle_store_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.input = Input::None,
            KeyCode::Enter => {
                self.input = Input::None;
                if !self.store.query.trim().is_empty() {
                    self.load_store(StoreView::Search);
                }
            }
            KeyCode::Backspace => {
                self.store.query.pop();
            }
            KeyCode::Char(c) => self.store.query.push(c),
            _ => {}
        }
    }

    fn handle_market_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.input = Input::None,
            KeyCode::Enter => {
                self.input = Input::None;
                self.market_start = 0;
                self.start_market_search();
            }
            KeyCode::Backspace => {
                self.market_query.pop();
            }
            KeyCode::Char(c) => self.market_query.push(c),
            _ => {}
        }
    }

    fn handle_add_friend_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.input = Input::None,
            KeyCode::Enter => {
                let query = self.prompt.trim().to_string();
                self.input = Input::None;
                if query.is_empty() {
                    return;
                }
                self.spawn_json(
                    "sending friend request",
                    vec!["friends".into(), "add".into(), query],
                    |v| match v {
                        Ok(_) => Msg::Notice("Friend request sent".to_string()),
                        Err(e) => Msg::Error(format!("{e:#}")),
                    },
                );
            }
            KeyCode::Backspace => {
                self.prompt.pop();
            }
            KeyCode::Char(c) => self.prompt.push(c),
            _ => {}
        }
    }

    fn handle_inventory_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.input = Input::None,
            KeyCode::Enter => {
                self.input = Input::None;
                if let Ok(app_id) = self.prompt.trim().parse::<u32>() {
                    self.load_inventory(app_id);
                } else {
                    self.error = Some("Inventory needs a numeric app id".to_string());
                }
            }
            KeyCode::Backspace => {
                self.prompt.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() => self.prompt.push(c),
            _ => {}
        }
    }

    fn handle_chat_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.close_chat(),
            KeyCode::Enter => self.send_chat(),
            KeyCode::Backspace => {
                if let Some(chat) = &mut self.chat {
                    chat.input.pop();
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(chat) = &mut self.chat {
                    chat.input.clear();
                }
            }
            KeyCode::Tab => self.input = Input::None,
            KeyCode::Char(c) => {
                if let Some(chat) = &mut self.chat {
                    chat.input.push(c);
                }
            }
            _ => {}
        }
    }

    fn screen_char(&mut self, c: char) {
        match self.screen {
            Screen::Library => match c {
                '/' => {
                    self.input = Input::Filter;
                    self.filter.clear();
                }
                'p' | 'P' => self.play_or_install(),
                'i' => self.confirm_install_selected(),
                'u' => self.job_selected("update", vec!["update".into()]),
                'v' => self.job_selected("verify", vec!["verify".into()]),
                'x' => {
                    if let Some(game) = self.selected_game() {
                        if game.is_installed {
                            self.confirm = Some(Confirm {
                                prompt: format!("Uninstall {}?", game.name),
                                action: ConfirmAction::Uninstall(game.app_id),
                            });
                        } else {
                            self.error = Some("That game is not installed".to_string());
                        }
                    }
                }
                'c' => self.job_selected("cloud sync", vec!["cloud".into(), "sync".into()]),
                'm' | 'M' => {
                    if let Some(game) = self.selected_game() {
                        self.open_manage(game);
                    }
                }
                'S' => {
                    if let Some(game) = self.selected_game() {
                        self.spawn_json(
                            "stopping game",
                            vec!["stop".into(), game.app_id.to_string()],
                            |v| match v {
                                Ok(_) => Msg::Notice("Stop requested".to_string()),
                                Err(e) => Msg::Error(format!("{e:#}")),
                            },
                        );
                    }
                }
                'g' => {
                    if let Some(game) = self.selected_game() {
                        self.load_store_detail(game.app_id);
                    }
                }
                'a' => {
                    if let Some(game) = self.selected_game() {
                        self.spawn_json(
                            "loading achievements",
                            vec!["achievements".into(), game.app_id.to_string()],
                            move |v| {
                                Msg::Achievements(
                                    game.app_id,
                                    v.and_then(deserialize::<Achievements>),
                                )
                            },
                        );
                    }
                }
                'w' => {
                    if let Some(game) = self.selected_game() {
                        self.spawn_json(
                            "loading workshop",
                            vec!["workshop".into(), "list".into(), game.app_id.to_string()],
                            move |v| Msg::Workshop(game.app_id, v),
                        );
                    }
                }
                'd' => {
                    if let Some(game) = self.selected_game() {
                        self.spawn_json(
                            "loading DLC",
                            vec!["dlc".into(), game.app_id.to_string()],
                            move |v| Msg::Dlc(game.app_id, v),
                        );
                    }
                }
                'o' => {
                    if let Some(game) = self.selected_game() {
                        self.open_url(&format!(
                            "https://store.steampowered.com/app/{}/",
                            game.app_id
                        ));
                    }
                }
                'f' => {
                    self.only_installed = !self.only_installed;
                    self.game_sel = 0;
                    *self.library_list.borrow_mut() = ListState::default();
                    self.notice = Some(if self.only_installed {
                        "Showing installed games".to_string()
                    } else {
                        "Showing the full library".to_string()
                    });
                }
                's' => {
                    if let Some(game) = self.selected_game() {
                        self.store.query = game.name;
                        self.screen = Screen::Store;
                        self.input = Input::None;
                        self.filter.clear();
                        self.load_store(StoreView::Search);
                    }
                }
                _ => {}
            },
            Screen::Store => match c {
                '/' => {
                    self.input = Input::StoreSearch;
                }
                'd' => self.load_store(StoreView::Deals),
                'v' => self.load_store(StoreView::Wishlist),
                'i' => self.confirm_install_store(),
                'a' => {
                    if let Some(row) = self.selected_store_row() {
                        self.confirm = Some(Confirm {
                            prompt: format!("Add {} to your wishlist?", row.name()),
                            action: ConfirmAction::AddWishlist(row.app_id),
                        });
                    }
                }
                'x' => {
                    if let Some(row) = self.selected_store_row() {
                        self.confirm = Some(Confirm {
                            prompt: format!("Remove {} from your wishlist?", row.name()),
                            action: ConfirmAction::RemoveWishlist(row.app_id),
                        });
                    }
                }
                'o' => {
                    if let Some(row) = self.selected_store_row() {
                        self.open_url(&format!(
                            "https://store.steampowered.com/app/{}/",
                            row.app_id
                        ));
                    }
                }
                'g' => {
                    if let Some(row) = self.selected_store_row() {
                        self.load_store_detail(row.app_id);
                    }
                }
                _ => {}
            },
            Screen::Community => {
                if self.chat.is_some() {
                    if c == 'i' {
                        self.input = Input::Chat;
                    }
                    return;
                }
                match c {
                    '/' => {
                        self.input = Input::Filter;
                        self.filter.clear();
                    }
                    'a' => {
                        self.prompt.clear();
                        self.input = Input::AddFriend;
                    }
                    'v' => {
                        if let Some(friend) = self.selected_friend() {
                            self.load_profile(friend.steam_id);
                        }
                    }
                    'm' => self.load_profile(0),
                    'x' => {
                        if let Some(friend) = self.selected_friend() {
                            self.confirm = Some(Confirm {
                                prompt: format!("Remove {} from your friends?", friend.name()),
                                action: ConfirmAction::RemoveFriend(friend.steam_id),
                            });
                        }
                    }
                    'o' => {
                        if let Some(friend) = self.selected_friend() {
                            self.open_url(&format!(
                                "https://steamcommunity.com/profiles/{}",
                                friend.steam_id
                            ));
                        } else if let Some(profile) = &self.profile {
                            let url = profile.profile_url.clone();
                            self.open_url(&url);
                        }
                    }
                    _ => {}
                }
            }
            Screen::Market => match c {
                '/' => {
                    self.input = Input::MarketSearch;
                }
                'v' => self.load_market_listings(),
                'w' => self.load_wallet(),
                'i' => self.load_inventory_all(),
                'I' => {
                    self.prompt.clear();
                    self.input = Input::InventoryApp;
                }
                '[' => self.market_previous_page(),
                ']' => self.market_next_page(),
                'o' => {
                    if let Some(row) = self.selected_market_row() {
                        let query = row.title().replace(' ', "+");
                        self.open_url(&format!(
                            "https://steamcommunity.com/market/search?q={query}"
                        ));
                    }
                }
                _ => {}
            },
            Screen::Downloads => match c {
                'x' => {
                    if let Some(download) = self.selected_download()
                        && !download.done
                    {
                        self.confirm = Some(Confirm {
                            prompt: format!("Stop {} of {}?", download.action, download.name),
                            action: ConfirmAction::StopInstall(download.app_id),
                        });
                    }
                }
                'c' => self.downloads.retain(|download| !download.done),
                'g' => {
                    if let Some(download) = self.selected_download() {
                        self.select_game(download.app_id);
                    }
                }
                _ => {}
            },
            Screen::Account => match c {
                'o' => {
                    if let Some(account) = &self.account {
                        self.open_url(&format!(
                            "https://steamcommunity.com/profiles/{}",
                            account.steam_id
                        ));
                    }
                }
                'c' => self.load_config(),
                'R' => {
                    self.spawn_json(
                        "reconnecting session",
                        vec!["login".into(), "--reconnect".into()],
                        Msg::Reconnected,
                    );
                }
                _ => {}
            },
        }
    }

    fn primary_action(&mut self) {
        match self.screen {
            Screen::Library => self.play_or_install(),
            Screen::Store => {
                if let Some(row) = self.selected_store_row() {
                    self.load_store_detail(row.app_id);
                }
            }
            Screen::Community => {
                if self.chat.is_some() {
                    self.input = Input::Chat;
                } else if let Some(friend) = self.selected_friend() {
                    self.open_chat(friend);
                }
            }
            Screen::Market => {
                if let Some((app_id, name)) = self
                    .selected_market_row()
                    .and_then(|row| row.price_query(self.market_app_id))
                {
                    self.load_market_price(app_id, name);
                }
            }
            Screen::Downloads => {
                if let Some(download) = self.selected_download() {
                    self.select_game(download.app_id);
                }
            }
            Screen::Account => {}
        }
    }

    // ------------------------------------------------------------- actions

    fn play_or_install(&mut self) {
        let Some(game) = self.selected_game() else {
            return;
        };
        if game.is_installed {
            self.start_job(
                game.app_id,
                "play",
                vec!["play".into(), game.app_id.to_string()],
            );
        } else if game.owned() || game.is_family_shared {
            self.confirm = Some(Confirm {
                prompt: format!("Download and install {}?", game.name),
                action: ConfirmAction::Install(game.app_id),
            });
        } else {
            self.error = Some("This game is not available to your account".to_string());
        }
    }

    fn confirm_install_selected(&mut self) {
        if let Some(game) = self.selected_game() {
            if game.is_installed {
                self.notice = Some(format!("{} is already installed", game.name));
                return;
            }
            self.confirm = Some(Confirm {
                prompt: format!("Download and install {}?", game.name),
                action: ConfirmAction::Install(game.app_id),
            });
        }
    }

    fn confirm_install_store(&mut self) {
        if let Some(row) = self.selected_store_row() {
            self.confirm = Some(Confirm {
                prompt: format!("Download and install {}?", row.name()),
                action: ConfirmAction::Install(row.app_id),
            });
        }
    }

    fn job_selected(&mut self, action: &str, prefix: Vec<String>) {
        let Some(game) = self.selected_game() else {
            return;
        };
        if !game.is_installed {
            self.error = Some(format!("{} is not installed", game.name));
            return;
        }
        let mut args = prefix;
        args.push(game.app_id.to_string());
        self.start_job(game.app_id, action, args);
    }

    fn start_job(&mut self, app_id: u32, action: &str, args: Vec<String>) {
        let id = self.next_job;
        self.next_job += 1;
        let name = self
            .games
            .iter()
            .find(|game| game.app_id == app_id)
            .map(|game| game.name.clone())
            .or_else(|| {
                self.store
                    .rows
                    .iter()
                    .find(|row| row.app_id == app_id)
                    .map(StoreRow::name)
            })
            .or_else(|| self.app_names.get(&app_id).cloned())
            .unwrap_or_else(|| "Unknown game".to_string());
        self.downloads.push(Download {
            id,
            app_id,
            action: action.to_string(),
            name: name.clone(),
            state: "starting".to_string(),
            percent: 0.0,
            speed_bps: 0.0,
            eta_seconds: None,
            file: String::new(),
            message: String::new(),
            downloaded_bytes: 0,
            total_bytes: 0,
            done: false,
            success: false,
        });
        self.notice = Some(format!("{action}: {name}"));
        self.error = None;

        let backend = self.backend.clone();
        let tx = self.tx.clone();
        let result = backend.stream(args, move |event| match event {
            Stream::Line { text, .. } => {
                let _ = tx.send(Msg::JobLine { id, text });
            }
            Stream::Done { success, error } => {
                let _ = tx.send(Msg::JobDone { id, success, error });
            }
        });
        if let Err(error) = result {
            self.error = Some(format!("{error:#}"));
            if let Some(download) = self.downloads.iter_mut().find(|d| d.id == id) {
                download.done = true;
                download.message = "failed to start".to_string();
            }
        }
    }

    fn open_chat(&mut self, friend: Friend) {
        self.close_chat();
        let steam_id = friend.steam_id;
        let name = self.friend_name(&friend);
        self.chat = Some(Chat {
            steam_id,
            name,
            messages: Vec::new(),
            input: String::new(),
            process: None,
            ready: false,
            typing: false,
            error: None,
        });
        self.input = Input::Chat;

        let tx = self.tx.clone();
        match self.backend.chat(steam_id, move |event| match event {
            Stream::Line { text, stderr } => {
                let _ = tx.send(Msg::ChatRaw(steam_id, text, stderr));
            }
            Stream::Done { error, .. } => {
                let _ = tx.send(Msg::ChatEnded(steam_id, error));
            }
        }) {
            Ok(process) => {
                if let Some(chat) = &mut self.chat {
                    chat.process = Some(process);
                }
            }
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                self.chat = None;
                self.input = Input::None;
                return;
            }
        }

        self.spawn_json(
            "loading chat history",
            vec![
                "chat".into(),
                "history".into(),
                steam_id.to_string(),
                "-n".into(),
                "50".into(),
            ],
            move |v| Msg::ChatHistory(steam_id, v.and_then(chat_history)),
        );
    }

    pub fn close_chat_now(&mut self) {
        self.close_chat();
    }

    fn close_chat(&mut self) {
        if let Some(process) = self.chat.take().and_then(|chat| chat.process) {
            process.kill();
        }
        if self.input == Input::Chat {
            self.input = Input::None;
        }
        if self.screen == Screen::Community {
            self.maybe_load_selected_profile();
        }
    }

    fn send_chat(&mut self) {
        let mut error = None;
        if let Some(chat) = &mut self.chat {
            let text = chat.input.trim().to_string();
            if text.is_empty() {
                return;
            }
            match chat.process.as_ref().map(|process| process.send(&text)) {
                None => error = Some("Chat is not connected".to_string()),
                Some(Ok(())) => {
                    chat.messages.push(ChatLine {
                        from_self: true,
                        text,
                        timestamp: now_secs(),
                    });
                    chat.input.clear();
                    chat.typing = false;
                }
                Some(Err(cause)) => error = Some(format!("{cause:#}")),
            }
        }
        if let Some(error) = error {
            self.error = Some(error);
        }
    }

    fn apply_confirm(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::Install(app_id) => {
                self.start_job(
                    app_id,
                    "install",
                    vec!["install".into(), app_id.to_string()],
                );
            }
            ConfirmAction::Uninstall(app_id) => {
                self.start_job(
                    app_id,
                    "uninstall",
                    vec!["uninstall".into(), app_id.to_string()],
                );
            }
            ConfirmAction::RemoveFriend(steam_id) => {
                self.spawn_json(
                    "removing friend",
                    vec!["friends".into(), "remove".into(), steam_id.to_string()],
                    |v| match v {
                        Ok(_) => Msg::Notice("Friend removed".to_string()),
                        Err(e) => Msg::Error(format!("{e:#}")),
                    },
                );
            }
            ConfirmAction::Logout => {
                let backend = self.backend.clone();
                let tx = self.tx.clone();
                thread::spawn(move || match backend.ok(&["logout"]) {
                    Ok(()) => {
                        let _ = tx.send(Msg::LoggedOut);
                    }
                    Err(error) => {
                        let _ = tx.send(Msg::Error(format!("{error:#}")));
                    }
                });
            }
            ConfirmAction::StopInstall(app_id) => {
                self.spawn_json(
                    "stopping install",
                    vec!["install".into(), "stop".into(), app_id.to_string()],
                    |v| match v {
                        Ok(_) => Msg::Notice("Stop requested".to_string()),
                        Err(e) => Msg::Error(format!("{e:#}")),
                    },
                );
            }
            ConfirmAction::AddWishlist(app_id) => {
                self.spawn_json(
                    "updating wishlist",
                    vec!["wishlist".into(), "add".into(), app_id.to_string()],
                    |v| match v {
                        Ok(_) => Msg::Notice("Added to wishlist".to_string()),
                        Err(e) => Msg::Error(format!("{e:#}")),
                    },
                );
            }
            ConfirmAction::RemoveWishlist(app_id) => {
                self.spawn_json(
                    "updating wishlist",
                    vec!["wishlist".into(), "remove".into(), app_id.to_string()],
                    |v| match v {
                        Ok(_) => Msg::Notice("Removed from wishlist".to_string()),
                        Err(e) => Msg::Error(format!("{e:#}")),
                    },
                );
            }
        }
    }

    fn open_url(&mut self, url: &str) {
        if let Err(error) = Backend::open_url(url) {
            self.error = Some(format!("{error:#}"));
        }
    }

    // ------------------------------------------------------------- messages

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Health(result) => {
                self.loading = None;
                match result {
                    Ok(value) => self.health = Some(value),
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::Library(result) => {
                self.loading = None;
                match result {
                    Ok(games) => {
                        self.games = games;
                        let len = self.filtered_games().len();
                        clamp(&mut self.game_sel, len);
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::Store(view, result) => {
                self.loading = None;
                match result {
                    Ok(rows) => {
                        self.store.view = view;
                        self.store.rows = rows;
                        self.store.sel = 0;
                        self.store.details = None;
                        self.store.loaded = true;
                        self.detail_scroll = 0;
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::StoreDetail(app_id, result) => {
                self.loading = None;
                match result {
                    Ok(mut value) => {
                        if value.is_array() {
                            value = value
                                .as_array()
                                .and_then(|items| items.first().cloned())
                                .unwrap_or(Value::Null);
                        }
                        self.info = Some((app_id, value));
                        self.detail_scroll = 0;
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::Friends(result) => {
                self.loading = None;
                match result {
                    Ok(friends) => {
                        self.friends = friends;
                        let len = self.filtered_friends().len();
                        clamp(&mut self.friend_sel, len);
                        self.profile_pending = None;
                        self.resolve_friend_names();
                        self.resolve_friend_games();
                        if self.screen == Screen::Community {
                            self.maybe_load_selected_profile();
                        }
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::Profile(steam_id, result) => {
                self.loading = None;
                match result {
                    Ok(profile) => {
                        let display_name = profile
                            .persona_name
                            .clone()
                            .filter(|name| name != &profile.steam_id.to_string())
                            .or_else(|| profile.real_name.clone());
                        let game_name = profile.game_name.clone();
                        if let Some(friend) = self
                            .friends
                            .iter_mut()
                            .find(|friend| friend.steam_id == profile.steam_id)
                        {
                            if let Some(name) = display_name {
                                friend.persona_name = Some(name);
                            }
                            if game_name.is_some() {
                                friend.game_name = game_name;
                            }
                        }
                        self.profile = Some(profile);
                    }
                    Err(error) => {
                        let _ = steam_id;
                        self.error = Some(format!("{error:#}"));
                    }
                }
            }
            Msg::ChatHistory(steam_id, result) => {
                self.loading = None;
                match result {
                    Ok(history) => {
                        if let Some(chat) = &mut self.chat
                            && chat.steam_id == steam_id
                        {
                            let mut lines: Vec<ChatLine> = history
                                .into_iter()
                                .map(|message| ChatLine {
                                    from_self: message.from_self,
                                    text: message.message,
                                    timestamp: message.timestamp,
                                })
                                .collect();
                            lines.append(&mut chat.messages);
                            chat.messages = lines;
                        }
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::ChatRaw(steam_id, text, stderr) => {
                let Some(chat) = &mut self.chat else {
                    return;
                };
                if chat.steam_id != steam_id {
                    return;
                }
                let Ok(value) = serde_json::from_str::<Value>(text.trim()) else {
                    if stderr && !text.trim().is_empty() {
                        chat.error = Some(text.trim().to_string());
                    }
                    return;
                };
                match value.get("event").and_then(Value::as_str).unwrap_or("") {
                    "ready" => {
                        chat.ready = true;
                        chat.typing = false;
                    }
                    "message" => {
                        if let Some(body) = value.get("text").and_then(Value::as_str) {
                            chat.messages.push(ChatLine {
                                from_self: false,
                                text: body.to_string(),
                                timestamp: value
                                    .get("timestamp")
                                    .and_then(Value::as_u64)
                                    .unwrap_or_else(now_secs),
                            });
                            chat.typing = false;
                        }
                    }
                    "echo" => {
                        if let Some(body) = value.get("text").and_then(Value::as_str) {
                            chat.messages.push(ChatLine {
                                from_self: true,
                                text: body.to_string(),
                                timestamp: value
                                    .get("timestamp")
                                    .and_then(Value::as_u64)
                                    .unwrap_or_else(now_secs),
                            });
                        }
                    }
                    "typing" => chat.typing = true,
                    "error" => {
                        chat.error = value
                            .get("message")
                            .and_then(Value::as_str)
                            .map(ToString::to_string);
                    }
                    "closed" => {
                        chat.ready = false;
                        chat.process = None;
                    }
                    _ => {}
                }
            }
            Msg::ChatEnded(steam_id, error) => {
                if let Some(chat) = &mut self.chat
                    && chat.steam_id == steam_id
                {
                    chat.process = None;
                    chat.ready = false;
                    if let Some(error) = error {
                        chat.error = Some(error);
                    }
                }
            }
            Msg::Account(result) => {
                self.loading = None;
                match result {
                    Ok(account) => self.account = Some(account),
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::OwnProfile(result) => {
                self.loading = None;
                match result {
                    Ok(profile) => self.own_profile = Some(profile),
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::Achievements(app_id, result) => {
                self.loading = None;
                match result {
                    Ok(mut achievements) => {
                        achievements.app_id = app_id;
                        self.achievements = Some(achievements);
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::Workshop(app_id, result) => {
                self.loading = None;
                match result {
                    Ok(value) => self.workshop = Some((app_id, value)),
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::Dlc(app_id, result) => {
                self.loading = None;
                match result {
                    Ok(value) => self.dlc = Some((app_id, value)),
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::Config(result) => {
                self.loading = None;
                match result {
                    Ok(value) => self.config = Some(value),
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::MarketListings(result) => {
                self.loading = None;
                match result {
                    Ok(state) => {
                        self.detail_scroll = 0;
                        self.market_rows = state
                            .listings
                            .iter()
                            .cloned()
                            .map(MarketRow::Listing)
                            .chain(state.buy_orders.iter().cloned().map(MarketRow::Buy))
                            .collect();
                        self.market = state;
                        self.market_view = MarketView::Listings;
                        self.market_sel = 0;
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::MarketSearch(result) => {
                self.loading = None;
                match result {
                    Ok(page) => {
                        self.market_total = page.total_count;
                        self.market_start = page.start;
                        self.market_rows = page.results.into_iter().map(MarketRow::Item).collect();
                        self.market_view = MarketView::Search;
                        self.market_sel = 0;
                        self.detail_scroll = 0;
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::Inventory(result) => {
                self.loading = None;
                match result {
                    Ok(items) => {
                        self.market_rows.clear();
                        let mut current_group: Option<String> = None;
                        for item in items {
                            let group = if item.is_card() {
                                "Trading cards".to_string()
                            } else if item.app_name.trim().is_empty() {
                                "Unknown game".to_string()
                            } else {
                                item.app_name.clone()
                            };
                            if current_group.as_deref() != Some(group.as_str()) {
                                self.market_rows.push(MarketRow::Header(group.clone()));
                                current_group = Some(group);
                            }
                            self.market_rows.push(MarketRow::Inventory(item));
                        }
                        self.market_view = MarketView::Inventory;
                        self.market_sel = self
                            .market_rows
                            .iter()
                            .position(|row| !matches!(row, MarketRow::Header(_)))
                            .unwrap_or(0);
                        self.detail_scroll = 0;
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::MarketPrice(_name, result) => {
                self.loading = None;
                match result {
                    Ok(value) => self.market_price = Some(value),
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::Wallet(result) => {
                self.loading = None;
                match result {
                    Ok(value) => {
                        self.wallet = Some(value);
                        self.wallet_error = None;
                    }
                    Err(error) => {
                        self.wallet_error = Some(format!("{error:#}"));
                    }
                }
            }
            Msg::Image(url, result) => {
                if self.image_loading.as_deref() == Some(url.as_str()) {
                    self.image_loading = None;
                }
                if self.image_key.as_deref() != Some(url.as_str()) {
                    return;
                }
                match result {
                    Ok(image) => {
                        *self.image.borrow_mut() = Some(self.picker.new_resize_protocol(image));
                    }
                    Err(_) => {
                        self.image_failed.insert(url);
                    }
                }
            }
            Msg::InstallList(result) => {
                self.loading = None;
                match result {
                    Ok(rows) => self.installs = rows,
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::JobLine { id, text } => self.handle_job_line(id, &text),
            Msg::JobDone { id, success, error } => self.handle_job_done(id, success, error),
            Msg::FriendName(steam_id, result) => {
                self.loading = None;
                self.name_requests.remove(&steam_id);
                self.name_attempted.insert(steam_id);
                if let Ok(profile) = result {
                    let display_name = profile
                        .persona_name
                        .clone()
                        .filter(|name| name != &profile.steam_id.to_string())
                        .or_else(|| profile.real_name.clone());
                    let game_name = profile.game_name.clone();
                    let game_app_id = profile.game_app_id;
                    if let Some(friend) = self
                        .friends
                        .iter_mut()
                        .find(|friend| friend.steam_id == steam_id)
                    {
                        if let Some(name) = display_name {
                            friend.persona_name = Some(name);
                        } else if let Some(numeric) = &profile.persona_name
                            && numeric == &steam_id.to_string()
                        {
                            friend.persona_name = Some(numeric.clone());
                        }
                        if game_name.is_some() {
                            friend.game_name = game_name;
                        }
                        if game_app_id.is_some() {
                            friend.game_app_id = game_app_id;
                        }
                    }
                }
                self.resolve_friend_names();
            }
            Msg::ManageConfig(app_id, result) => {
                self.loading = None;
                match result {
                    Ok(config) => {
                        if let Some(manage) = &mut self.manage
                            && manage.game.app_id == app_id
                        {
                            manage.config = Some(config);
                            if manage.status.as_deref() == Some("Loading settings…") {
                                manage.status = None;
                            }
                        }
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::ManageProtons(result) => {
                self.loading = None;
                match result {
                    Ok(protons) => {
                        if let Some(manage) = &mut self.manage {
                            manage.protons = Some(protons);
                        }
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::ManageBranches(app_id, result) => {
                self.loading = None;
                match result {
                    Ok(branches) => {
                        if let Some(manage) = &mut self.manage
                            && manage.game.app_id == app_id
                        {
                            manage.branches = branches.branches;
                            manage.status = None;
                        }
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::ManageLaunchOptions(app_id, result) => {
                self.loading = None;
                match result {
                    Ok(options) => {
                        if let Some(manage) = &mut self.manage
                            && manage.game.app_id == app_id
                        {
                            manage.launch_options = Some(options);
                            manage.status = None;
                        }
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::ManageSaved(app_id, status, result) => {
                self.loading = None;
                match result {
                    Ok(_) => {
                        if let Some(manage) = &mut self.manage
                            && manage.game.app_id == app_id
                        {
                            manage.status = Some(status);
                            manage.mode = ManageMode::Main;
                            manage.sel = 0;
                        }
                        self.load_manage_config(app_id);
                        self.refresh_library();
                    }
                    Err(error) => self.error = Some(format!("{error:#}")),
                }
            }
            Msg::AppNames(result) => {
                self.loading = None;
                if let Ok(value) = result {
                    let entries: Vec<&Value> = match &value {
                        Value::Array(items) => items.iter().collect(),
                        Value::Object(_) => vec![&value],
                        _ => Vec::new(),
                    };
                    for entry in entries {
                        let Some(id) = entry.get("app_id").and_then(Value::as_u64) else {
                            continue;
                        };
                        let Some(name) = entry.get("name").and_then(Value::as_str) else {
                            continue;
                        };
                        if !name.trim().is_empty() {
                            self.app_names.insert(id as u32, name.to_string());
                        }
                    }
                }
            }
            Msg::Reconnected(result) => {
                self.loading = None;
                match result {
                    Ok(_) => {
                        self.notice = Some("Session reconnected".to_string());
                        self.refresh_health();
                        self.refresh_library();
                    }
                    Err(error) => {
                        self.error = Some(format!("{error:#} — press l to sign in again"));
                    }
                }
            }
            Msg::LoggedOut => {
                self.clear_session_data();
                self.notice = Some("Signed out".to_string());
            }
            Msg::Notice(text) => {
                self.loading = None;
                self.notice = Some(text);
                self.error = None;
            }
            Msg::Error(text) => {
                self.loading = None;
                self.error = Some(text);
            }
        }
    }

    fn handle_job_line(&mut self, id: u64, text: &str) {
        let Some(download) = self.downloads.iter_mut().find(|d| d.id == id) else {
            return;
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            if let Some(progress) = Progress::from_value(&value) {
                download.state = progress.state;
                download.percent = progress.percent;
                download.speed_bps = progress.speed_bps;
                download.eta_seconds = progress.eta_seconds;
                download.file = progress.file;
                download.downloaded_bytes = progress.bytes_downloaded;
                download.total_bytes = progress.total_bytes;
                return;
            }
            match value.get("event").and_then(Value::as_str).unwrap_or("") {
                "result" => {
                    download.success = true;
                    download.state = "done".to_string();
                    download.message = value
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("complete")
                        .to_string();
                }
                "error" => {
                    if let Some(message) = value.get("message").and_then(Value::as_str) {
                        download.message = message.to_string();
                    }
                }
                _ => {
                    if let Some(message) = value.get("error").and_then(Value::as_str) {
                        download.message = message.to_string();
                    }
                }
            }
            return;
        }
        download.message = truncate(trimmed, 100);
    }

    fn handle_job_done(&mut self, id: u64, success: bool, error: Option<String>) {
        let Some(download) = self.downloads.iter_mut().find(|d| d.id == id) else {
            return;
        };
        download.done = true;
        download.success = success;
        if success {
            download.percent = 100.0;
            download.state = "done".to_string();
            if download.message.is_empty() {
                download.message = "complete".to_string();
            }
        } else {
            download.state = "failed".to_string();
            if let Some(error) = error {
                download.message = error;
            } else if download.message.is_empty() {
                download.message = "failed".to_string();
            }
        }
        let action = download.action.clone();
        let name = download.name.clone();
        let app_id = download.app_id;
        if action == "play" {
            download.state = "closed".to_string();
            if success {
                download.message = "closed".to_string();
                self.notice = Some(format!("Game closed: {name}"));
            } else {
                let log = game_log_path(app_id);
                download.message = format!("closed; log: {log}");
                self.notice = Some(format!("Game closed: {name} — log: {log}"));
            }
            self.refresh_library();
            return;
        }
        if success {
            self.notice = Some(format!("{action} finished: {name}"));
        } else {
            self.error = Some(format!("{action} failed: {name}"));
        }
        self.refresh_library();
    }

    pub fn session_label(&self) -> String {
        let Some(health) = &self.health else {
            return "session: checking".to_string();
        };
        if health
            .get("logged_in")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let name = health
                .get("account")
                .and_then(Value::as_str)
                .or_else(|| self.account.as_ref().map(|a| a.account_name.as_str()))
                .unwrap_or("signed in");
            name.to_string()
        } else {
            "not signed in (press l)".to_string()
        }
    }

    pub fn pending_label(&self) -> Option<&str> {
        self.loading.as_deref()
    }
}

fn game_log_path(app_id: u32) -> String {
    let dir = aurelia::core::config::config_dir()
        .map(|dir| dir.join("logs"))
        .unwrap_or_else(|_| std::path::PathBuf::from("~/.config/Aurelia/logs"));
    dir.join(format!("wine_{app_id}.log")).display().to_string()
}

fn game_header_url(app_id: u32) -> String {
    format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/header.jpg")
}

fn asset_url(value: &Value, key: &str) -> Option<String> {
    value
        .get("assets")
        .and_then(|assets| vstr(assets, key))
        .filter(|url| !url.trim().is_empty())
}

fn next_screen(screen: Screen) -> Screen {
    let index = (screen.index() + 1) % Screen::ALL.len();
    Screen::ALL[index]
}

fn previous_screen(screen: Screen) -> Screen {
    let index = (screen.index() + Screen::ALL.len() - 1) % Screen::ALL.len();
    Screen::ALL[index]
}

fn move_index(index: &mut usize, len: usize, delta: isize) {
    if len == 0 {
        *index = 0;
        return;
    }
    let next = index.saturating_add_signed(delta);
    *index = next.min(len - 1);
}

fn clamp(index: &mut usize, len: usize) {
    if len == 0 {
        *index = 0;
    } else if *index >= len {
        *index = len - 1;
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_cycle_wraps() {
        assert_eq!(next_screen(Screen::Account), Screen::Library);
        assert_eq!(previous_screen(Screen::Library), Screen::Account);
    }

    #[test]
    fn selection_stays_in_bounds() {
        let mut index = 1;
        move_index(&mut index, 3, 10);
        assert_eq!(index, 2);
        move_index(&mut index, 3, -10);
        assert_eq!(index, 0);
        move_index(&mut index, 0, 1);
        assert_eq!(index, 0);
    }
}
