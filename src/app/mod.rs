use anyhow::Result;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

pub mod cmd;
pub mod commands;
pub mod dispatch;
pub mod events;
pub mod omnibox;
pub mod palette;

use crate::config::Config;
use crate::db::bookmarks;
use crate::extensions::ExtensionManager;
use crate::input::{KeybindingRegistry, Mode};
use crate::lua::LuaEngine;
use crate::passwords::BitwardenClient;
use crate::servo::PaneStateManager;
use crate::ui::palette::CommandPalette;
use crate::ui::search::SearchCategory;
use crate::ui::search::SearchItem;
use crate::wm::{BspTree, Rect};

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

pub struct AppState {
    pub wm: BspTree,
    pub mode: Mode,
    pub keybindings: KeybindingRegistry,
    pub should_quit: bool,
    pub command_palette_input: String,
    /// Find-in-page bar state.
    pub find_bar_open: bool,
    pub find_query: String,
    /// URL bar editing state.
    pub url_bar_focused: bool,
    pub url_bar_input: String,
    /// Omnibox dropdown results (shown when URL bar is focused and has input).
    pub omnibox_results: Vec<crate::ui::SearchItem>,
    /// Index of the selected omnibox result (for keyboard navigation).
    pub omnibox_selected: usize,
    /// Last query used for omnibox update (to avoid redundant recomputation).
    pub last_omnibox_query: String,
    /// Whether link hint mode is active (digits are captured to follow links).
    pub hint_mode: bool,
    /// Whether hints should open links in new tabs (F key) vs navigate (f key).
    pub hint_new_tab: bool,
    /// Buffer for accumulating hint digits while in link hint mode.
    pub hint_buffer: String,
    pub db: Option<rusqlite::Connection>,
    pub status_message: String,

    /// Web engine manager — one engine instance per pane.
    pub engines: PaneStateManager,

    /// Per-pane mode: which panes are in reader mode.
    pub reader_mode_panes: std::collections::HashSet<uuid::Uuid>,
    /// Per-pane mode: which panes are in minimal mode.
    pub minimal_mode_panes: std::collections::HashSet<uuid::Uuid>,

    /// Command palette state.
    pub palette: CommandPalette,

    /// Lua scripting engine (for init.lua and custom keybindings).
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
    pub terminal_pane_ids: std::collections::HashSet<uuid::Uuid>,

    /// Bitwarden password manager client.
    pub bitwarden: BitwardenClient,

    /// Command to auto-type into the next terminal pane that gets created.
    pub pending_terminal_command: Option<String>,

    /// Pane ID pending close from tab sidebar click.
    /// Consumed by main.rs in about_to_wait.
    pub pending_tab_close: Option<uuid::Uuid>,

    /// When true, the next about_to_wait iteration requests a new popup window.
    pub pending_new_window: bool,

    /// URL to navigate a popup window to after creation (from pane detach).
    pub pending_detach_url: Option<url::Url>,

    /// Quickmarks — single-letter bookmarks mapping to URLs.
    quickmarks: std::collections::HashMap<char, String>,

    /// Per-pane scroll marks. Maps pane_id → letter → scroll fraction (0.0-1.0).
    marks: std::collections::HashMap<uuid::Uuid, std::collections::HashMap<char, f64>>,

    /// Pending mark action: Some('s') means "waiting for mark letter to set",
    /// Some('g') means "waiting for mark letter to go to".
    pending_mark_action: Option<char>,

    /// Pending mark-set letter. Set when user presses `m` then a letter.
    /// The JS callback will store the actual scroll fraction once received.
    pub pending_mark_set: Option<char>,

    /// Pending scroll-to-mark fraction. Set when user jumps to a mark.
    /// The render loop consumes this to scroll the webview.
    pub pending_mark_jump: Option<f64>,

    /// ID of the previously active pane, for tab-swap.
    last_active_pane_id: Option<uuid::Uuid>,

    /// Per-pane last-focus timestamp for LRU tab unloading.
    /// Updated each time a pane becomes active.
    pane_last_focus: std::collections::HashMap<uuid::Uuid, std::time::Instant>,

    /// Timestamp of last auto-save. Used for debouncing.
    pub last_auto_save: std::time::Instant,

    /// Whether the user has interacted with this session.
    /// Prevents auto-saving a fresh session (just the homepage).
    pub session_dirty: bool,

    /// Tracks key-to-frame latency for profiling.
    pub input_latency: crate::profiling::InputLatencyTracker,

    /// Set of pane IDs that are muted (media paused + muted).
    pub muted_pane_ids: std::collections::HashSet<uuid::Uuid>,

    /// Set of pane IDs that are pinned (cannot be closed).
    pub pinned_pane_ids: std::collections::HashSet<uuid::Uuid>,
    /// Set of pane IDs in private/incognito mode (no history saved).
    pub private_pane_ids: std::collections::HashSet<uuid::Uuid>,
    /// Custom tab names keyed by pane ID string.
    pub tab_names: std::collections::HashMap<String, String>,

    /// Adblock blocked request count (updated by main.rs each frame).
    pub adblock_blocked_count: u64,

    /// Extension manager — loads and manages WebExtensions.
    /// Wrapped in Arc<Mutex<>> so the Lua engine can share access.
    pub extension_manager: Arc<Mutex<ExtensionManager>>,

    /// Sync filesystem watcher (started/stopped by sync commands).
    pub sync_watcher: crate::sync::watcher::SyncWatcher,

    /// Download manager — handles file downloads with progress tracking.
    pub download_manager: crate::downloads::DownloadManager,

    /// ARP server — Aileron Remote Protocol for mobile clients.
    /// Created on demand via `:arp-start` command.
    pub arp_server: Option<crate::arp::ArpServer>,

    /// ARP command receiver — polled each frame to process mobile mutations.
    /// Stored separately because it must not be dropped while the server runs.
    pub arp_cmd_receiver:
        Option<std::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<crate::arp::ArpCommand>>>,

    /// Whether the history panel overlay is open.
    pub history_panel_open: bool,

    /// Cached history entries for the history panel.
    pub history_entries: Vec<crate::db::history::HistoryEntry>,

    /// Selected index in the history panel (for j/k navigation).
    pub history_selected: usize,

    /// Whether the tab search panel overlay is open.
    pub tab_search_open: bool,

    /// Filter query for the tab search panel.
    pub tab_search_query: String,

    /// Selected index in the tab search panel (for j/k navigation).
    pub tab_search_selected: usize,

    /// Stack of recently closed tabs for :tab-restore.
    /// Each entry is (url, title).
    pub closed_tab_stack: std::collections::VecDeque<(String, String)>,

    /// Whether the bookmarks panel overlay is open.
    pub bookmarks_panel_open: bool,

    /// Cached bookmarks for the bookmarks panel.
    pub bookmarks_entries: Vec<crate::db::bookmarks::Bookmark>,

    /// Selected index in the bookmarks panel (for j/k navigation).
    pub bookmarks_selected: usize,

    /// Whether the help panel overlay is open.
    pub help_panel_open: bool,

    /// Whether the per-site settings panel is open.
    pub site_settings_panel_open: bool,

    /// Current per-site settings values (loaded from DB when panel opens).
    pub site_settings_zoom: Option<f64>,
    pub site_settings_js: Option<bool>,
    pub site_settings_cookies: Option<bool>,
    pub site_settings_adblock: Option<bool>,
    pub site_settings_url_pattern: String,

    /// Whether a webview crash was detected this frame (for recovery UI).
    pub webview_crash_detected: bool,

    /// URL of the pane that crashed (for reload recovery).
    pub crashed_pane_url: Option<String>,

    /// Pending bookmark import: "firefox" or "chrome".
    pub pending_import: Option<String>,

    /// ID of the pane that crashed.
    pub crashed_pane_id: Option<uuid::Uuid>,

    /// Pending URL to open in a new tab (set by :g <url> command).
    pub pending_new_tab_url: Option<url::Url>,

    /// Whether auto-fill is available for the current page.
    /// Set to true when a login form is detected and Bitwarden has credentials.
    pub autofill_available: bool,

    /// Username field ID detected on the current page (for getElementById fill).
    pub autofill_username_id: String,

    /// Password field ID detected on the current page (for getElementById fill).
    pub autofill_password_id: String,

    /// Pre-computed JS to inject when user triggers auto-fill.
    /// Generated when autofill_available is set to avoid blocking the UI.
    pub autofill_js: Option<String>,

    /// Status message to display after auto-fill is triggered.
    pub autofill_status_msg: String,

    /// Per-pane tracking of already-injected content script IDs.
    /// Keys are pane IDs, values are sets of "extension_id:script_id" strings.
    /// Cleared on each LoadStarted to allow re-injection on new navigations.
    pub injected_content_script_ids:
        std::collections::HashMap<uuid::Uuid, std::collections::HashSet<String>>,

    /// Cached tab display info to avoid per-frame title/url string allocations.
    pub tab_display_cache: std::collections::HashMap<uuid::Uuid, TabDisplayInfo>,
    /// Whether the tab display cache needs recomputation.
    pub tab_display_dirty: bool,

    /// Cached config JSON for get-config IPC (avoids per-message serialization).
    pub config_json_cache: String,
    /// Whether the config JSON cache needs recomputation.
    pub config_json_dirty: bool,

    /// Cached pane leaf count for status bar (avoids per-frame tree traversal).
    pub cached_pane_count: usize,
    /// Whether the cached pane count needs recomputation.
    pub pane_count_dirty: bool,

    /// Cached HTTPS safe list (avoids re-reading from disk on every pane creation).
    pub https_safe_list_cache: Option<std::collections::HashSet<String>>,
    /// Tracks whether AILERON_DEBUG was set when the cache was populated.
    https_safe_list_debug_flag: bool,

    /// Accessibility live-region text summarizing current state for screen readers.
    /// Updated on important state changes (mode, URL, pane close, navigation, error).
    pub accessibility_text: String,
}

impl AppState {
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
        let should_quit = false;
        let command_palette_input = String::new();
        let find_bar_open = false;
        let find_query = String::new();
        let url_bar_focused = false;
        let url_bar_input = String::new();
        let hint_mode = false;
        let hint_new_tab = false;
        let hint_buffer = String::new();

        let db_path = Self::db_path()?;
        let db = match std::fs::create_dir_all(db_path.parent().unwrap()) {
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

        // Load quickmarks from database
        let quickmarks = if let Some(ref conn) = db {
            crate::db::quickmarks::load_quickmarks(conn).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

        // Load tab names from database
        let tab_names = if let Some(ref conn) = db {
            crate::db::tab_names::load_tab_names(conn).unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

        // Create extension manager and inject into Lua engine
        let extension_manager = Arc::new(Mutex::new(ExtensionManager::new(Self::extensions_dir())));
        if let Some(ref engine) = lua_engine {
            engine.set_extension_manager(extension_manager.clone());
        }

        Ok(Self {
            wm,
            mode,
            keybindings,
            should_quit,
            command_palette_input,
            find_bar_open,
            find_query,
            url_bar_focused,
            url_bar_input,
            omnibox_results: Vec::new(),
            omnibox_selected: 0,
            last_omnibox_query: String::new(),
            hint_mode,
            hint_new_tab,
            hint_buffer,
            db,
            status_message: String::new(),
            engines,
            reader_mode_panes: std::collections::HashSet::new(),
            minimal_mode_panes: std::collections::HashSet::new(),
            palette,
            lua_engine,
            config,
            pending_wry_actions: VecDeque::new(),
            pending_workspace_restore: None,
            current_workspace_name: "default".into(),
            terminal_pane_ids: std::collections::HashSet::new(),
            bitwarden: BitwardenClient::new(),
            pending_terminal_command: None,
            pending_tab_close: None,
            pending_new_window: false,
            pending_detach_url: None,
            quickmarks,
            marks: std::collections::HashMap::new(),
            pending_mark_action: None,
            pending_mark_set: None,
            pending_mark_jump: None,
            last_active_pane_id: None,
            pane_last_focus: std::collections::HashMap::new(),
            last_auto_save: std::time::Instant::now(),
            session_dirty: false,
            input_latency: crate::profiling::InputLatencyTracker::new(),
            muted_pane_ids: std::collections::HashSet::new(),
            pinned_pane_ids: std::collections::HashSet::new(),
            private_pane_ids: std::collections::HashSet::new(),
            tab_names,
            adblock_blocked_count: 0,
            extension_manager: extension_manager.clone(),
            sync_watcher: crate::sync::watcher::SyncWatcher::new(),
            download_manager: crate::downloads::DownloadManager::new(
                directories::UserDirs::new()
                    .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
                    .unwrap_or_else(|| std::path::PathBuf::from("./Downloads")),
            ),
            arp_server: None,
            arp_cmd_receiver: None,
            history_panel_open: false,
            history_entries: Vec::new(),
            history_selected: 0,
            tab_search_open: false,
            tab_search_query: String::new(),
            tab_search_selected: 0,
            closed_tab_stack: std::collections::VecDeque::new(),
            bookmarks_panel_open: false,
            bookmarks_entries: Vec::new(),
            bookmarks_selected: 0,
            help_panel_open: false,
            site_settings_panel_open: false,
            site_settings_zoom: None,
            site_settings_js: None,
            site_settings_cookies: None,
            site_settings_adblock: None,
            site_settings_url_pattern: String::new(),
            webview_crash_detected: false,
            crashed_pane_url: None,
            pending_import: None,
            crashed_pane_id: None,
            pending_new_tab_url: None,
            autofill_available: false,
            autofill_username_id: String::new(),
            autofill_password_id: String::new(),
            autofill_js: None,
            autofill_status_msg: String::new(),
            injected_content_script_ids: std::collections::HashMap::new(),
            tab_display_cache: std::collections::HashMap::new(),
            tab_display_dirty: true,
            config_json_cache: String::new(),
            config_json_dirty: true,
            cached_pane_count: 0,
            pane_count_dirty: true,
            https_safe_list_cache: None,
            https_safe_list_debug_flag: false,
            accessibility_text: String::new(),
        })
    }

    pub fn get_cached_https_safe_list(&mut self) -> std::collections::HashSet<String> {
        let current_debug = std::env::var("AILERON_DEBUG").is_ok();
        if self.https_safe_list_cache.is_some() && self.https_safe_list_debug_flag == current_debug
        {
            return self.https_safe_list_cache.clone().unwrap();
        }
        let list = crate::net::privacy::load_https_safe_list();
        self.https_safe_list_debug_flag = current_debug;
        self.https_safe_list_cache = Some(list.clone());
        list
    }

    /// Store a scroll mark fraction for a pane. Called from the IPC handler
    /// when the webview reports its scroll position back to Rust.
    pub fn store_mark_fraction(&mut self, pane_id: uuid::Uuid, mark: char, fraction: f64) {
        self.marks
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
        self.marks.remove(pane_id);
        self.tab_names.remove(&pane_id.to_string());
        self.private_pane_ids.remove(pane_id);
    }

    /// Look up a quickmark URL by its key character.
    pub fn quickmarks_get(&self, key: &char) -> Option<url::Url> {
        self.quickmarks
            .get(key)
            .and_then(|s| url::Url::parse(s).ok())
    }

    /// Get all quickmarks as (key, url) pairs.
    pub fn quickmarks_list(&self) -> Vec<(char, String)> {
        self.quickmarks
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }

    /// Load persisted scroll marks from the database for a given URL into the
    /// in-memory pane marks. Called when a page finishes loading.
    pub fn load_scroll_marks_for_pane(&mut self, pane_id: uuid::Uuid, url: &str) {
        if let Some(ref conn) = self.db
            && let Ok(db_marks) = crate::db::scroll_marks::load_scroll_marks_for_url(conn, url)
            && !db_marks.is_empty()
        {
            self.marks.entry(pane_id).or_default().extend(db_marks);
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
        assert!(state.should_quit);
    }

    #[test]
    fn test_command_chaining_triple() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();
        state.handle_raw_command("vs && sp && swap");
        // vs and sp should have created splits; swap should show "No previous pane"
        assert_eq!(state.status_message, "No previous pane");
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
        assert_eq!(state.pending_mark_action, Some('s'));

        // Press 'a' to set mark a
        state.process_key_event(KeyEvent {
            key: Key::Character('a'),
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert!(state.pending_mark_action.is_none());
        assert_eq!(state.status_message, "Mark a set");

        // The mark is stored asynchronously via IPC. Verify the pending state
        // and that a CaptureScrollFraction action was queued.
        assert_eq!(state.pending_mark_set, Some('a'));
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
        assert_eq!(state.pending_mark_action, Some('g'));

        // Press 'z' (not set)
        state.process_key_event(KeyEvent {
            key: Key::Character('z'),
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert_eq!(state.status_message, "Mark z not set");
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
        assert_eq!(state.pending_mark_action, Some('s'));

        // Press Escape to cancel
        state.process_key_event(KeyEvent {
            key: Key::Escape,
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert!(state.pending_mark_action.is_none());
    }

    #[test]
    fn test_swap_no_previous_pane() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();
        state.execute_command("swap");
        assert_eq!(state.status_message, "No previous pane");
    }
}
