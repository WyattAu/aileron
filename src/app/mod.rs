use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

pub mod cmd;
pub mod commands;
pub mod dispatch;
pub mod event_handler;
pub mod events;
pub mod instance;
pub mod omnibox;
pub mod palette;
pub mod panes;
pub mod popup;
pub mod render;

use crate::config::Config;
use crate::db::bookmarks;
use crate::extensions::ExtensionManager;
use crate::input::{KeybindingRegistry, Mode};
#[cfg(feature = "lua")]
use crate::lua::LuaEngine;
#[cfg(feature = "passwords")]
use crate::passwords::BitwardenClient;
use crate::servo::PaneStateManager;
use crate::ui::palette::CommandPalette;
use crate::ui::search::SearchCategory;
use crate::ui::search::SearchItem;
use crate::wm::{BspTree, Rect};
use uuid::Uuid;

/// Actions to be executed on wry panes by main.rs.
/// Used as a bridge since AppState doesn't own WryPaneManager.
#[derive(Debug, Clone, PartialEq)]
pub enum WryAction {
    /// Navigate the active pane to a URL.
    Navigate(url::Url),
    /// Go back in the active pane's history.
    Back,
    /// Go forward in the active pane's history.
    Forward,
    /// Reload the current page.
    Reload,
    /// Toggle bookmark on the current URL (main.rs will read URL from wry).
    ToggleBookmark,
    /// Auto-fill credentials into the active pane via JavaScript.
    Autofill { js: String },
    /// Open WebKit devtools for the active pane.
    ToggleDevTools,
    /// Scroll the active pane by a pixel offset.
    ScrollBy { x: f64, y: f64 },
    /// Smooth scroll the webview (uses CSS smooth behavior).
    SmoothScroll { x: f64, y: f64 },
    /// Scroll the active pane to a position (fraction of page height from top).
    ScrollTo { fraction: f64 },
    /// Run arbitrary JavaScript in the active pane.
    RunJs(String),
    /// Save workspace with live URLs from wry panes.
    /// main.rs collects URLs from WryPaneManager and sends them back.
    SaveWorkspace {
        name: String,
        /// Maps pane_id -> live URL string, collected from WryPaneManager.
        pane_urls: std::collections::HashMap<uuid::Uuid, String>,
    },
    /// Enter reader mode: strip CSS, extract article content, display clean text.
    EnterReaderMode,
    /// Exit reader mode: reload the original URL.
    ExitReaderMode,
    /// Enter minimal mode: reload with JS disabled and images blocked.
    EnterMinimalMode,
    /// Exit minimal mode: reload with normal settings.
    ExitMinimalMode,
    /// Show an error page in the active pane (graceful pane error handling).
    ShowPaneError { message: String },
    /// List loaded content scripts (handled by main.rs).
    ListContentScripts,
    /// Get network log from active pane.
    GetNetworkLog,
    /// Clear network log from active pane.
    ClearNetworkLog,
    /// Get JS console log from active pane.
    GetConsoleLog,
    /// Clear JS console log from active pane.
    ClearConsoleLog,
    /// Save current config to disk.
    SaveConfig,
    /// Print the current page.
    Print,
    /// Toggle mute on the active pane (pause/mute media elements).
    ToggleMute,
    /// Capture the current scroll fraction via JS and send it back via IPC.
    /// Used by the mark-set feature to record the actual scroll position.
    CaptureScrollFraction,
    /// Set the system clipboard contents.
    SetClipboard(String),
}

#[derive(Clone)]
pub struct TabDisplayInfo {
    pub title: String,
    pub url: String,
    pub truncated_title_horizontal: String,
    pub truncated_title_sidebar: String,
    pub truncated_url: String,
}

#[derive(Default)]
pub struct CrashRecoveryState {
    pub webview_crash_detected: bool,
    pub crashed_pane_url: Option<String>,
    pub crashed_pane_id: Option<Uuid>,
}

#[derive(Default)]
pub struct AutofillState {
    pub available: bool,
    pub username_id: String,
    pub password_id: String,
    pub js: Option<String>,
    pub status_msg: String,
}

/// Pending permission request from an extension.
/// Set when an extension calls `permissions.request()`, consumed when
/// the user clicks Allow or Deny in the prompt dialog.
#[derive(Debug, Clone)]
pub struct PendingPermissionRequest {
    /// Extension ID that requested the permission.
    pub extension_id: String,
    /// Human-readable extension name (from manifest).
    pub extension_name: String,
    /// Permissions being requested (for display).
    pub permissions: Vec<String>,
    /// Unique request ID for correlating with the JS Promise.
    pub request_id: u64,
}

/// A sync conflict entry for display in the conflicts panel.
#[derive(Debug, Clone)]
pub struct SyncConflictEntry {
    /// Relative path of the conflicted file.
    pub path: String,
    /// BLAKE3 hash of the local version.
    pub local_hash: String,
    /// BLAKE3 hash of the remote version.
    pub remote_hash: String,
    /// Local file size in bytes.
    pub local_size: u64,
    /// Remote file size in bytes.
    pub remote_size: u64,
}

#[derive(Default)]
pub struct PanelState {
    pub history_panel_open: bool,
    pub history_entries: Vec<crate::db::history::HistoryEntry>,
    pub history_selected: usize,
    pub tab_search_open: bool,
    pub tab_search_query: String,
    pub tab_search_selected: usize,
    pub bookmarks_panel_open: bool,
    pub bookmarks_entries: Vec<crate::db::bookmarks::Bookmark>,
    pub bookmarks_selected: usize,
    pub help_panel_open: bool,
    pub workspace_panel_open: bool,
    pub workspace_entries: Vec<crate::db::workspaces::Workspace>,
    pub workspace_selected: usize,
    pub site_settings_panel_open: bool,
    pub site_settings_zoom: Option<f64>,
    pub site_settings_js: Option<bool>,
    pub site_settings_cookies: Option<bool>,
    pub site_settings_adblock: Option<bool>,
    pub site_settings_url_pattern: String,
    /// Permission prompt dialog state.
    pub permission_prompt_open: bool,
    pub pending_permission_request: Option<PendingPermissionRequest>,
    /// Sync status panel state.
    pub sync_status_panel_open: bool,
    /// Sync conflicts panel state.
    pub sync_conflicts_panel_open: bool,
    pub sync_conflict_entries: Vec<SyncConflictEntry>,
    pub sync_conflict_selected: usize,
}

pub struct SessionState {
    pub should_quit: bool,
    pub session_dirty: bool,
    pub last_auto_save: std::time::Instant,
    marks: HashMap<Uuid, HashMap<char, f64>>,
    pending_mark_action: Option<char>,
    pub pending_mark_set: Option<char>,
    pub pending_mark_jump: Option<f64>,
    pub quickmarks: HashMap<String, String>,
}

pub struct TabState {
    pub closed_tab_stack: VecDeque<(String, String)>,
    pub tab_names: HashMap<Uuid, String>,
    pub muted_pane_ids: HashSet<Uuid>,
    pub pinned_pane_ids: HashSet<Uuid>,
    pub private_pane_ids: HashSet<Uuid>,
    pub reader_mode_panes: HashSet<Uuid>,
    pub minimal_mode_panes: HashSet<Uuid>,
    pub last_active_pane_id: Option<Uuid>,
    pub tab_display_cache: HashMap<Uuid, TabDisplayInfo>,
    pub tab_display_dirty: bool,
}

#[derive(Default)]
pub struct UiState {
    pub url_bar_focused: bool,
    pub url_bar_input: String,
    pub command_palette_input: String,
    pub find_bar_open: bool,
    pub find_query: String,
    pub hint_mode: bool,
    pub hint_new_tab: bool,
    pub hint_buffer: String,
    pub omnibox_results: Vec<SearchItem>,
    pub omnibox_selected: usize,
    pub last_omnibox_query: String,
    pub status_message: String,
    pub accessibility_text: String,
}

pub struct CacheState {
    pub cached_pane_count: usize,
    pub pane_count_dirty: bool,
    pub config_json_cache: String,
    pub config_json_dirty: bool,
    pub https_safe_list_cache: Option<HashSet<String>>,
    https_safe_list_debug_flag: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            should_quit: false,
            session_dirty: false,
            last_auto_save: std::time::Instant::now(),
            marks: HashMap::new(),
            pending_mark_action: None,
            pending_mark_set: None,
            pending_mark_jump: None,
            quickmarks: HashMap::new(),
        }
    }
}

impl Default for TabState {
    fn default() -> Self {
        Self {
            closed_tab_stack: VecDeque::new(),
            tab_names: HashMap::new(),
            muted_pane_ids: HashSet::new(),
            pinned_pane_ids: HashSet::new(),
            private_pane_ids: HashSet::new(),
            reader_mode_panes: HashSet::new(),
            minimal_mode_panes: HashSet::new(),
            last_active_pane_id: None,
            tab_display_cache: HashMap::new(),
            tab_display_dirty: true,
        }
    }
}

impl Default for CacheState {
    fn default() -> Self {
        Self {
            cached_pane_count: 0,
            pane_count_dirty: true,
            config_json_cache: String::new(),
            config_json_dirty: true,
            https_safe_list_cache: None,
            https_safe_list_debug_flag: false,
        }
    }
}

pub struct AppState {
    pub wm: BspTree,
    pub mode: Mode,
    pub keybindings: KeybindingRegistry,
    pub db: Option<rusqlite::Connection>,

    /// Web engine manager — one engine instance per pane.
    pub engines: PaneStateManager,

    /// Command palette state.
    pub palette: CommandPalette,

    /// Lua scripting engine (for init.lua and custom keybindings).
    #[cfg(feature = "lua")]
    lua_engine: Option<LuaEngine>,

    /// User configuration.
    pub config: Config,

    /// Queue of pending wry actions requested by the user.
    /// Consumed by main.rs each frame to drive the actual wry pane.
    /// Uses a queue so multiple actions per frame are not silently dropped.
    pub pending_wry_actions: VecDeque<WryAction>,

    /// Workspace name requested for restore. Set by `:ws-load <name>`.
    /// Consumed by main.rs which rebuilds the wry panes.
    pub pending_workspace_restore: Option<String>,

    /// Name of the currently active workspace. Displayed in status bar.
    /// Updated on workspace save, load, and restore.
    pub current_workspace_name: String,

    /// Set of pane IDs that should be terminal panes (not web panes).
    /// main.rs checks this when creating wry panes and uses the terminal
    /// custom protocol + IPC handler instead of regular web navigation.
    #[cfg(feature = "terminal")]
    pub terminal_pane_ids: HashSet<Uuid>,

    /// Bitwarden password manager client.
    #[cfg(feature = "passwords")]
    pub bitwarden: BitwardenClient,

    /// Command to auto-type into the next terminal pane that gets created.
    #[cfg(feature = "terminal")]
    pub pending_terminal_command: Option<String>,

    /// Pane ID pending close from tab sidebar click.
    /// Consumed by main.rs in about_to_wait.
    pub pending_tab_close: Option<Uuid>,

    /// When true, the next about_to_wait iteration requests a new popup window.
    pub pending_new_window: bool,

    /// URL to navigate a popup window to after creation (from pane detach).
    pub pending_detach_url: Option<url::Url>,

    /// Per-pane last-focus timestamp for LRU tab unloading.
    /// Updated each time a pane becomes active.
    pane_last_focus: HashMap<Uuid, std::time::Instant>,

    /// Tracks key-to-frame latency for profiling.
    pub input_latency: crate::profiling::InputLatencyTracker,

    /// Frame timing profiler — collects per-phase duration samples.
    /// Exposed on AppState so `:stats` command can read stats.
    pub profiler: crate::profiling::Profiler,

    /// Adblock blocked request count (updated by main.rs each frame).
    pub adblock_blocked_count: u64,

    /// Extension manager — loads and manages WebExtensions.
    /// Wrapped in Arc<RwLock<>> so readers don't block each other.
    pub extension_manager: Arc<parking_lot::RwLock<ExtensionManager>>,

    /// Sync filesystem watcher (started/stopped by sync commands).
    #[cfg(feature = "sync")]
    pub sync_watcher: crate::sync::watcher::SyncWatcher,

    /// Download manager — handles file downloads with progress tracking.
    pub download_manager: crate::downloads::DownloadManager,

    /// ARP server — Aileron Remote Protocol for mobile clients.
    /// Created on demand via `:arp-start` command.
    #[cfg(feature = "arp")]
    pub arp_server: Option<crate::arp::ArpServer>,

    /// ARP command receiver — polled each frame to process mobile mutations.
    /// Stored separately because it must not be dropped while the server runs.
    #[cfg(feature = "arp")]
    pub arp_cmd_receiver:
        Option<std::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<crate::arp::ArpCommand>>>,

    /// Pending bookmark import: "firefox" or "chrome".
    pub pending_import: Option<String>,

    /// Pending URL to open in a new tab (set by `:g <url>` command).
    pub pending_new_tab_url: Option<url::Url>,

    /// Per-pane tracking of already-injected content script IDs.
    /// Keys are pane IDs, values are sets of "extension_id:script_id" strings.
    /// Cleared on each LoadStarted to allow re-injection on new navigations.
    pub injected_content_script_ids: HashMap<Uuid, HashSet<String>>,

    pub ui: UiState,
    pub panels: PanelState,
    pub tabs: TabState,
    pub session: SessionState,
    pub crash: CrashRecoveryState,
    pub autofill: AutofillState,
    pub cache: CacheState,
}

impl AppState {
    #[must_use = "ignoring this value may lead to data loss or unexpected behavior"]
    pub fn new(viewport: Rect, config: Config) -> Result<Self> {
        // Use homepage from config
        let initial_url = url::Url::parse(&config.homepage)
            .unwrap_or_else(|_| url::Url::parse("aileron://welcome").unwrap());
        let wm = BspTree::new(viewport, initial_url.clone());
        let mode = Mode::Normal;
        let mut keybindings = KeybindingRegistry::default();

        // Apply custom keybinding overrides from config
        if !config.keybindings.is_empty() {
            let applied = keybindings.apply_config_overrides(&config.keybindings);
            if applied > 0 {
                info!("Applied {} custom keybinding(s)", applied);
            }
        }

        let db_path = Self::db_path()?;
        let db = match std::fs::create_dir_all(
            db_path
                .parent()
                .expect("db_path must have a parent directory"),
        ) {
            Ok(_) => match crate::db::open_database(&db_path) {
                Ok(conn) => Some(conn),
                Err(e) => {
                    warn!("Failed to open database: {}", e);
                    None
                }
            },
            Err(e) => {
                warn!("Failed to create database directory: {}", e);
                None
            }
        };

        // Create web engine manager with placeholder factory
        // (will be replaced with Servo when available per ADR-001)
        let mut engines = PaneStateManager::new();
        let root_pane_id = wm.active_pane_id();
        engines.create_pane(root_pane_id, initial_url, None);

        // Build command palette with history + bookmarks from DB
        let mut palette = CommandPalette::new();
        if let Some(ref conn) = db {
            // History items
            if let Ok(entries) = crate::db::history::recent_entries(conn, 50) {
                for entry in entries {
                    palette.add_item(SearchItem {
                        id: format!("history:{}", entry.id),
                        label: entry.title.clone(),
                        description: entry.url.clone(),
                        category: SearchCategory::History,
                    });
                }
            }
            // Bookmark items
            if let Ok(bm_list) = bookmarks::all_bookmarks(conn) {
                for bm in bm_list {
                    palette.add_item(SearchItem {
                        id: format!("bookmark:{}", bm.id),
                        label: bm.title.clone(),
                        description: bm.url.clone(),
                        category: SearchCategory::Bookmark,
                    });
                }
            }
        }

        // Initialize Lua engine and load init.lua if present
        #[cfg(feature = "lua")]
        let lua_engine = match LuaEngine::new() {
            Ok(engine) => {
                let init_lua = config.init_lua_path();
                if init_lua.exists() {
                    match engine.load_file(&init_lua) {
                        Ok(()) => info!("Loaded init.lua from {}", init_lua.display()),
                        Err(e) => warn!("Failed to load init.lua: {}", e),
                    }
                } else {
                    info!("No init.lua found at {}", init_lua.display());
                }
                // Apply any custom keybindings from Lua
                let pending = engine.take_pending_keybinds();
                for bind in &pending {
                    if let Some(combo) = LuaEngine::parse_key_string(&bind.mode, &bind.key) {
                        if let Some(action) = LuaEngine::resolve_action(&bind.action) {
                            info!("Lua keybind: {} {} -> {:?}", bind.mode, bind.key, action);
                            keybindings.register(combo, action);
                        } else {
                            warn!("Lua keybind: unknown action '{}'", bind.action);
                        }
                    } else {
                        warn!("Lua keybind: failed to parse key '{}'", bind.key);
                    }
                }

                // Populate palette with custom Lua commands
                for cmd in engine.custom_commands() {
                    palette.add_item(SearchItem {
                        id: format!("custom:{}", cmd.name),
                        label: cmd.name.clone(),
                        description: cmd.description,
                        category: SearchCategory::Custom,
                    });
                }

                Some(engine)
            }
            Err(e) => {
                warn!("Failed to initialize Lua engine: {}", e);
                None
            }
        };

        #[cfg(not(feature = "lua"))]
        let _lua_engine: Option<std::convert::Infallible> = None;

        // Load quickmarks from database
        let mut quickmarks = if let Some(ref conn) = db {
            crate::db::quickmarks::load_quickmarks(conn).unwrap_or_default()
        } else {
            HashMap::new()
        };

        // Seed default quickmarks if none exist
        if quickmarks.is_empty() {
            let defaults: &[(&str, &str)] = &[
                ("gh", "https://github.com"),
                ("gl", "https://gitlab.com"),
                ("rd", "https://reddit.com"),
            ];
            if let Some(ref conn) = db {
                for &(key, url) in defaults {
                    if let Err(e) = crate::db::quickmarks::set_quickmark(conn, key, url) {
                        warn!(%e, "Failed to set default quickmark");
                    }
                    quickmarks.insert(key.to_string(), url.to_string());
                }
            } else {
                for &(key, url) in defaults {
                    quickmarks.insert(key.to_string(), url.to_string());
                }
            }
        }

        // Load tab names from database
        let tab_names = if let Some(ref conn) = db {
            crate::db::tab_names::load_tab_names(conn).unwrap_or_default()
        } else {
            HashMap::new()
        };

        // Create extension manager and inject into Lua engine
        let extension_manager = Arc::new(parking_lot::RwLock::new(ExtensionManager::new(
            Self::extensions_dir(),
        )));
        #[cfg(feature = "lua")]
        if let Some(ref engine) = lua_engine {
            engine.set_extension_manager(extension_manager.clone());
        }

        let session = SessionState {
            quickmarks,
            ..Default::default()
        };

        let tabs = TabState {
            tab_names,
            ..Default::default()
        };

        Ok(Self {
            wm,
            mode,
            keybindings,
            db,
            engines,
            palette,
            #[cfg(feature = "lua")]
            lua_engine,
            config,
            pending_wry_actions: VecDeque::new(),
            pending_workspace_restore: None,
            current_workspace_name: "default".into(),
            #[cfg(feature = "terminal")]
            terminal_pane_ids: HashSet::new(),
            #[cfg(feature = "passwords")]
            bitwarden: BitwardenClient::new(),
            #[cfg(feature = "terminal")]
            pending_terminal_command: None,
            pending_tab_close: None,
            pending_new_window: false,
            pending_detach_url: None,
            pane_last_focus: HashMap::new(),
            input_latency: crate::profiling::InputLatencyTracker::new(),
            profiler: {
                let mut p = crate::profiling::Profiler::new();
                p.enable();
                p
            },
            adblock_blocked_count: 0,
            extension_manager: extension_manager.clone(),
            #[cfg(feature = "sync")]
            sync_watcher: crate::sync::watcher::SyncWatcher::new(),
            download_manager: crate::downloads::DownloadManager::new(
                directories::UserDirs::new()
                    .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
                    .unwrap_or_else(|| std::path::PathBuf::from("./Downloads")),
            ),
            #[cfg(feature = "arp")]
            arp_server: None,
            #[cfg(feature = "arp")]
            arp_cmd_receiver: None,
            pending_import: None,
            pending_new_tab_url: None,
            injected_content_script_ids: HashMap::new(),
            ui: UiState::default(),
            panels: PanelState::default(),
            tabs,
            session,
            crash: CrashRecoveryState::default(),
            autofill: AutofillState::default(),
            cache: CacheState::default(),
        })
    }

    /// Drain pending navigations from the Lua engine and push them as WryAction::Navigate.
    /// Call this after hook callbacks or during frame processing to ensure Lua-initiated
    /// navigations are processed.
    #[cfg(feature = "lua")]
    pub fn drain_lua_navigations(&mut self) {
        if let Some(ref engine) = self.lua_engine {
            let navs = engine.take_pending_navigations();
            for url_str in navs {
                match url::Url::parse(&url_str) {
                    Ok(url) => {
                        self.pending_wry_actions.push_back(WryAction::Navigate(url));
                    }
                    Err(e) => {
                        warn!("Lua navigate: invalid URL '{}': {}", url_str, e);
                    }
                }
            }
        }
    }

    pub fn get_cached_https_safe_list(&mut self) -> HashSet<String> {
        let current_debug = std::env::var("AILERON_DEBUG").is_ok();
        if self.cache.https_safe_list_cache.is_some()
            && self.cache.https_safe_list_debug_flag == current_debug
        {
            return self
                .cache
                .https_safe_list_cache
                .clone()
                .expect("https safe list cache populated above");
        }
        let list = crate::net::privacy::load_https_safe_list();
        self.cache.https_safe_list_debug_flag = current_debug;
        self.cache.https_safe_list_cache = Some(list.clone());
        list
    }

    /// Store a scroll mark fraction for a pane. Called from the IPC handler
    /// when the webview reports its scroll position back to Rust.
    pub fn store_mark_fraction(&mut self, pane_id: uuid::Uuid, mark: char, fraction: f64) {
        self.session
            .marks
            .entry(pane_id)
            .or_default()
            .insert(mark, fraction);
    }

    /// Record that a pane was focused. Call when active pane changes.
    pub fn record_pane_focus(&mut self, pane_id: uuid::Uuid) {
        self.pane_last_focus
            .insert(pane_id, std::time::Instant::now());
    }

    /// Clear injected script tracking for a pane (called on LoadStarted).
    pub fn clear_injected_scripts(&mut self, pane_id: uuid::Uuid) {
        self.injected_content_script_ids.remove(&pane_id);
    }

    /// Check if a content script has already been injected for a pane.
    pub fn is_script_injected(&self, pane_id: uuid::Uuid, script_key: &str) -> bool {
        self.injected_content_script_ids
            .get(&pane_id)
            .map(|s| s.contains(script_key))
            .unwrap_or(false)
    }

    /// Record that a content script was injected for a pane.
    pub fn mark_script_injected(&mut self, pane_id: uuid::Uuid, script_key: &str) {
        self.injected_content_script_ids
            .entry(pane_id)
            .or_default()
            .insert(script_key.to_string());
    }

    /// Call each frame to track pane focus changes.
    /// Compares current active pane to last recorded and updates timestamps.
    pub fn update_pane_focus_tracking(&mut self) {
        let active_id = self.wm.active_pane_id();
        let now = std::time::Instant::now();
        self.pane_last_focus
            .entry(active_id)
            .and_modify(|t| {
                // Only update if not recently recorded (avoid thrashing)
                if now.duration_since(*t) > std::time::Duration::from_millis(100) {
                    *t = now;
                }
            })
            .or_insert(now);
    }

    /// Find the least-recently-focused pane (excluding the active pane).
    /// Returns None if there is only one pane.
    #[must_use = "ignoring this value may lead to data loss or unexpected behavior"]
    pub fn find_lru_pane(&self) -> Option<uuid::Uuid> {
        let active_id = self.wm.active_pane_id();
        let mut best: Option<(uuid::Uuid, std::time::Instant)> = None;
        for (id, instant) in &self.pane_last_focus {
            if *id != active_id && best.is_none_or(|(_, b)| *instant < b) {
                best = Some((*id, *instant));
            }
        }
        best.map(|(id, _)| id)
    }

    /// Clean up per-pane state when a pane is closed. Prevents memory leaks.
    pub fn cleanup_pane_state(&mut self, pane_id: &uuid::Uuid) {
        self.pane_last_focus.remove(pane_id);
        self.session.marks.remove(pane_id);
        self.tabs.tab_names.remove(pane_id);
        self.tabs.private_pane_ids.remove(pane_id);
    }

    /// Execute a command string (URL, keybind, etc.) from the command palette.
    /// Public wrapper so main.rs can call it for IME-driven Enter submission.
    pub fn execute_command_pub(&mut self, cmd: &str) {
        self.execute_command(cmd);
    }

    /// Look up a quickmark URL by its key string.
    #[must_use = "ignoring this value may lead to data loss or unexpected behavior"]
    pub fn quickmarks_get(&self, key: &str) -> Option<url::Url> {
        self.session
            .quickmarks
            .get(key)
            .and_then(|s| url::Url::parse(s).ok())
    }

    pub fn quickmarks_list(&self) -> Vec<(String, String)> {
        self.session
            .quickmarks
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    #[cfg(feature = "terminal")]
    pub fn is_terminal_pane(&self, id: &uuid::Uuid) -> bool {
        self.terminal_pane_ids.contains(id)
    }

    #[cfg(not(feature = "terminal"))]
    #[allow(dead_code)]
    pub fn is_terminal_pane(&self, _id: &uuid::Uuid) -> bool {
        false
    }

    #[cfg(feature = "terminal")]
    pub fn register_terminal_pane(&mut self, id: uuid::Uuid) {
        self.terminal_pane_ids.insert(id);
    }

    #[cfg(feature = "terminal")]
    pub fn unregister_terminal_pane(&mut self, id: &uuid::Uuid) {
        self.terminal_pane_ids.remove(id);
    }

    #[cfg(feature = "terminal")]
    pub fn terminal_pane_count(&self) -> usize {
        self.terminal_pane_ids.len()
    }

    #[cfg(not(feature = "terminal"))]
    #[allow(dead_code)]
    pub fn terminal_pane_count(&self) -> usize {
        0
    }

    /// Load persisted scroll marks from the database for a given URL into the
    /// in-memory pane marks. Called when a page finishes loading.
    pub fn load_scroll_marks_for_pane(&mut self, pane_id: uuid::Uuid, url: &str) {
        if let Some(ref conn) = self.db
            && let Ok(db_marks) = crate::db::scroll_marks::load_scroll_marks_for_url(conn, url)
            && !db_marks.is_empty()
        {
            self.session
                .marks
                .entry(pane_id)
                .or_default()
                .extend(db_marks);
        }
    }

    fn db_path() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("com", "aileron", "Aileron")
            .ok_or_else(|| anyhow::anyhow!("Failed to determine project directories"))?;
        let data_dir = dirs.data_dir().to_path_buf();
        Ok(data_dir.join("aileron.db"))
    }

    fn extensions_dir() -> PathBuf {
        directories::ProjectDirs::from("com", "aileron", "Aileron")
            .map(|dirs| dirs.data_dir().join("extensions"))
            .unwrap_or_else(|| PathBuf::from("./extensions"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Key;

    #[test]
    fn test_looks_like_url_with_scheme() {
        assert!(crate::app::cmd::util::looks_like_url("https://example.com"));
        assert!(crate::app::cmd::util::looks_like_url("http://example.com"));
        assert!(crate::app::cmd::util::looks_like_url("aileron://welcome"));
        assert!(crate::app::cmd::util::looks_like_url(
            "ftp://files.example.com"
        ));
    }

    #[test]
    fn test_looks_like_url_bare_domain() {
        assert!(crate::app::cmd::util::looks_like_url("example.com"));
        assert!(crate::app::cmd::util::looks_like_url("www.google.com"));
        assert!(crate::app::cmd::util::looks_like_url("rust-lang.org"));
        assert!(crate::app::cmd::util::looks_like_url(
            "sub.domain.example.org"
        ));
    }

    #[test]
    fn test_looks_like_url_rejects_non_urls() {
        assert!(!crate::app::cmd::util::looks_like_url("quit"));
        assert!(!crate::app::cmd::util::looks_like_url("vs"));
        assert!(!crate::app::cmd::util::looks_like_url(""));
        assert!(!crate::app::cmd::util::looks_like_url("hello world"));
        // "file.txt" looks like a domain (bare domain detection is intentionally permissive)
    }

    #[test]
    fn test_looks_like_url_bare_domain_with_path() {
        // Contains '/' so won't match bare domain rule, but doesn't have ://
        assert!(!crate::app::cmd::util::looks_like_url("example.com/path")); // no scheme
    }

    #[test]
    fn test_looks_like_url_edge_cases() {
        assert!(!crate::app::cmd::util::looks_like_url("a.b")); // TLD "b" is only 1 char
        assert!(!crate::app::cmd::util::looks_like_url(".com")); // starts with dot, first part empty
        assert!(!crate::app::cmd::util::looks_like_url("example.")); // trailing dot, last part empty
    }

    #[test]
    fn test_pending_wry_actions_queue_drains() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();
        assert!(state.pending_wry_actions.is_empty());

        state.pending_wry_actions.push_back(WryAction::Navigate(
            url::Url::parse("https://example.com").unwrap(),
        ));
        assert_eq!(state.pending_wry_actions.len(), 1);

        let action = state.pending_wry_actions.pop_front();
        assert!(action.is_some());
        assert!(state.pending_wry_actions.is_empty());
    }

    #[test]
    fn test_pending_wry_actions_queue_multiple() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();

        // Simulate two actions firing in one frame
        state
            .pending_wry_actions
            .push_back(WryAction::ScrollBy { x: 0.0, y: 120.0 });
        state
            .pending_wry_actions
            .push_back(WryAction::ScrollBy { x: 0.0, y: 120.0 });
        assert_eq!(state.pending_wry_actions.len(), 2);

        // Both should be consumable (not dropped)
        let _ = state.pending_wry_actions.pop_front();
        let _ = state.pending_wry_actions.pop_front();
        assert!(state.pending_wry_actions.is_empty());
    }

    #[test]
    fn test_command_chaining_quit() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();
        state.execute_command("quit && open example.com");
        assert!(state.session.should_quit);
    }

    #[test]
    fn test_command_chaining_triple() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();
        state.handle_raw_command("vs && sp && swap");
        // vs and sp should have created splits; swap should show "No previous pane"
        assert_eq!(state.ui.status_message, "No previous pane");
    }

    #[test]
    fn test_mark_set_and_query() {
        use crate::input::mode::{KeyEvent, Modifiers};

        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();

        // Press 'm' to enter mark set mode
        state.process_key_event(KeyEvent {
            key: Key::Character('m'),
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert_eq!(state.session.pending_mark_action, Some('s'));

        // Press 'a' to set mark a
        state.process_key_event(KeyEvent {
            key: Key::Character('a'),
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert!(state.session.pending_mark_action.is_none());
        assert_eq!(state.ui.status_message, "Mark a set");

        // The mark is stored asynchronously via IPC. Verify the pending state
        // and that a CaptureScrollFraction action was queued.
        assert_eq!(state.session.pending_mark_set, Some('a'));
        assert!(
            state
                .pending_wry_actions
                .iter()
                .any(|a| matches!(a, WryAction::CaptureScrollFraction))
        );
    }

    #[test]
    fn test_mark_goto_nonexistent() {
        use crate::input::mode::{KeyEvent, Modifiers};

        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();

        // Press '\'' to enter mark goto mode
        state.process_key_event(KeyEvent {
            key: Key::Character('\''),
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert_eq!(state.session.pending_mark_action, Some('g'));

        // Press 'z' (not set)
        state.process_key_event(KeyEvent {
            key: Key::Character('z'),
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert_eq!(state.ui.status_message, "Mark z not set");
    }

    #[test]
    fn test_mark_prefix_cancels_on_non_letter() {
        use crate::input::mode::{KeyEvent, Modifiers};

        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();

        // Press 'm' to enter mark set mode
        state.process_key_event(KeyEvent {
            key: Key::Character('m'),
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert_eq!(state.session.pending_mark_action, Some('s'));

        // Press Escape to cancel
        state.process_key_event(KeyEvent {
            key: Key::Escape,
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert!(state.session.pending_mark_action.is_none());
    }

    #[test]
    fn test_swap_no_previous_pane() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();
        state.execute_command("swap");
        assert_eq!(state.ui.status_message, "No previous pane");
    }
}
