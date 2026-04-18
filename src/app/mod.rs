use anyhow::Result;
use std::collections::VecDeque;
use std::path::PathBuf;
use tracing::{info, warn};

pub mod dispatch;

use crate::config::Config;
use crate::db::bookmarks;
use crate::input::{EventDestination, Key, KeyEvent, KeybindingRegistry, Mode};
use crate::lua::LuaEngine;
use crate::passwords::BitwardenClient;
use crate::servo::PaneStateManager;
use crate::ui::palette::{CommandPalette, PaletteAction};
use crate::ui::search::SearchCategory;
use crate::ui::search::SearchItem;
use crate::wm::{BspTree, Direction, Rect};

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
}

pub struct AppState {
    pub wm: BspTree,
    pub mode: Mode,
    pub keybindings: KeybindingRegistry,
    pub should_quit: bool,
    pub command_palette_open: bool,
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

    /// ID of the previously active pane, for tab-swap.
    last_active_pane_id: Option<uuid::Uuid>,

    /// Timestamp of last auto-save. Used for debouncing.
    pub last_auto_save: std::time::Instant,

    /// Whether the user has interacted with this session.
    /// Prevents auto-saving a fresh session (just the homepage).
    pub session_dirty: bool,

    /// Set of pane IDs that are muted (media paused + muted).
    pub muted_pane_ids: std::collections::HashSet<uuid::Uuid>,

    /// Set of pane IDs that are pinned (cannot be closed).
    pub pinned_pane_ids: std::collections::HashSet<uuid::Uuid>,

    /// Adblock blocked request count (updated by main.rs each frame).
    pub adblock_blocked_count: u64,
}

impl AppState {
    pub fn new(viewport: Rect, config: Config) -> Result<Self> {
        // Use homepage from config
        let initial_url = url::Url::parse(&config.homepage)
            .unwrap_or_else(|_| url::Url::parse("aileron://welcome").unwrap());
        let wm = BspTree::new(viewport, initial_url.clone());
        let mode = Mode::Normal;
        let mut keybindings = KeybindingRegistry::default();
        let should_quit = false;
        let command_palette_open = false;
        let command_palette_input = String::new();
        let find_bar_open = false;
        let find_query = String::new();
        let url_bar_focused = false;
        let url_bar_input = String::new();
        let hint_mode = false;
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
        engines.create_pane(root_pane_id, initial_url);

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

        Ok(Self {
            wm,
            mode,
            keybindings,
            should_quit,
            command_palette_open,
            command_palette_input,
            find_bar_open,
            find_query,
            url_bar_focused,
            url_bar_input,
            omnibox_results: Vec::new(),
            omnibox_selected: 0,
            last_omnibox_query: String::new(),
            hint_mode,
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
            terminal_pane_ids: std::collections::HashSet::new(),
            bitwarden: BitwardenClient::new(),
            pending_terminal_command: None,
            pending_tab_close: None,
            pending_new_window: false,
            pending_detach_url: None,
            quickmarks: std::collections::HashMap::new(),
            marks: std::collections::HashMap::new(),
            pending_mark_action: None,
            last_active_pane_id: None,
            last_auto_save: std::time::Instant::now(),
            session_dirty: false,
            muted_pane_ids: std::collections::HashSet::new(),
            pinned_pane_ids: std::collections::HashSet::new(),
            adblock_blocked_count: 0,
        })
    }

    fn db_path() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("com", "aileron", "Aileron")
            .ok_or_else(|| anyhow::anyhow!("Failed to determine project directories"))?;
        let data_dir = dirs.data_dir().to_path_buf();
        Ok(data_dir.join("aileron.db"))
    }

    /// Refresh the command palette with latest history items from the DB
    /// and open pane URLs from the engine manager.
    pub fn refresh_palette_items(&mut self) {
        self.palette.clear_items();
        if let Some(ref conn) = self.db {
            if let Ok(entries) = crate::db::history::recent_entries(conn, 50) {
                for entry in entries {
                    self.palette.add_item(SearchItem {
                        id: format!("history:{}", entry.id),
                        label: entry.title.clone(),
                        description: entry.url.clone(),
                        category: SearchCategory::History,
                    });
                }
            }
            if let Ok(bm_list) = bookmarks::all_bookmarks(conn) {
                for bm in bm_list {
                    self.palette.add_item(SearchItem {
                        id: format!("bookmark:{}", bm.id),
                        label: bm.title.clone(),
                        description: bm.url.clone(),
                        category: SearchCategory::Bookmark,
                    });
                }
            }
        }

        // Add open pane URLs as switchable tabs
        let panes = self.wm.panes();
        for (pane_id, _rect) in &panes {
            let url_str = self
                .engines
                .get(pane_id)
                .and_then(|e| e.current_url().cloned())
                .map(|u| u.to_string())
                .unwrap_or_else(|| "aileron://new".into());
            let is_active = *pane_id == self.wm.active_pane_id();
            let label = if is_active {
                format!("● {}", url_str)
            } else {
                url_str.clone()
            };
            self.palette.add_item(SearchItem {
                id: format!("tab:{}", pane_id),
                label,
                description: url_str,
                category: SearchCategory::OpenTab,
            });
        }

        // Re-add custom Lua commands
        if let Some(ref engine) = self.lua_engine {
            for cmd in engine.custom_commands() {
                self.palette.add_item(SearchItem {
                    id: format!("custom:{}", cmd.name),
                    label: cmd.name.clone(),
                    description: cmd.description,
                    category: SearchCategory::Custom,
                });
            }
        }
    }

    pub fn process_key_event(&mut self, event: KeyEvent) {
        // If palette is open, route input to it
        if self.palette.open {
            let key_str: Option<String> = match &event.key {
                Key::Up => Some("Up".into()),
                Key::Down => Some("Down".into()),
                Key::Enter => Some("Enter".into()),
                Key::Escape => Some("Escape".into()),
                Key::Backspace => Some("Backspace".into()),
                Key::Character(c) => Some(c.to_string()),
                _ => None,
            };

            if let Some(key_str) = key_str {
                let action = self.palette.handle_input(&key_str);
                match action {
                    PaletteAction::ItemSelected(item) => {
                        self.command_palette_open = false;
                        self.command_palette_input.clear();
                        self.execute_palette_selection(&item);
                    }
                    PaletteAction::Closed => {
                        self.command_palette_open = false;
                        self.command_palette_input.clear();
                    }
                    PaletteAction::QuerySubmit(query) => {
                        self.command_palette_open = false;
                        self.command_palette_input.clear();
                        self.handle_raw_command(&query);
                    }
                    PaletteAction::Consumed => {
                        self.command_palette_input = self.palette.query.clone();
                    }
                }
            }
            return;
        }

        // Handle pending mark actions (m or ' prefix)
        if let Some(action) = self.pending_mark_action.take()
            && let Key::Character(c) = &event.key
            && c.is_ascii_lowercase()
        {
            let active_id = self.wm.active_pane_id();
            match action {
                's' => {
                    self.marks.entry(active_id).or_default().insert(*c, 0.5);
                    self.status_message = format!("Mark {} set", c);
                }
                'g' => {
                    if self
                        .marks
                        .get(&active_id)
                        .and_then(|m| m.get(c))
                        .is_some()
                    {
                        self.status_message = format!("Mark {} jumped", c);
                    } else {
                        self.status_message = format!("Mark {} not set", c);
                    }
                }
                _ => {}
            }
            return;
        }

        // Check keybindings first
        let action = self
            .keybindings
            .lookup(self.mode, event.modifiers, event.key.clone())
            .cloned();
        if let Some(action) = action {
            self.execute_action(&action);
            return;
        }

        // Mark prefix keys in Normal mode
        if self.mode == Mode::Normal {
            if let Key::Character('m') = &event.key {
                self.pending_mark_action = Some('s');
                self.status_message = "Set mark (press a-z)".into();
                return;
            } else if let Key::Character('\'') = &event.key {
                self.pending_mark_action = Some('g');
                self.status_message = "Go to mark (press a-z)".into();
                return;
            }
        }

        // Check mode transitions
        if let Some(new_mode) = crate::input::mode::transition(self.mode, &event) {
            self.mode = new_mode;
            self.update_status();
            if let Some(ref engine) = self.lua_engine {
                engine.call_hooks("mode_change", &[self.mode.as_str()]);
            }
            return;
        }

        // Route to destination
        let dest = crate::input::router::route_event(self.mode, &event);
        match dest {
            EventDestination::KeybindingHandler => {}
            EventDestination::Servo => {
                if let Key::Character(c) = &event.key {
                    tracing::debug!("Would send '{}' to Servo", c);
                }
            }
            EventDestination::CommandPalette => {
                if let Key::Character(c) = &event.key {
                    self.command_palette_input.push(*c);
                } else if event.key == Key::Backspace {
                    self.command_palette_input.pop();
                } else if event.key == Key::Enter {
                    let input = self.command_palette_input.clone();
                    self.execute_command(&input);
                    self.command_palette_open = false;
                    self.command_palette_input.clear();
                }
            }
            EventDestination::Egui => {}
            EventDestination::Discard => {}
        }
    }

    fn execute_action(&mut self, action: &crate::input::Action) {
        self.session_dirty = true;
        use dispatch::ActionEffect;

        let effects = dispatch::dispatch_action(action);

        for effect in &effects {
            match effect {
                ActionEffect::Wry(wry_action) => {
                    self.pending_wry_actions.push_back(wry_action.clone());
                }
                ActionEffect::Status(msg) => {
                    self.status_message = msg.clone();
                }
                ActionEffect::SetMode(mode) => {
                    self.mode = *mode;
                    self.update_status();
                    if let Some(ref engine) = self.lua_engine {
                        engine.call_hooks("mode_change", &[self.mode.as_str()]);
                    }
                }
                ActionEffect::Quit => {
                    info!("Quit requested");
                    self.should_quit = true;
                }
                ActionEffect::OpenPalette => {
                    // Refresh items before opening so recent history/bookmarks are current
                    self.refresh_palette_items();
                    self.palette.open();
                    self.command_palette_open = true;
                    self.command_palette_input.clear();
                    self.status_message = "Command palette".into();
                }
                ActionEffect::RequestSplit(direction) => {
                    let active = self.wm.active_pane_id();
                    let new_url = url::Url::parse("aileron://new").unwrap();
                    match self.wm.split(active, *direction, 0.5) {
                        Ok(new_id) => {
                            self.engines.create_pane(new_id, new_url);
                            self.status_message = "Split vertical".into();
                        }
                        Err(e) => self.status_message = format!("Split failed: {}", e),
                    }
                }
                ActionEffect::OpenTerminal => {
                    let active = self.wm.active_pane_id();
                    let term_url = url::Url::parse("aileron://terminal").unwrap();
                    match self.wm.split(active, crate::wm::SplitDirection::Vertical, 0.5) {
                        Ok(new_id) => {
                            self.engines.create_pane(new_id, term_url.clone());
                            self.terminal_pane_ids.insert(new_id);
                            self.status_message = "Terminal opened".into();
                        }
                        Err(e) => self.status_message = format!("Terminal failed: {}", e),
                    }
                }
                ActionEffect::RequestClosePane => {
                    let active = self.wm.active_pane_id();
                    if self.pinned_pane_ids.contains(&active) {
                        self.status_message = "Cannot close pinned pane (use :pin to unpin)".into();
                        return;
                    }
                    if let Ok(()) = self.wm.close(active) {
                        self.engines.remove_pane(&active);
                        self.status_message = "Pane closed".into();
                    }
                }
                ActionEffect::RequestNavigatePane(direction) => {
                    let current = self.wm.active_pane_id();
                    if let Some(id) = self.wm.navigate(*direction) {
                        self.last_active_pane_id = Some(current);
                        self.wm.set_active_pane(id);
                        self.update_status();
                    }
                }
                ActionEffect::RequestExternalBrowser => {
                    let active_id = self.wm.active_pane_id();
                    if let Some(engine) = self.engines.get(&active_id)
                        && let Some(url) = engine.current_url() {
                            match crate::servo::open_in_system_browser(url) {
                                Ok(()) => {
                                    self.status_message = "Opened in system browser".into();
                                }
                                Err(e) => {
                                    self.status_message = format!("Failed: {}", e);
                                }
                            }
                        }
                }
                ActionEffect::OpenFindBar => {
                    self.find_bar_open = true;
                    self.find_query.clear();
                    self.status_message = "Find: ".into();
                }
                ActionEffect::CloseFindBar => {
                    self.find_bar_open = false;
                    self.find_query.clear();
                    // Clear highlights in the page
                    self.pending_wry_actions.push_back(WryAction::RunJs(
                        "window.getSelection().removeAllRanges()".into(),
                    ));
                }
                ActionEffect::FindInPage { query, forward } => {
                    if !query.is_empty() {
                        let direction = if *forward { "true" } else { "false" };
                        let escaped = query.replace('\\', "\\\\").replace('\'', "\\'");
                        self.pending_wry_actions.push_back(WryAction::RunJs(format!(
                            "window.find('{}', false, true, {}, false, false, false)",
                            escaped, direction
                        )));
                    }
                }
                ActionEffect::ToggleLinkHints => {
                    self.hint_mode = !self.hint_mode;
                    if self.hint_mode {
                        self.status_message = "Link hints: type number, Escape to cancel".into();
                    } else {
                        self.status_message.clear();
                    }
                    // Wry(RunJs) effect is also dispatched to inject/remove the CSS
                }
                ActionEffect::SaveWorkspace => {
                    // Queue a save action for main.rs to handle.
                    // main.rs has access to WryPaneManager for live URLs.
                    let name =
                        format!("workspace-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
                    self.pending_wry_actions
                        .push_back(WryAction::SaveWorkspace {
                            name: name.clone(),
                            pane_urls: std::collections::HashMap::new(),
                        });
                    self.status_message = format!("Saving workspace: {}...", name);
                }
                ActionEffect::CopyUrl => {
                    let active_id = self.wm.active_pane_id();
                    if let Some(engine) = self.engines.get(&active_id)
                        && let Some(url) = engine.current_url()
                    {
                        let url_str = url.to_string();
                        let copied = std::process::Command::new("wl-copy")
                            .arg(&url_str)
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status()
                            .ok()
                            .map(|s| s.success())
                            .unwrap_or(false)
                            || std::process::Command::new("xclip")
                                .args(["-selection", "clipboard"])
                                .arg(&url_str)
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .status()
                                .ok()
                                .map(|s| s.success())
                                .unwrap_or(false);
                        if copied {
                            let display = if url_str.len() > 60 {
                                format!("{}...", &url_str[..57])
                            } else {
                                url_str
                            };
                            self.status_message = format!("Copied: {}", display);
                        } else {
                            self.status_message =
                                "Clipboard: install wl-clipboard or xclip".into();
                        }
                    }
                }
                ActionEffect::ResizePane(direction) => {
                    let active = self.wm.active_pane_id();
                    let amount = match direction {
                        Direction::Left | Direction::Up => -0.05,
                        Direction::Right | Direction::Down => 0.05,
                    };
                    match self.wm.resize_pane(active, amount) {
                        Ok(()) => self.status_message = "Pane resized".into(),
                        Err(e) => self.status_message = format!("Resize failed: {}", e),
                    }
                }
                ActionEffect::NewWindow => {
                    self.pending_new_window = true;
                    self.status_message = "Opening new window...".into();
                }
                ActionEffect::EnterReaderMode => {
                    let active_id = self.wm.active_pane_id();
                    if self.reader_mode_panes.contains(&active_id) {
                        self.reader_mode_panes.remove(&active_id);
                        self.pending_wry_actions.push_back(WryAction::ExitReaderMode);
                        self.status_message = "Reader mode off".into();
                    } else {
                        self.reader_mode_panes.insert(active_id);
                        self.pending_wry_actions.push_back(WryAction::EnterReaderMode);
                        self.status_message = "Reader mode on".into();
                    }
                }
                ActionEffect::ExitReaderMode => {}
                ActionEffect::EnterMinimalMode => {
                    let active_id = self.wm.active_pane_id();
                    if self.minimal_mode_panes.contains(&active_id) {
                        self.minimal_mode_panes.remove(&active_id);
                        self.pending_wry_actions.push_back(WryAction::ExitMinimalMode);
                        self.status_message = "Minimal mode off".into();
                    } else {
                        self.minimal_mode_panes.insert(active_id);
                        self.pending_wry_actions.push_back(WryAction::EnterMinimalMode);
                        self.status_message = "Minimal mode on".into();
                    }
                }
                ActionEffect::ExitMinimalMode => {}
                ActionEffect::GetNetworkLog => {
                    self.pending_wry_actions.push_back(WryAction::GetNetworkLog);
                }
                ActionEffect::ClearNetworkLog => {}
                ActionEffect::GetConsoleLog => {
                    self.pending_wry_actions.push_back(WryAction::GetConsoleLog);
                }
                ActionEffect::ClearConsoleLog => {}
                ActionEffect::DetachPane => {
                    let active_id = self.wm.active_pane_id();
                    let url = self
                        .engines
                        .get(&active_id)
                        .and_then(|e| e.current_url().cloned());
                    if let Some(url) = url {
                        match self.wm.close(active_id) {
                            Ok(()) => {
                                self.engines.remove_pane(&active_id);
                                self.terminal_pane_ids.remove(&active_id);
                                self.pending_new_window = true;
                                self.pending_detach_url = Some(url);
                                self.status_message = "Detaching pane to popup...".into();
                            }
                            Err(_) => {
                                self.status_message = "Cannot detach the only pane".into();
                            }
                        }
                    } else {
                        self.status_message = "No URL to detach".into();
                    }
                }
                ActionEffect::CloseOtherPanes => {
                    let active_id = self.wm.active_pane_id();
                    let other_ids: Vec<uuid::Uuid> = self
                        .wm
                        .panes()
                        .iter()
                        .filter_map(|(id, _)| if *id != active_id { Some(*id) } else { None })
                        .collect();
                    for id in &other_ids {
                        self.engines.remove_pane(id);
                        self.terminal_pane_ids.remove(id);
                    }
                    if let Err(e) = self.wm.retain_only(active_id) {
                        self.status_message = format!("Failed: {}", e);
                    } else {
                        self.status_message = format!("Closed {} other pane(s)", other_ids.len());
                    }
                }
                ActionEffect::Print => {
                    self.pending_wry_actions.push_back(WryAction::Print);
                    self.status_message = "Printing...".into();
                }
                ActionEffect::PinPane => {
                    let active_id = self.wm.active_pane_id();
                    if self.pinned_pane_ids.contains(&active_id) {
                        self.pinned_pane_ids.remove(&active_id);
                        self.status_message = "Pane unpinned".into();
                    } else {
                        self.pinned_pane_ids.insert(active_id);
                        self.status_message = "Pane pinned".into();
                    }
                }
            }
        }
    }

    /// Queue a navigation to a URL, applying any Lua URL redirect rules.
    fn navigate_with_redirects(&mut self, mut url: url::Url) {
        self.session_dirty = true;
        // Apply URL redirect rules from Lua engine
        if let Some(ref engine) = self.lua_engine {
            url = engine.apply_url_redirects(&url);
        }
        // Update placeholder engine
        let active_id = self.wm.active_pane_id();
        if let Some(engine) = self.engines.get_mut(&active_id) {
            engine.navigate(&url);
        }
        if let Some(ref engine) = self.lua_engine {
            engine.call_hooks("navigate", &[url.as_str()]);
        }
        self.pending_wry_actions.push_back(WryAction::Navigate(url));
    }

    fn execute_command(&mut self, cmd: &str) {
        let cmd = cmd.trim();

        // Command chaining: split on " && " and execute each
        if cmd.contains(" && ") {
            for part in cmd.split(" && ") {
                self.execute_command(part.trim());
            }
            return;
        }

        match cmd {
            "q" | "quit" => self.should_quit = true,
            "vs" => self.execute_action(&crate::input::Action::SplitVertical),
            "sp" => self.execute_action(&crate::input::Action::SplitHorizontal),
            "files" | "browse" => {
                let path = crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path())
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                let encoded = crate::servo::wry_engine::percent_encode_path(&path.to_string_lossy());
                if let Ok(url) = url::Url::parse(&format!("aileron://files?path={}", encoded)) {
                    self.navigate_with_redirects(url);
                    self.status_message = format!("File browser: {}", path.display());
                }
            }
            "swap" | "tab-swap" => self.swap_panes(),
            "only" => {
                self.execute_action(&crate::input::Action::CloseOtherPanes);
            }
            "reader" => {
                self.execute_action(&crate::input::Action::ToggleReaderMode);
            }
            "minimal" => {
                self.execute_action(&crate::input::Action::ToggleMinimalMode);
            }
            "settings" => {
                if let Ok(url) = url::Url::parse("aileron://settings") {
                    self.navigate_with_redirects(url);
                    self.status_message = "Settings".into();
                }
            }
            "privacy" => {
                let https = self.config.https_upgrade_enabled;
                let tracking = self.config.tracking_protection_enabled;
                let adblock = self.config.adblock_enabled;
                self.status_message = format!(
                    "HTTPS upgrade: {} | Tracking protection: {} | Adblock: {}",
                    if https { "ON" } else { "OFF" },
                    if tracking { "ON" } else { "OFF" },
                    if adblock { "ON" } else { "OFF" },
                );
            }
            "engine" => {
                self.status_message =
                    "Engine: WebKit (Servo planned for Q3 2026)".into();
            }
            "https-toggle" => {
                let active_id = self.wm.active_pane_id();
                if let Some(engine) = self.engines.get(&active_id)
                    && let Some(url) = engine.current_url()
                    && let Some(host) = url.host_str()
                {
                    let host_lower = host.to_lowercase();
                    let safe_list = crate::net::privacy::load_https_safe_list();
                    if crate::net::privacy::is_https_safe(&host_lower, &safe_list) {
                        self.status_message = format!(
                            "HTTPS upgrade: {} is in the safe list",
                            host_lower
                        );
                    } else {
                        self.status_message = format!(
                            "HTTPS upgrade: {} is not in the safe list ({} domains)",
                            host_lower,
                            safe_list.len()
                        );
                    }
                } else {
                    self.status_message = "No active page URL".into();
                }
            }
            "" => {}
            _ => {
                // Shell command: !<cmd>
                if let Some(cmd) = cmd.strip_prefix("!") {
                    let cmd = cmd.trim();
                    if cmd.is_empty() {
                        self.status_message = "Usage: !<command>".into();
                        return;
                    }
                    match std::process::Command::new("sh").args(["-c", cmd]).output() {
                        Ok(output) => {
                            let stdout =
                                String::from_utf8_lossy(&output.stdout).trim().to_string();
                            let line = stdout.lines().next().unwrap_or("");
                            if line.len() > 80 {
                                self.status_message = format!("{}...", &line[..77]);
                            } else if line.is_empty() {
                                self.status_message = format!("(exit {})", output.status);
                            } else {
                                self.status_message = line.to_string();
                            }
                        }
                        Err(e) => {
                            self.status_message = format!("!{}: {}", cmd, e);
                        }
                    }
                    return;
                }

                // Runtime config: set <key> <value>
                if let Some(rest) = cmd.strip_prefix("set ") {
                    let rest = rest.trim();
                    let mut parts = rest.splitn(2, ' ');
                    if let Some(key) = parts.next() {
                        let value = parts.next().unwrap_or("");
                        match key {
                            "search_engine" if !value.is_empty() => {
                                self.config.search_engine = value.to_string();
                                self.status_message = format!("search_engine = {}", value);
                            }
                            "homepage" if !value.is_empty() => {
                                self.config.homepage = value.to_string();
                                self.status_message = format!("homepage = {}", value);
                            }
                            "adblock" => {
                                self.config.adblock_enabled = !value.contains("off")
                                    && !value.contains("false")
                                    && !value.contains("0");
                                self.status_message = format!(
                                    "adblock = {}",
                                    self.config.adblock_enabled
                                );
                            }
                            "https_upgrade" | "https-upgrade" => {
                                self.config.https_upgrade_enabled = !value.contains("off")
                                    && !value.contains("false")
                                    && !value.contains("0");
                                self.status_message = format!(
                                    "https_upgrade = {}",
                                    self.config.https_upgrade_enabled
                                );
                            }
                    "tracking_protection" | "tracking-protection" => {
                        self.config.tracking_protection_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message = format!(
                            "tracking_protection = {}",
                            self.config.tracking_protection_enabled
                        );
                    }
                    "popup_blocker" | "popup-blocker" | "popups" => {
                        self.config.popup_blocker_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message = format!(
                            "popup_blocker = {}",
                            self.config.popup_blocker_enabled
                        );
                    }
                    _ => {
                        self.status_message = format!(
                            "Unknown setting: {} (try: search_engine, homepage, adblock, https_upgrade, tracking_protection, popup_blocker)",
                            key
                        );
                    }
                }
            }
            return;
        }

        // Explicit navigate: open <url>
        if let Some(url_str) = cmd.strip_prefix("open ") {
                    let url_str = url_str.trim();
                    if url_str.is_empty() {
                        self.status_message = "Usage: open <url>".into();
                        return;
                    }
                    let url = if url_str.contains("://") {
                        url::Url::parse(url_str)
                    } else {
                        url::Url::parse(&format!("https://{}", url_str))
                    };
                    match url {
                        Ok(u) => {
                            self.navigate_with_redirects(u);
                            self.status_message = format!("Opening: {}", url_str);
                        }
                        Err(e) => {
                            self.status_message = format!("Invalid URL: {}", e);
                        }
                    }
                    return;
                }

                // Check for ssh <host> command
                if let Some(host) = cmd.strip_prefix("ssh ") {
                    let host = host.trim();
                    if host.is_empty() {
                        self.status_message = "Usage: ssh <host>".into();
                        return;
                    }
                    self.pending_terminal_command = Some(format!("ssh {}\n", host));
                    self.execute_action(&crate::input::Action::OpenTerminal);
                    return;
                }

                // Try to navigate if it looks like a URL
                if Self::looks_like_url(cmd)
                    && let Ok(url) = url::Url::parse(cmd) {
                        self.navigate_with_redirects(url);
                        self.status_message = format!("Navigating to {}", cmd);
                        return;
                    }
                // Treat as search query
                if let Some(url) = self.config.search_url(cmd) {
                    self.navigate_with_redirects(url);
                    self.status_message = format!("Searching: {}", cmd);
                } else {
                    self.status_message = format!("Unknown command: {}", cmd);
                }
            }
        }
    }

    /// Handle a raw query submitted from the command palette (no matching results).
    /// Checks if it's a URL, a known command, or shows an error.
    fn handle_raw_command(&mut self, query: &str) {
        // Command chaining: split on " && " and execute each
        if query.contains(" && ") {
            for part in query.split(" && ") {
                self.handle_raw_command(part.trim());
            }
            return;
        }

        // Check for known commands first
        match query {
            "q" | "quit" => {
                self.should_quit = true;
                return;
            }
            "vs" => {
                self.execute_action(&crate::input::Action::SplitVertical);
                return;
            }
            "sp" => {
                self.execute_action(&crate::input::Action::SplitHorizontal);
                return;
            }
            "files" | "browse" => {
                let path = crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path())
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                let encoded = crate::servo::wry_engine::percent_encode_path(&path.to_string_lossy());
                if let Ok(url) = url::Url::parse(&format!("aileron://files?path={}", encoded)) {
                    self.navigate_with_redirects(url);
                    self.status_message = format!("File browser: {}", path.display());
                }
                return;
            }
            _ => {}
        }

        // Layout save/load aliases for workspace save/load
        if let Some(name) = query.strip_prefix("layout-save ") {
            let name = name.trim();
            if name.is_empty() {
                self.status_message = "Usage: :layout-save <name>".into();
                return;
            }
            self.pending_wry_actions
                .push_back(WryAction::SaveWorkspace {
                    name: name.to_string(),
                    pane_urls: std::collections::HashMap::new(),
                });
            self.status_message = format!("Saving layout: {}...", name);
            return;
        }

        if let Some(name) = query.strip_prefix("layout-load ") {
            let name = name.trim();
            if name.is_empty() {
                self.status_message = "Usage: :layout-load <name>".into();
                return;
            }
            self.pending_workspace_restore = Some(name.to_string());
            self.status_message = format!("Loading layout: {}...", name);
            return;
        }

        // Check for bw- commands (password manager)
        if let Some(rest) = query.strip_prefix("bw-unlock ") {
            let password = rest.trim();
            if password.is_empty() {
                self.status_message = "Usage: bw-unlock <password>".into();
                return;
            }
            match self.bitwarden.unlock(password) {
                Ok(_) => {
                    self.status_message = "Vault unlocked".into();
                    info!("Bitwarden vault unlocked");
                }
                Err(e) => {
                    self.status_message = format!("Unlock failed: {}", e);
                    warn!("Bitwarden unlock failed: {}", e);
                }
            }
            return;
        }

        if let Some(rest) = query.strip_prefix("bw-search ") {
            let search_query = rest.trim();
            if search_query.is_empty() {
                self.status_message = "Usage: bw-search <query>".into();
                return;
            }
            if !self.bitwarden.is_unlocked() {
                self.status_message = "Vault is locked. Use bw-unlock <password> first.".into();
                return;
            }
            match self.bitwarden.search(search_query) {
                Ok(items) => {
                    if items.is_empty() {
                        self.status_message = format!("No vault items matching '{}'", search_query);
                    } else {
                        // Add search results to palette as Credential items
                        let credential_items: Vec<SearchItem> = items
                            .iter()
                            .map(|item| SearchItem {
                                id: format!("credential:{}", item.id),
                                label: item.name.clone(),
                                description: item.url.clone().unwrap_or_else(|| item.id.clone()),
                                category: SearchCategory::Credential,
                            })
                            .collect();
                        self.palette.add_items(credential_items);
                        self.status_message = format!(
                            "Found {} vault items for '{}'. Open palette to select.",
                            items.len(),
                            search_query
                        );
                        // Auto-open palette to show results
                        self.palette.open();
                        self.command_palette_open = true;
                        self.command_palette_input.clear();
                        // Re-search within palette to show the credential items
                        self.palette.update_query("");
                    }
                }
                Err(e) => {
                    self.status_message = format!("Vault search failed: {}", e);
                    warn!("Bitwarden search failed: {}", e);
                }
            }
            return;
        }

        if query == "bw-lock" {
            self.bitwarden.lock();
            self.status_message = "Vault locked".into();
            self.palette.set_items(
                self.palette
                    .results()
                    .iter()
                    .filter(|i| i.category != SearchCategory::Credential)
                    .cloned()
                    .collect(),
            );
            return;
        }

        if query == "bw-autofill" {
            let active_id = self.wm.active_pane_id();
            if let Some(engine) = self.engines.get(&active_id)
                && let Some(url) = engine.current_url()
            {
                let url_str = url.to_string();
                if !self.bitwarden.is_unlocked() {
                    self.status_message = "Vault locked. Use :bw-unlock <password>".into();
                } else {
                    match self.bitwarden.search_for_url(&url_str) {
                        Ok(items) if items.len() == 1 => {
                            match self.bitwarden.get_credential(&items[0].id) {
                                Ok(cred) => {
                                    let js = self.bitwarden.autofill_js(&cred);
                                    self.pending_wry_actions.push_back(WryAction::RunJs(js));
                                    self.status_message = format!("Auto-filled: {}", items[0].name);
                                }
                                Err(e) => self.status_message = format!("!{}", e),
                            }
                        }
                        Ok(items) if items.is_empty() => {
                            self.status_message = "No credentials found for this site".into();
                        }
                        Ok(items) => {
                            self.status_message = format!(
                                "Multiple matches ({}). Use :bw-search <query> to pick.",
                                items.len()
                            );
                        }
                        Err(e) => self.status_message = format!("!{}", e),
                    }
                }
            }
            return;
        }

        if query == "bw-detect" {
            self.pending_wry_actions.push_back(WryAction::RunJs(
                BitwardenClient::detect_login_forms_js().into(),
            ));
            self.status_message = "Detecting login forms...".into();
            return;
        }

        if query == "keyring-test" {
            if crate::passwords::keyring::is_available() {
                self.status_message = "System keyring: available".into();
            } else {
                self.status_message = "System keyring: not available".into();
            }
            return;
        }

        if let Some(path) = query.strip_prefix("pdf ") {
            let path = path.trim();
            if path.is_empty() {
                self.status_message = "Usage: :pdf <path-or-url>".into();
                return;
            }
            std::process::Command::new("xdg-open")
                .arg(path)
                .spawn()
                .map_err(|e| self.status_message = format!("!{}", e))
                .ok();
            self.status_message = format!("Opening PDF: {}", path);
            return;
        }

        // SSH convenience command: open a terminal and auto-type ssh <host>
        if let Some(host) = query.strip_prefix("ssh ") {
            let host = host.trim();
            if host.is_empty() {
                self.status_message = "Usage: ssh <host>".into();
                return;
            }
            self.pending_terminal_command = Some(format!("ssh {}\n", host));
            self.execute_action(&crate::input::Action::OpenTerminal);
            return;
        }

        // Workspace commands: ws-save <name>, ws-list, ws-load <name>
        if let Some(name) = query.strip_prefix("ws-save ") {
            let name = name.trim();
            if name.is_empty() {
                self.status_message = "Usage: ws-save <name>".into();
                return;
            }
            // Queue save for main.rs (which has WryPaneManager access)
            self.pending_wry_actions
                .push_back(WryAction::SaveWorkspace {
                    name: name.to_string(),
                    pane_urls: std::collections::HashMap::new(),
                });
            self.status_message = format!("Saving workspace: {}...", name);
            return;
        }

        if query == "ws-list" {
            let workspaces = self.list_workspaces();
            if workspaces.is_empty() {
                self.status_message = "No saved workspaces.".into();
            } else {
                let names: Vec<&str> = workspaces
                    .iter()
                    .filter(|w| w.name != "_autosave")
                    .map(|w| w.name.as_str())
                    .collect();
                self.status_message = format!("Workspaces: {}", names.join(", "));
            }
            return;
        }

        if let Some(name) = query.strip_prefix("ws-load ") {
            let name = name.trim();
            if name.is_empty() {
                self.status_message = "Usage: ws-load <name>".into();
                return;
            }
            // Workspace restore requires main.rs to rebuild wry panes.
            // Store the requested workspace name for main.rs to pick up.
            self.pending_workspace_restore = Some(name.to_string());
            self.status_message = format!("Restoring workspace: {}...", name);
            return;
        }

        // Swap active pane with previously active pane
        if query == "swap" || query == "tab-swap" {
            self.swap_panes();
            return;
        }

        if query == "pin" {
            self.execute_action(&crate::input::Action::PinPane);
            return;
        }

        if query == "scripts" || query == "content-scripts" {
            self.pending_wry_actions
                .push_back(WryAction::ListContentScripts);
            return;
        }

        if query == "network" || query == "netlog" {
            self.pending_wry_actions.push_back(WryAction::GetNetworkLog);
            return;
        }
        if query == "network-clear" || query == "netlog-clear" {
            self.pending_wry_actions.push_back(WryAction::ClearNetworkLog);
            return;
        }
        if query == "console" || query == "consolelog" {
            self.pending_wry_actions.push_back(WryAction::GetConsoleLog);
            return;
        }
        if query == "console-clear" {
            self.pending_wry_actions.push_back(WryAction::ClearConsoleLog);
            return;
        }

        if query == "downloads" {
            if let Some(db) = self.db.as_ref() {
                match crate::db::downloads::recent_downloads(db, 10) {
                    Ok(entries) => {
                        if entries.is_empty() {
                            self.status_message = "No downloads".into();
                        } else {
                            let items: Vec<String> = entries.iter().map(|e| {
                                format!("{} [{}%]", e.filename, e.progress_percent)
                            }).collect();
                            self.status_message = format!("Downloads: {}", items.join(", "));
                        }
                    }
                    Err(e) => self.status_message = format!("Error: {}", e),
                }
            }
            return;
        }
        if query == "downloads-clear" {
            if let Some(db) = self.db.as_ref() {
                match crate::db::downloads::clear_downloads(db) {
                    Ok(count) => self.status_message = format!("Cleared {} downloads", count),
                    Err(e) => self.status_message = format!("Error: {}", e),
                }
            }
            return;
        }
        if let Some(id_str) = query.strip_prefix("downloads-open ") {
            let id_str = id_str.trim();
            if id_str.is_empty() {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::downloads::get_latest_download_id(db) {
                        Ok(id) => {
                            match crate::db::downloads::get_download_dest_path(db, id) {
                                Ok(dest) => {
                                    let _ = std::process::Command::new("xdg-open")
                                        .arg(&dest)
                                        .stdout(std::process::Stdio::null())
                                        .stderr(std::process::Stdio::null())
                                        .spawn();
                                    self.status_message = format!("Opened: {}", dest);
                                }
                                Err(e) => self.status_message = format!("Error: {}", e),
                            }
                        }
                        Err(e) => self.status_message = format!("No downloads: {}", e),
                    }
                }
            } else if let Ok(id) = id_str.parse::<i64>() {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::downloads::get_download_dest_path(db, id) {
                        Ok(dest) => {
                            let _ = std::process::Command::new("xdg-open")
                                .arg(&dest)
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .spawn();
                            self.status_message = format!("Opened: {}", dest);
                        }
                        Err(e) => self.status_message = format!("Error: {}", e),
                    }
                } else {
                    self.status_message = "No database".into();
                }
            } else {
                self.status_message = "Usage: downloads-open [id]".into();
            }
            return;
        }
        if query == "downloads-dir" {
            if let Some(downloads_dir) = directories::UserDirs::new()
                .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
            {
                let _ = std::process::Command::new("xdg-open")
                    .arg(&downloads_dir)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                self.status_message = format!("Opened: {}", downloads_dir.display());
            } else {
                self.status_message = "Could not determine downloads directory".into();
            }
            return;
        }

        if query == "cookies-clear" {
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.cookie.split(';').forEach(function(c) { document.cookie = c.trim().split('=')[0] + '=;expires=Thu, 01 Jan 1970 00:00:00 GMT;path=/'; }); 'Cookies cleared'".into(),
            ));
            self.status_message = "Cookies cleared for current pane".into();
            return;
        }

        if query == "inspect" {
            self.pending_wry_actions.push_back(WryAction::ToggleDevTools);
            return;
        }

        if query == "privacy" {
            let https = self.config.https_upgrade_enabled;
            let tracking = self.config.tracking_protection_enabled;
            let adblock = self.config.adblock_enabled;
            self.status_message = format!(
                "HTTPS upgrade: {} | Tracking protection: {} | Adblock: {}",
                if https { "ON" } else { "OFF" },
                if tracking { "ON" } else { "OFF" },
                if adblock { "ON" } else { "OFF" },
            );
            return;
        }

        if query == "https-toggle" {
            let active_id = self.wm.active_pane_id();
            if let Some(engine) = self.engines.get(&active_id)
                && let Some(url) = engine.current_url()
                && let Some(host) = url.host_str()
            {
                let host_lower = host.to_lowercase();
                let safe_list = crate::net::privacy::load_https_safe_list();
                if crate::net::privacy::is_https_safe(&host_lower, &safe_list) {
                    self.status_message = format!(
                        "HTTPS upgrade: {} is already in the safe list",
                        host_lower
                    );
                } else {
                    self.status_message = format!(
                        "HTTPS upgrade: {} is not in the safe list ({} domains loaded)",
                        host_lower,
                        safe_list.len()
                    );
                }
            } else {
                self.status_message = "No active page URL".into();
            }
            return;
        }

        if query == "config-save" {
            self.pending_wry_actions.push_back(WryAction::SaveConfig);
            return;
        }

        if query == "import-firefox" {
            self.import_firefox();
            return;
        }

        if query == "import-chrome" {
            self.import_chrome();
            return;
        }

        if let Some(proxy_url) = query.strip_prefix("proxy ") {
            let proxy_url = proxy_url.trim();
            if proxy_url.is_empty() || proxy_url == "none" {
                self.config.proxy = None;
                unsafe { std::env::remove_var("all_proxy") };
                self.status_message = "Proxy disabled".into();
            } else {
                self.config.proxy = Some(proxy_url.to_string());
                unsafe { std::env::set_var("all_proxy", proxy_url) };
                self.status_message = format!("Proxy: {}", proxy_url);
            }
            return;
        }

        if query == "back" || query == "bd" {
            self.pending_wry_actions.push_back(WryAction::Back);
            return;
        }
        if query == "forward" || query == "fw" {
            self.pending_wry_actions.push_back(WryAction::Forward);
            return;
        }
        if query == "reload" {
            self.pending_wry_actions.push_back(WryAction::Reload);
            return;
        }

        // Rendering engine info: :engine, :engine servo, :engine webkit
        if query == "engine" {
            self.status_message = "Engine: WebKit (Servo planned for Q3 2026)".into();
            return;
        }
        if query == "engine servo" {
            self.status_message =
                "Servo engine not yet available (planned for Q3 2026)".into();
            return;
        }
        if query == "engine webkit" {
            self.status_message = "Using WebKit engine (default)".into();
            return;
        }

        // Search engine switching: :engine <name>
        if let Some(engine_name) = query.strip_prefix("engine ") {
            let engine_name = engine_name.trim();
            if engine_name.is_empty() {
                let current = &self.config.search_engine;
                let name = self.config.search_engines.iter()
                    .find(|(_, url)| *url == current)
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("default");
                self.status_message = format!("Search engine: {} ({})", name, current);
            } else if engine_name == "default" {
                self.config.search_engine = "https://duckduckgo.com/?q={query}".into();
                self.status_message = "Search engine: default (DuckDuckGo)".into();
            } else if let Some(url) = self.config.search_engines.get(engine_name) {
                self.config.search_engine = url.clone();
                self.status_message = format!("Search engine: {} ({})", engine_name, url);
            } else {
                let available: Vec<&str> = std::iter::once("default")
                    .chain(self.config.search_engines.keys().map(|s| s.as_str()))
                    .collect();
                self.status_message = format!("Unknown engine: {}. Available: {}", engine_name, available.join(", "));
            }
            return;
        }

        // Clear browsing data: :clear history|bookmarks|workspaces|all
        if let Some(subcmd) = query.strip_prefix("clear ") {
            let subcmd = subcmd.trim();
            match subcmd {
                "history" => {
                    if let Some(db) = self.db.as_ref() {
                        match crate::db::history::clear_history(db) {
                            Ok(count) => self.status_message = format!("Cleared {} history entries", count),
                            Err(e) => self.status_message = format!("Failed: {}", e),
                        }
                    }
                }
                "bookmarks" => {
                    if let Some(db) = self.db.as_ref() {
                        match crate::db::bookmarks::clear_bookmarks(db) {
                            Ok(count) => self.status_message = format!("Cleared {} bookmarks", count),
                            Err(e) => self.status_message = format!("Failed: {}", e),
                        }
                    }
                }
                "workspaces" => {
                    let workspaces = self.list_workspaces();
                    if let Some(db) = self.db.as_ref() {
                        for ws in &workspaces {
                            let _ = crate::db::workspaces::delete_workspace(db, &ws.name);
                        }
                    }
                    self.status_message = format!("Cleared {} workspaces", workspaces.len());
                }
                "cookies" => {
                    self.pending_wry_actions.push_back(WryAction::RunJs(
                        "document.cookie.split(';').forEach(function(c) { document.cookie = c.trim().split('=')[0] + '=;expires=Thu, 01 Jan 1970 00:00:00 GMT;path=/'; }); 'Cookies cleared'".into(),
                    ));
                    self.status_message = "Cookies cleared for current pane".into();
                }
                "all" => {
                    let mut parts = Vec::new();
                    if let Some(db) = self.db.as_ref() {
                        if let Ok(c) = crate::db::history::clear_history(db) {
                            parts.push(format!("{} history", c));
                        }
                        if let Ok(c) = crate::db::bookmarks::clear_bookmarks(db) {
                            parts.push(format!("{} bookmarks", c));
                        }
                        let ws = self.list_workspaces();
                        for w in &ws {
                            let _ = crate::db::workspaces::delete_workspace(db, &w.name);
                        }
                        parts.push(format!("{} workspaces", ws.len()));
                    }
                    self.status_message = format!("Cleared: {}", parts.join(", "));
                }
                _ => {
                    self.status_message = "Usage: :clear history|bookmarks|workspaces|cookies|all".into();
                }
            }
            return;
        }

        // Explicit navigate: open <url>
        if let Some(url_str) = query.strip_prefix("open ") {
            let url_str = url_str.trim();
            if url_str.is_empty() {
                self.status_message = "Usage: open <url>".into();
                return;
            }
            let url = if url_str.contains("://") {
                url::Url::parse(url_str)
            } else {
                url::Url::parse(&format!("https://{}", url_str))
            };
            match url {
                Ok(u) => {
                    self.navigate_with_redirects(u);
                    self.status_message = format!("Opening: {}", url_str);
                }
                Err(e) => {
                    self.status_message = format!("Invalid URL: {}", e);
                }
            }
            return;
        }

        // Shell command: !<cmd>
        if let Some(cmd) = query.strip_prefix("!") {
            let cmd = cmd.trim();
            if cmd.is_empty() {
                self.status_message = "Usage: !<command>".into();
                return;
            }
            match std::process::Command::new("sh").args(["-c", cmd]).output() {
                Ok(output) => {
                    let stdout =
                        String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let line = stdout.lines().next().unwrap_or("");
                    if line.len() > 80 {
                        self.status_message = format!("{}...", &line[..77]);
                    } else if line.is_empty() {
                        self.status_message = format!("(exit {})", output.status);
                    } else {
                        self.status_message = line.to_string();
                    }
                }
                Err(e) => {
                    self.status_message = format!("!{}: {}", cmd, e);
                }
            }
            return;
        }

        // Runtime config: set <key> <value>
        if let Some(rest) = query.strip_prefix("set ") {
            let rest = rest.trim();
            let mut parts = rest.splitn(2, ' ');
            if let Some(key) = parts.next() {
                let value = parts.next().unwrap_or("");
                match key {
                    "search_engine" if !value.is_empty() => {
                        self.config.search_engine = value.to_string();
                        self.status_message = format!("search_engine = {}", value);
                    }
                    "homepage" if !value.is_empty() => {
                        self.config.homepage = value.to_string();
                        self.status_message = format!("homepage = {}", value);
                    }
                    "adblock" => {
                        self.config.adblock_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message =
                            format!("adblock = {}", self.config.adblock_enabled);
                    }
                    "https_upgrade" | "https-upgrade" => {
                        self.config.https_upgrade_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message = format!(
                            "https_upgrade = {}",
                            self.config.https_upgrade_enabled
                        );
                    }
                    "tracking_protection" | "tracking-protection" => {
                        self.config.tracking_protection_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message = format!(
                            "tracking_protection = {}",
                            self.config.tracking_protection_enabled
                        );
                    }
                    "popup_blocker" | "popup-blocker" | "popups" => {
                        self.config.popup_blocker_enabled = !value.contains("off")
                            && !value.contains("false")
                            && !value.contains("0");
                        self.status_message = format!(
                            "popup_blocker = {}",
                            self.config.popup_blocker_enabled
                        );
                    }
                    _ => {
                        self.status_message = format!(
                            "Unknown setting: {} (try: search_engine, homepage, adblock, https_upgrade, tracking_protection, popup_blocker)",
                            key
                        );
                    }
                }
            }
            return;
        }

        // Project-wide search: :grep <pattern> [path]
        if let Some(pattern) = query.strip_prefix("grep ") {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                self.status_message = "Usage: :grep <pattern> [path]".into();
                return;
            }
            let (pattern, search_path) = if pattern.contains(' ') {
                let mut parts = pattern.splitn(2, ' ');
                let p = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or(".");
                (p, path)
            } else {
                (pattern, ".")
            };

            let output = if std::path::PathBuf::from("/usr/bin/rg").exists() {
                std::process::Command::new("rg")
                    .args(["--no-heading", "-n", "-i", pattern, search_path])
                    .output()
            } else {
                std::process::Command::new("grep")
                    .args(["-rn", "-i", pattern, search_path])
                    .output()
            };

            match output {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let lines: Vec<&str> = stdout.lines().take(15).collect();
                    let total = stdout.lines().count();
                    if lines.is_empty() {
                        self.status_message = "No matches found".into();
                    } else {
                        let results: Vec<String> = lines.iter().map(|l| {
                            if l.len() > 80 { format!("{}...", &l[..77]) } else { l.to_string() }
                        }).collect();
                        let suffix = if total > 15 { format!(" (+{} more)", total - 15) } else { String::new() };
                        self.status_message = format!("{}{}", results.join(" │ "), suffix);
                    }
                }
                Ok(output) => {
                    self.status_message = format!("grep: {}", String::from_utf8_lossy(&output.stderr));
                }
                Err(e) => {
                    self.status_message = format!("grep failed: {}", e);
                }
            }
            return;
        }

        if query == "git-status" || query == "gs" {
            if let Some(root) = crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path()) {
                match std::process::Command::new("git")
                    .args(["-C", &root.to_string_lossy(), "status", "--short"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let lines: Vec<&str> = stdout.lines().take(10).collect();
                        if lines.is_empty() {
                            self.status_message = "Working tree clean".into();
                        } else {
                            let total = stdout.lines().count();
                            let suffix = if total > 10 { format!(" (+{} more)", total - 10) } else { String::new() };
                            self.status_message = format!("{}{}", lines.join(" │ "), suffix);
                        }
                    }
                    Ok(output) => {
                        self.status_message = format!("git: {}", String::from_utf8_lossy(&output.stderr).trim());
                    }
                    Err(e) => self.status_message = format!("git failed: {}", e),
                }
            } else {
                self.status_message = "Not in a git repository".into();
            }
            return;
        }

        if query == "git-log" || query == "gl" {
            if let Some(root) = crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path()) {
                match std::process::Command::new("git")
                    .args(["-C", &root.to_string_lossy(), "log", "--oneline", "-10"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        self.status_message = if stdout.is_empty() { "No commits".into() } else { format!("Log: {}", stdout.replace('\n', " │ ")) };
                    }
                    Ok(output) => {
                        self.status_message = format!("git: {}", String::from_utf8_lossy(&output.stderr).trim());
                    }
                    Err(e) => self.status_message = format!("git failed: {}", e),
                }
            } else {
                self.status_message = "Not in a git repository".into();
            }
            return;
        }

        if query == "git-diff" || query == "gd" {
            if let Some(root) = crate::git::repo_root(std::env::current_dir().unwrap_or_default().as_path()) {
                match std::process::Command::new("git")
                    .args(["-C", &root.to_string_lossy(), "diff", "--stat"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        self.status_message = if stdout.is_empty() { "No changes".into() } else { format!("Diff: {}", stdout.replace('\n', " │ ")) };
                    }
                    Ok(output) => {
                        self.status_message = format!("git: {}", String::from_utf8_lossy(&output.stderr).trim());
                    }
                    Err(e) => self.status_message = format!("git failed: {}", e),
                }
            } else {
                self.status_message = "Not in a git repository".into();
            }
            return;
        }

        if query == "terminal-clear" || query == "cls" {
            let active_id = self.wm.active_pane_id();
            if self.terminal_pane_ids.contains(&active_id) {
                self.pending_wry_actions.push_back(WryAction::RunJs(
                    r#"if (window._terminal && window._terminal.clear) { window._terminal.clear(); }"#.into(),
                ));
                self.status_message = "Terminal cleared".into();
            } else {
                self.status_message = "Not a terminal pane".into();
            }
            return;
        }

        if let Some(pattern) = query.strip_prefix("terminal-search ") {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                self.status_message = "Usage: :terminal-search <pattern>".into();
                return;
            }
            let active_id = self.wm.active_pane_id();
            if self.terminal_pane_ids.contains(&active_id) {
                let escaped = pattern.replace('\\', "\\\\").replace('\'', "\\'");
                self.pending_wry_actions.push_back(WryAction::RunJs(
                    format!(r#"
if (window._terminal && window._terminal.buffer) {{
    var buffer = window._terminal.buffer;
    var lines = buffer.active.bufferBase.getLines();
    var matches = [];
    for (var i = 0; i < lines.length; i++) {{
        if (lines[i].includes('{}')) {{
            matches.push((i, lines[i].trim()));
        }}
    }}
    if (matches.length > 0) {{
        var firstMatch = matches[0];
        window._terminal.scrollToLine(firstMatch[0]);
    }}
    matches.length + ' match(es) in scrollback';
}}
"#, escaped),
                ));
            } else {
                self.status_message = "Not a terminal pane".into();
            }
            return;
        }

        // Print command
        if query == "print" {
            self.pending_wry_actions.push_back(WryAction::Print);
            self.status_message = "Printing...".into();
            return;
        }

        // Popup blocker toggle
        if query == "popups" {
            self.config.popup_blocker_enabled = !self.config.popup_blocker_enabled;
            self.status_message = format!(
                "Popup blocker: {}",
                if self.config.popup_blocker_enabled { "on" } else { "off" }
            );
            return;
        }
        if let Some(val) = query.strip_prefix("popups ") {
            let val = val.trim();
            self.config.popup_blocker_enabled =
                !val.contains("off") && !val.contains("false") && !val.contains("0");
            self.status_message = format!(
                "Popup blocker: {}",
                if self.config.popup_blocker_enabled { "on" } else { "off" }
            );
            return;
        }

        // Cookie management
        if query == "cookies" {
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.cookie || '(no cookies for this site)'".into(),
            ));
            self.status_message = "Showing cookies...".into();
            return;
        }
        if let Some(domain) = query.strip_prefix("cookies-block ") {
            let domain = domain.trim();
            if domain.is_empty() {
                self.status_message = "Usage: :cookies-block <domain>".into();
                return;
            }
            if let Some(db) = self.db.as_ref() {
                match crate::db::site_settings::set_site_field(
                    db,
                    domain,
                    "exact",
                    "cookies",
                    Some("off"),
                ) {
                    Ok(()) => self.status_message = format!("Cookies blocked for {}", domain),
                    Err(e) => self.status_message = format!("Failed: {}", e),
                }
            }
            return;
        }
        if let Some(domain) = query.strip_prefix("cookies-allow ") {
            let domain = domain.trim();
            if domain.is_empty() {
                self.status_message = "Usage: :cookies-allow <domain>".into();
                return;
            }
            if let Some(db) = self.db.as_ref() {
                match crate::db::site_settings::set_site_field(
                    db,
                    domain,
                    "exact",
                    "cookies",
                    Some("on"),
                ) {
                    Ok(()) => self.status_message = format!("Cookies allowed for {}", domain),
                    Err(e) => self.status_message = format!("Failed: {}", e),
                }
            }
            return;
        }

        // Mute / unmute
        if query == "mute" {
            let active_id = self.wm.active_pane_id();
            self.muted_pane_ids.insert(active_id);
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.querySelectorAll('video, audio').forEach(function(el) { el.muted = true; el.pause(); });"
                    .into(),
            ));
            self.status_message = "Muted".into();
            return;
        }
        if query == "unmute" {
            let active_id = self.wm.active_pane_id();
            self.muted_pane_ids.remove(&active_id);
            self.pending_wry_actions.push_back(WryAction::RunJs(
                "document.querySelectorAll('video, audio').forEach(function(el) { el.muted = false; });"
                    .into(),
            ));
            self.status_message = "Unmuted".into();
            return;
        }

        // Theme commands
        if query == "theme" {
            self.status_message = format!("Theme: {}", self.config.theme);
            return;
        }
        if query == "theme list" {
            let themes = self.config.available_themes();
            self.status_message = format!("Themes: {}", themes.join(", "));
            return;
        }
        if let Some(name) = query.strip_prefix("theme ") {
            let name = name.trim();
            if name.is_empty() {
                self.status_message = format!("Theme: {}", self.config.theme);
                return;
            }
            if self.config.themes.contains_key(name) {
                self.config.theme = name.to_string();
                self.status_message = format!("Theme: {}", name);
            } else {
                let available = self.config.available_themes();
                self.status_message = format!(
                    "Unknown theme '{}'. Available: {}",
                    name,
                    available.join(", ")
                );
            }
            return;
        }

        // Site settings commands
        if query == "site-settings" {
            let active_id = self.wm.active_pane_id();
            if let Some(engine) = self.engines.get(&active_id)
                && let Some(url) = engine.current_url()
            {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::site_settings::get_site_settings_for_url(db, url.as_str()) {
                        Ok(settings) => {
                            if settings.is_empty() {
                                self.status_message = "No per-site settings for current URL".into();
                            } else {
                                let items: Vec<String> = settings
                                    .iter()
                                    .map(|s| {
                                        let mut parts = vec![format!("{}[{}]", s.pattern, s.pattern_type)];
                                        if let Some(z) = s.zoom_level {
                                            parts.push(format!("zoom={}", z));
                                        }
                                        if let Some(b) = s.adblock_enabled {
                                            parts.push(format!("adblock={}", if b { "on" } else { "off" }));
                                        }
                                        if let Some(b) = s.javascript_enabled {
                                            parts.push(format!("js={}", if b { "on" } else { "off" }));
                                        }
                                        if let Some(b) = s.cookies_enabled {
                                            parts.push(format!("cookies={}", if b { "on" } else { "off" }));
                                        }
                                        if let Some(b) = s.autoplay_enabled {
                                            parts.push(format!("autoplay={}", if b { "on" } else { "off" }));
                                        }
                                        parts.join(" ")
                                    })
                                    .collect();
                                self.status_message = format!("Site settings: {}", items.join(" | "));
                            }
                        }
                        Err(e) => self.status_message = format!("Error: {}", e),
                    }
                }
            } else {
                self.status_message = "No active URL".into();
            }
            return;
        }
        if let Some(rest) = query.strip_prefix("site-settings set ") {
            let rest = rest.trim();
            let mut parts = rest.splitn(2, ' ');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                let value = value.trim();
                let active_id = self.wm.active_pane_id();
                let host = self
                    .engines
                    .get(&active_id)
                    .and_then(|e| e.current_url())
                    .and_then(|u| u.host_str())
                    .map(|h| h.to_lowercase())
                    .unwrap_or_default();

                if host.is_empty() {
                    self.status_message = "No active URL for site settings".into();
                } else if let Some(db) = self.db.as_ref() {
                    match crate::db::site_settings::set_site_field(db, &host, "exact", key, Some(value)) {
                        Ok(()) => self.status_message = format!("Set {}={} for {}", key, value, host),
                        Err(e) => self.status_message = format!("Failed: {}", e),
                    }
                }
            } else {
                self.status_message = "Usage: :site-settings set <key> <value> (zoom, adblock, js, cookies, autoplay)".into();
            }
            return;
        }
        if query == "site-settings list" {
            if let Some(db) = self.db.as_ref() {
                match crate::db::site_settings::list_site_settings(db) {
                    Ok(settings) => {
                        if settings.is_empty() {
                            self.status_message = "No site settings".into();
                        } else {
                            let items: Vec<String> = settings
                                .iter()
                                .take(10)
                                .map(|s| format!("[{}] {} (id:{})", s.pattern_type, s.pattern, s.id))
                                .collect();
                            let suffix = if settings.len() > 10 {
                                format!(" (+{} more)", settings.len() - 10)
                            } else {
                                String::new()
                            };
                            self.status_message = format!("{}{}", items.join(" | "), suffix);
                        }
                    }
                    Err(e) => self.status_message = format!("Error: {}", e),
                }
            }
            return;
        }
        if let Some(id_str) = query.strip_prefix("site-settings delete ") {
            let id_str = id_str.trim();
            if let Ok(id) = id_str.parse::<i64>() {
                if let Some(db) = self.db.as_ref() {
                    match crate::db::site_settings::delete_site_setting(db, id) {
                        Ok(true) => self.status_message = format!("Deleted site setting {}", id),
                        Ok(false) => self.status_message = format!("No site setting with id {}", id),
                        Err(e) => self.status_message = format!("Failed: {}", e),
                    }
                }
            } else {
                self.status_message = "Usage: :site-settings delete <id>".into();
            }
            return;
        }
        if let Some(domain) = query.strip_prefix("site-settings clear ") {
            let domain = domain.trim();
            if domain.is_empty() {
                self.status_message = "Usage: :site-settings clear <domain>".into();
                return;
            }
            if let Some(db) = self.db.as_ref() {
                match crate::db::site_settings::delete_site_settings_for_domain(db, domain) {
                    Ok(count) => self.status_message = format!("Cleared {} setting(s) for {}", count, domain),
                    Err(e) => self.status_message = format!("Failed: {}", e),
                }
            }
            return;
        }

        // Quickmark set: m<letter> <url>
        if query.starts_with('m') && query.len() >= 2 && query.as_bytes()[1].is_ascii_alphabetic() {
            let letter = query.as_bytes()[1] as char;
            let rest = query[2..].trim();
            if rest.is_empty() {
                self.status_message = format!("Quickmark {}: {}", letter,
                    self.quickmarks.get(&letter).map(|s| s.as_str()).unwrap_or("(not set)"));
                return;
            }
            self.quickmarks.insert(letter, rest.to_string());
            self.status_message = format!("Quickmark {} set", letter);
            return;
        }

        // Quickmark go: g<letter>
        if query.starts_with('g') && query.len() == 2 && query.as_bytes()[1].is_ascii_alphabetic() {
            let letter = query.as_bytes()[1] as char;
            match self.quickmarks.get(&letter) {
                Some(url_str) => {
                    if let Ok(url) = url::Url::parse(url_str) {
                        self.navigate_with_redirects(url);
                        self.status_message = format!("Quickmark {}", letter);
                    }
                }
                None => {
                    self.status_message = format!("Quickmark {} not set", letter);
                }
            }
            return;
        }

        // Check if it looks like a URL
        if Self::looks_like_url(query) {
            // Try parsing as-is first, then prepend https://
            let url = if let Ok(u) = url::Url::parse(query) {
                u
            } else if let Ok(u) = url::Url::parse(&format!("https://{}", query)) {
                u
            } else {
                self.status_message = format!("Invalid URL: {}", query);
                return;
            };

            // Update placeholder engine + apply URL redirects
            self.navigate_with_redirects(url);
            self.status_message = format!("Navigating to {}", query);
        } else {
            // Try fuzzy suggestion before falling back to search
            let known_commands = [
                "q", "quit", "open", "ssh", "set", "vs", "sp", "files", "browse",
                "bw-unlock", "bw-search", "bw-lock", "bw-autofill", "bw-detect",
                "keyring-test",
                "adblock-toggle", "adblock-count", "privacy", "https-toggle",
                "downloads", "downloads-open", "downloads-dir", "downloads-clear",
                "import-firefox", "import-chrome",
                "site-settings", "cookies", "cookies-clear", "cookies-block", "cookies-allow",
                "popups", "mute", "unmute", "theme", "theme-list",
                "print", "pdf", "pin",
                "scripts", "network", "network-clear", "console", "console-clear",
                "inspect", "proxy", "config-save", "clear",
                "layout-save", "layout-load", "ws-save", "ws-load", "ws-list",
                "reader", "minimal", "only", "detach",
            ];
            let cmd = query;
            let suggestion = known_commands
                .iter()
                .filter(|c| c.contains(cmd) || cmd.contains(*c))
                .min_by_key(|c| Self::levenshtein_distance(cmd, c));
            if let Some(sug) = suggestion {
                self.status_message = format!("Unknown command: {} (did you mean :{}?)", cmd, sug);
            } else if let Some(url) = self.config.search_url(query) {
                self.navigate_with_redirects(url);
                self.status_message = format!("Searching: {}", query);
            } else {
                self.status_message = format!("Search failed for: {}", query);
            }
        }
    }

    fn levenshtein_distance(a: &str, b: &str) -> usize {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        let m = a.len();
        let n = b.len();
        if m == 0 { return n; }
        if n == 0 { return m; }
        let mut dp = vec![vec![0; n + 1]; m + 1];
        for (i, row) in dp.iter_mut().enumerate().take(m + 1) { row[0] = i; }
        for (j, val) in dp[0].iter_mut().enumerate().take(n + 1).skip(1) { *val = j; }
        for i in 1..=m {
            for j in 1..=n {
                let cost = if a[i-1] == b[j-1] { 0 } else { 1 };
                dp[i][j] = (dp[i-1][j] + 1).min((dp[i][j-1] + 1).min(dp[i-1][j-1] + cost));
            }
        }
        dp[m][n]
    }

    /// Check if a string looks like a URL.
    /// Matches: http://, https://, aileron://, or bare domains like "example.com"
    fn looks_like_url(s: &str) -> bool {
        // Explicit scheme
        if s.contains("://") {
            return true;
        }
        // Bare domain: contains a dot and no spaces, and doesn't look like a command
        if s.contains('.') && !s.contains(' ') && !s.contains('/') {
            // Exclude things that look like file paths or commands
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty()) {
                // Check TLD is reasonable (at least 2 chars, all alpha)
                if let Some(tld) = parts.last() {
                    return tld.len() >= 2 && tld.chars().all(|c| c.is_alphabetic());
                }
            }
        }
        false
    }

    pub fn execute_palette_selection(&mut self, item: &crate::ui::SearchItem) {
        match item.category {
            SearchCategory::History => {
                // Navigate to the selected history URL
                if let Ok(url) = url::Url::parse(&item.description) {
                    let active_id = self.wm.active_pane_id();
                    if let Some(engine) = self.engines.get_mut(&active_id) {
                        engine.navigate(&url);
                        self.status_message = format!("Navigating to {}", item.label);
                    }
                }
            }
            SearchCategory::Command => {
                self.execute_command(&item.id);
            }
            SearchCategory::Bookmark => {
                if let Ok(url) = url::Url::parse(&item.description) {
                    let active_id = self.wm.active_pane_id();
                    if let Some(engine) = self.engines.get_mut(&active_id) {
                        engine.navigate(&url);
                        self.status_message = format!("Opening bookmark: {}", item.label);
                    }
                }
            }
            SearchCategory::Credential => {
                // Extract item ID from "credential:<id>"
                if let Some(item_id) = item.id.strip_prefix("credential:") {
                    if !self.bitwarden.is_unlocked() {
                        self.status_message =
                            "Vault is locked. Use bw-unlock <password> first.".into();
                        return;
                    }
                    match self.bitwarden.get_credential(item_id) {
                        Ok(credential) => {
                            let js = self.bitwarden.autofill_js(&credential);
                            info!("Auto-filling credential for: {}", credential.name);
                            self.pending_wry_actions
                                .push_back(WryAction::Autofill { js });
                            self.status_message = format!("Auto-filled: {}", credential.name);
                        }
                        Err(e) => {
                            self.status_message = format!("Failed to get credential: {}", e);
                            warn!("Bitwarden get_credential failed: {}", e);
                        }
                    }
                }
            }
            SearchCategory::Custom => {
                // Extract command name from "custom:<name>"
                if let Some(name) = item.id.strip_prefix("custom:") {
                    match self.call_lua_command(name) {
                        Ok(result) => {
                            info!("Lua command '{}' executed: {}", name, result);
                            self.status_message = format!("✓ {}", name);
                        }
                        Err(e) => {
                            self.status_message = format!("Command '{}' failed: {}", name, e);
                            warn!("Lua command '{}' failed: {}", name, e);
                        }
                    }
                }
            }
            SearchCategory::OpenTab => {
                if let Some(pane_id_str) = item.id.strip_prefix("tab:")
                    && let Ok(pane_id) = uuid::Uuid::parse_str(pane_id_str)
                        && self.wm.get_rect(pane_id).is_some() {
                            let old_active = self.wm.active_pane_id();
                            if old_active != pane_id {
                                self.last_active_pane_id = Some(old_active);
                            }
                            self.wm.set_active_pane(pane_id);
                            let url = self
                                .engines
                                .get(&pane_id)
                                .and_then(|e| e.current_url().cloned())
                                .map(|u| u.to_string())
                                .unwrap_or_default();
                            let display = if url.len() > 50 {
                                format!("{}...", &url[..47])
                            } else {
                                url
                            };
                            self.status_message = format!("Switched to: {}", display);
                        }
            }
            _ => {
                self.status_message = format!("Selected: {}", item.label);
            }
        }
    }

    pub fn update_status(&mut self) {
        self.status_message = format!("-- {} --", self.mode);
    }

    pub fn update_omnibox(&mut self, query: &str) {
        self.omnibox_results.clear();
        self.omnibox_selected = 0;

        let query = query.trim();
        if query.is_empty() {
            self.last_omnibox_query.clear();
            return;
        }

        self.last_omnibox_query = query.to_string();

        let looks_like_url = query.contains("://") || query.starts_with("aileron://")
            || (query.contains('.') && !query.contains(' '));

        if looks_like_url {
            let url = if query.contains("://") || query.starts_with("aileron://") {
                query.to_string()
            } else {
                format!("https://{}", query)
            };
            self.omnibox_results.push(SearchItem {
                id: format!("nav:{}", url),
                label: url.clone(),
                description: "Navigate to URL".to_string(),
                category: SearchCategory::Command,
            });
        } else {
            let search_url = self.config.search_url(query)
                .map(|u| u.to_string())
                .unwrap_or_default();
            self.omnibox_results.push(SearchItem {
                id: format!("search:{}", query),
                label: format!("Search: {}", query),
                description: search_url,
                category: SearchCategory::Command,
            });
        }

        if let Some(db) = self.db.as_ref() {
            if let Ok(bookmarks) = bookmarks::search_bookmarks(db, query, 5) {
                for bm in bookmarks {
                    self.omnibox_results.push(SearchItem {
                        id: format!("bookmark:{}", bm.url),
                        label: bm.title,
                        description: bm.url,
                        category: SearchCategory::Bookmark,
                    });
                }
            }

            if let Ok(history) = crate::db::history::search(db, query, 5) {
                for h in history {
                    self.omnibox_results.push(SearchItem {
                        id: format!("history:{}", h.url),
                        label: h.url.clone(),
                        description: format!("visited {} times", h.visit_count),
                        category: SearchCategory::History,
                    });
                }
            }
        }

        if self.omnibox_results.len() > 10 {
            self.omnibox_results.truncate(10);
        }
    }

    pub fn handle_omnibox_select(&mut self, index: usize) {
        if let Some(item) = self.omnibox_results.get(index) {
            let id = item.id.clone();
            let label = item.label.clone();
            if let Some(url_str) = id.strip_prefix("nav:") {
                if let Ok(url) = url::Url::parse(url_str) {
                    self.navigate_with_redirects(url);
                    self.status_message = format!("Navigating to {}", url_str);
                }
            } else if let Some(query) = id.strip_prefix("search:") {
                if let Some(url) = self.config.search_url(query) {
                    self.navigate_with_redirects(url);
                    self.status_message = format!("Searching: {}", query);
                }
            } else if let Some(url) = id.strip_prefix("bookmark:") {
                if let Ok(parsed) = url::Url::parse(url) {
                    self.navigate_with_redirects(parsed);
                    self.status_message = format!("Opening bookmark: {}", label);
                }
            } else if let Some(url) = id.strip_prefix("history:")
                && let Ok(parsed) = url::Url::parse(url)
            {
                self.navigate_with_redirects(parsed);
                self.status_message = format!("Opening: {}", url);
            }
        }
    }

    /// Call a registered Lua custom command by name.
    pub fn call_lua_command(&self, name: &str) -> anyhow::Result<String> {
        if let Some(ref engine) = self.lua_engine {
            engine.call_command(name, &[])
        } else {
            anyhow::bail!("Lua engine not initialized")
        }
    }

    /// Save the current pane layout as a named workspace.
    /// Uses URLs from the BSP tree's Pane structs (no live URL capture).
    pub fn save_workspace(&self, name: &str) -> anyhow::Result<()> {
        self.save_workspace_with_urls(name, &std::collections::HashMap::new())
    }

    /// Save the current pane layout as a named workspace with live URLs.
    /// The `pane_urls` map overrides BSP tree URLs with current wry pane URLs.
    pub fn save_workspace_with_urls(
        &self,
        name: &str,
        pane_urls: &std::collections::HashMap<uuid::Uuid, String>,
    ) -> anyhow::Result<()> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database connection"))?;

        let url_resolver =
            |pane_id: uuid::Uuid| -> Option<String> { pane_urls.get(&pane_id).cloned() };

        let data = self.wm.to_workspace_data(url_resolver)?;
        crate::db::workspaces::save_workspace(db, name, &data)?;
        Ok(())
    }

    /// List all saved workspaces.
    pub fn list_workspaces(&self) -> Vec<crate::db::workspaces::Workspace> {
        self.db
            .as_ref()
            .and_then(|conn| crate::db::workspaces::list_workspaces(conn).ok())
            .unwrap_or_default()
    }

    pub fn record_visit(&self, url: &url::Url, title: &str) {
        if let Some(ref conn) = self.db
            && let Err(e) = crate::db::history::record_visit(conn, url, title) {
                warn!("Failed to record visit: {}", e);
            }
    }

    pub fn recent_history(&self, limit: usize) -> Vec<crate::db::history::HistoryEntry> {
        self.db
            .as_ref()
            .and_then(|conn| crate::db::history::recent_entries(conn, limit).ok())
            .unwrap_or_default()
    }

    pub fn search_history(
        &self,
        query: &str,
        limit: usize,
    ) -> Vec<crate::db::history::HistoryEntry> {
        self.db
            .as_ref()
            .and_then(|conn| crate::db::history::search(conn, query, limit).ok())
            .unwrap_or_default()
    }

    fn import_firefox(&mut self) {
        let db = match self.db.as_ref() {
            Some(db) => db,
            None => {
                self.status_message = "No database connection".into();
                return;
            }
        };

        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => {
                self.status_message = "Cannot determine HOME directory".into();
                return;
            }
        };

        let firefox_dir = std::path::Path::new(&home).join(".mozilla/firefox");
        if !firefox_dir.exists() {
            self.status_message = "Firefox data not found (~/.mozilla/firefox)".into();
            return;
        }

        let mut bookmarks_imported = 0usize;
        let mut history_imported = 0usize;

        let profiles: Vec<std::path::PathBuf> = std::fs::read_dir(&firefox_dir)
            .ok()
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .filter(|e| {
                        e.file_name()
                            .to_str()
                            .map(|n| n.ends_with(".default") || n.contains(".default-"))
                            .unwrap_or(false)
                    })
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default();

        for profile_dir in &profiles {
            let bk_dir = profile_dir.join("bookmarkbackups");
            if bk_dir.exists()
                && let Ok(entries) = std::fs::read_dir(&bk_dir)
            {
                let mut backups: Vec<std::path::PathBuf> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e == "json" || e == "html")
                            .unwrap_or(false)
                    })
                    .collect();
                backups.sort();
                backups.reverse();
                if let Some(latest) = backups.first() {
                    let ext = latest.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext == "json" {
                        bookmarks_imported += Self::import_firefox_bookmarks_json(db, latest);
                    } else {
                        bookmarks_imported += Self::import_firefox_bookmarks_html(db, latest);
                    }
                }
            }

            let places_path = profile_dir.join("places.sqlite");
            if places_path.exists() {
                history_imported += Self::import_firefox_history(db, &places_path);
            }
        }

        self.status_message = format!(
            "Firefox import: {} bookmarks, {} history entries",
            bookmarks_imported, history_imported
        );
    }

    fn import_firefox_bookmarks_json(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(j) => j,
            Err(_) => return 0,
        };
        let mut count = 0usize;
        Self::walk_firefox_json_bookmarks(&json, db, &mut count);
        count
    }

    fn walk_firefox_json_bookmarks(node: &serde_json::Value, db: &rusqlite::Connection, count: &mut usize) {
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for child in children {
                if child.get("type").and_then(|t| t.as_str()) == Some("text/x-moz-place")
                    && let (Some(url), Some(title)) = (
                        child.get("uri").and_then(|u| u.as_str()),
                        child.get("title").and_then(|t| t.as_str()),
                    )
                    && url.starts_with("http")
                    && bookmarks::import_bookmark(db, url, title).unwrap_or(false)
                {
                    *count += 1;
                }
                Self::walk_firefox_json_bookmarks(child, db, count);
            }
        }
    }

    fn import_firefox_bookmarks_html(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let mut count = 0usize;
        for line in content.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("<DT><A ")
                && let Some(href_start) = rest.find("HREF=\"")
                && let Some(href_end) = rest[href_start + 6..].find('"')
            {
                let after_href = &rest[href_start + 6..];
                let url = &after_href[..href_end];
                let title = after_href[href_end + 1..]
                    .find('>')
                    .and_then(|gt| {
                        let after_gt = &after_href[gt + 1..];
                        after_gt.find("</A>").map(|end| &after_gt[..end])
                    })
                    .unwrap_or(url);
                if url.starts_with("http")
                    && bookmarks::import_bookmark(db, url, title).unwrap_or(false)
                {
                    count += 1;
                }
            }
        }
        count
    }

    fn import_firefox_history(db: &rusqlite::Connection, places_path: &std::path::Path) -> usize {
        let conn = match rusqlite::Connection::open_with_flags(
            places_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let mut stmt = match conn.prepare(
            "SELECT p.url, p.title, h.visit_date
             FROM moz_places p
             JOIN moz_historyvisits h ON p.id = h.place_id
             WHERE p.url LIKE 'http%' AND h.visit_type IN (1, 2)
             ORDER BY h.visit_date DESC
             LIMIT 500",
        ) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let rows = match stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, i64>(2).unwrap_or(0),
            ))
        }) {
            Ok(r) => r,
            Err(_) => return 0,
        };
        let mut count = 0usize;
        for row in rows.filter_map(|r| r.ok()) {
            let (url, title, visit_date) = row;
            let visited_at = if visit_date > 0 {
                let epoch_us = visit_date / 1000;
                let secs = epoch_us / 1_000_000;
                chrono::DateTime::from_timestamp(secs, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            if crate::db::history::import_visit(db, &url, &title, &visited_at).unwrap_or(false) {
                count += 1;
            }
        }
        count
    }

    fn import_chrome(&mut self) {
        let db = match self.db.as_ref() {
            Some(db) => db,
            None => {
                self.status_message = "No database connection".into();
                return;
            }
        };

        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => {
                self.status_message = "Cannot determine HOME directory".into();
                return;
            }
        };

        let chrome_dir = std::path::Path::new(&home)
            .join(".config/google-chrome/Default");
        if !chrome_dir.exists() {
            self.status_message = "Chrome data not found (~/.config/google-chrome/Default)".into();
            return;
        }

        let bookmarks_path = chrome_dir.join("Bookmarks");
        let history_path = chrome_dir.join("History");

        let mut bookmarks_imported = 0usize;
        let mut history_imported = 0usize;

        if bookmarks_path.exists() {
            bookmarks_imported = Self::import_chrome_bookmarks(db, &bookmarks_path);
        }

        if history_path.exists() {
            history_imported = Self::import_chrome_history(db, &history_path);
        }

        self.status_message = format!(
            "Chrome import: {} bookmarks, {} history entries",
            bookmarks_imported, history_imported
        );
    }

    fn import_chrome_bookmarks(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(j) => j,
            Err(_) => return 0,
        };
        let mut count = 0usize;
        if let Some(roots) = json.get("roots").and_then(|r| r.as_object()) {
            for (_key, node) in roots {
                Self::walk_chrome_bookmark_node(node, db, &mut count);
            }
        }
        count
    }

    fn walk_chrome_bookmark_node(node: &serde_json::Value, db: &rusqlite::Connection, count: &mut usize) {
        if node.get("type").and_then(|t| t.as_str()) == Some("url")
            && let (Some(url), Some(name)) = (
                node.get("url").and_then(|u| u.as_str()),
                node.get("name").and_then(|n| n.as_str()),
            )
            && url.starts_with("http")
            && bookmarks::import_bookmark(db, url, name).unwrap_or(false)
        {
            *count += 1;
            return;
        }
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for child in children {
                Self::walk_chrome_bookmark_node(child, db, count);
            }
        }
    }

    fn import_chrome_history(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
        let conn = match rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let mut stmt = match conn.prepare(
            "SELECT u.url, u.title, v.visit_time
             FROM urls u
             JOIN visits v ON u.id = v.url
             ORDER BY v.visit_time DESC
             LIMIT 500",
        ) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let rows = match stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, i64>(2).unwrap_or(0),
            ))
        }) {
            Ok(r) => r,
            Err(_) => return 0,
        };
        let mut count = 0usize;
        for row in rows.filter_map(|r| r.ok()) {
            let (url, title, visit_time) = row;
            let visited_at = if visit_time > 0 {
                let epoch_us = visit_time / 1000;
                let secs = epoch_us / 1_000_000;
                chrono::DateTime::from_timestamp(secs, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            if crate::db::history::import_visit(db, &url, &title, &visited_at).unwrap_or(false) {
                count += 1;
            }
        }
        count
    }

    fn swap_panes(&mut self) {
        if let Some(last_id) = self.last_active_pane_id {
            let active_id = self.wm.active_pane_id();
            if last_id != active_id
                && self
                    .wm
                    .panes()
                    .iter()
                    .any(|(id, _)| *id == last_id)
            {
                let active_url = self
                    .engines
                    .get(&active_id)
                    .and_then(|e| e.current_url().cloned());
                let last_url = self
                    .engines
                    .get(&last_id)
                    .and_then(|e| e.current_url().cloned());
                if let (Some(a_url), Some(l_url)) = (active_url, last_url) {
                    if let Some(engine) = self.engines.get_mut(&active_id) {
                        engine.navigate(&l_url);
                    }
                    if let Some(engine) = self.engines.get_mut(&last_id) {
                        engine.navigate(&a_url);
                    }
                    self.status_message = "Panes swapped".into();
                }
            } else {
                self.status_message = "No previous pane to swap with".into();
            }
        } else {
            self.status_message = "No previous pane".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_url_with_scheme() {
        assert!(AppState::looks_like_url("https://example.com"));
        assert!(AppState::looks_like_url("http://example.com"));
        assert!(AppState::looks_like_url("aileron://welcome"));
        assert!(AppState::looks_like_url("ftp://files.example.com"));
    }

    #[test]
    fn test_looks_like_url_bare_domain() {
        assert!(AppState::looks_like_url("example.com"));
        assert!(AppState::looks_like_url("www.google.com"));
        assert!(AppState::looks_like_url("rust-lang.org"));
        assert!(AppState::looks_like_url("sub.domain.example.org"));
    }

    #[test]
    fn test_looks_like_url_rejects_non_urls() {
        assert!(!AppState::looks_like_url("quit"));
        assert!(!AppState::looks_like_url("vs"));
        assert!(!AppState::looks_like_url(""));
        assert!(!AppState::looks_like_url("hello world"));
        // "file.txt" looks like a domain (bare domain detection is intentionally permissive)
    }

    #[test]
    fn test_looks_like_url_bare_domain_with_path() {
        // Contains '/' so won't match bare domain rule, but doesn't have ://
        assert!(!AppState::looks_like_url("example.com/path")); // no scheme
    }

    #[test]
    fn test_looks_like_url_edge_cases() {
        assert!(!AppState::looks_like_url("a.b")); // TLD "b" is only 1 char
        assert!(!AppState::looks_like_url(".com")); // starts with dot, first part empty
        assert!(!AppState::looks_like_url("example.")); // trailing dot, last part empty
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

        let active_id = state.wm.active_pane_id();
        assert!(state.marks.get(&active_id).unwrap().contains_key(&'a'));
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
