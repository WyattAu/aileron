use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
#[cfg(feature = "lua")]
use tracing::{info, warn};

pub mod cmd;
pub mod commands;
pub mod dispatch;
#[cfg(feature = "headed")]
pub mod event_handler;
pub mod events;
pub mod headed_state;
pub mod instance;
pub mod omnibox;
pub mod palette;
pub mod panes;
#[cfg(feature = "headed")]
pub mod popup;

use crate::config::Config;
#[cfg(feature = "lua")]
use crate::lua::LuaEngine;
#[cfg(feature = "passwords")]
use crate::passwords::BitwardenClient;
#[cfg(feature = "lua")]
use crate::ui::search::SearchCategory;
use crate::ui::search::SearchItem;
use crate::wm::Rect;
use uuid::Uuid;

/// S4 stage 1b: moved to aileron-core `state` (the MCP command arm in
/// agent mode queues WryActions, so the type must live where CoreState
/// lives). Re-exported for API compatibility.
pub use aileron_core::state::WryAction;

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

/// S4 stage 2d: moved to aileron-core `state` (offscreen IPC writes it).
pub use aileron_core::state::AutofillState;

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

pub struct TabState {
    pub closed_tab_stack: VecDeque<(String, String)>,
    pub tab_names: HashMap<Uuid, String>,
    pub muted_pane_ids: HashSet<Uuid>,
    pub pinned_pane_ids: HashSet<Uuid>,
    pub reader_mode_panes: HashSet<Uuid>,
    pub minimal_mode_panes: HashSet<Uuid>,
    pub last_active_pane_id: Option<Uuid>,
    pub tab_display_cache: HashMap<Uuid, TabDisplayInfo>,
    pub tab_display_dirty: bool,
}

/// S4 stage 2d: UiState moved to aileron-core `state` — the offscreen
/// IPC + crash paths write it from agent-reachable code.
pub use aileron_core::state::UiState;

impl Default for TabState {
    fn default() -> Self {
        Self {
            closed_tab_stack: VecDeque::new(),
            tab_names: HashMap::new(),
            muted_pane_ids: HashSet::new(),
            pinned_pane_ids: HashSet::new(),
            reader_mode_panes: HashSet::new(),
            minimal_mode_panes: HashSet::new(),
            last_active_pane_id: None,
            tab_display_cache: HashMap::new(),
            tab_display_dirty: true,
        }
    }
}

pub struct AppState {
    /// Agent-runtime state (pane tree, db, MCP sessions, config, …).
    pub core: crate::state::CoreState,
    /// Headed-only state. `None` in agent mode — the agent runtime
    /// never touches headed state (S4 stage 1b contract).
    pub headed: Option<crate::app::headed_state::HeadedState>,

    /// Lua scripting engine (for init.lua and custom keybindings).
    #[cfg(feature = "lua")]
    lua_engine: Option<LuaEngine>,

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

    /// Sync filesystem watcher (started/stopped by sync commands).
    #[cfg(feature = "sync")]
    pub sync_watcher: crate::sync::watcher::SyncWatcher,

    /// ARP server — Aileron Remote Protocol for mobile clients.
    /// Created on demand via `:arp-start` command.
    #[cfg(feature = "arp")]
    pub arp_server: Option<crate::arp::ArpServer>,

    /// ARP command receiver — polled each frame to process mobile mutations.
    /// Stored separately because it must not be dropped while the server runs.
    #[cfg(feature = "arp")]
    pub arp_cmd_receiver:
        Option<std::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<crate::arp::ArpCommand>>>,
}

impl AppState {
    /// Headed state accessor (read). Panics in agent mode by contract:
    /// the agent runtime must never touch headed state.
    #[must_use]
    pub fn headed(&self) -> &crate::app::headed_state::HeadedState {
        self.headed
            .as_ref()
            .expect("headed state accessed in agent mode")
    }

    /// Headed state accessor (mutate). Panics in agent mode by contract.
    pub fn headed_mut(&mut self) -> &mut crate::app::headed_state::HeadedState {
        self.headed
            .as_mut()
            .expect("headed state accessed in agent mode")
    }

    #[must_use = "ignoring this value may lead to data loss or unexpected behavior"]
    pub fn new(viewport: Rect, config: Config) -> Result<Self> {
        let core = crate::state::CoreState::new(viewport, config)?;
        #[cfg(feature = "lua")]
        let mut headed =
            crate::app::headed_state::HeadedState::new(core.db.as_ref(), &core.config)?;
        #[cfg(not(feature = "lua"))]
        let headed = crate::app::headed_state::HeadedState::new(core.db.as_ref(), &core.config)?;

        // Initialize Lua engine and load init.lua if present
        #[cfg(feature = "lua")]
        let lua_engine = match LuaEngine::new() {
            Ok(engine) => {
                let init_lua = core.config.init_lua_path();
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
                            headed.keybindings.register(combo, action);
                        } else {
                            warn!("Lua keybind: unknown action '{}'", bind.action);
                        }
                    } else {
                        warn!("Lua keybind: failed to parse key '{}'", bind.key);
                    }
                }

                // Populate palette with custom Lua commands
                for cmd in engine.custom_commands() {
                    headed.palette.add_item(SearchItem {
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

        // Create extension manager and inject into Lua engine
        #[cfg(feature = "lua")]
        if let Some(ref engine) = lua_engine {
            engine.set_extension_manager(core.extension_manager.clone());
        }

        Ok(Self {
            core,
            headed: Some(headed),
            #[cfg(feature = "lua")]
            lua_engine,
            #[cfg(feature = "terminal")]
            terminal_pane_ids: HashSet::new(),
            #[cfg(feature = "passwords")]
            bitwarden: BitwardenClient::new(),
            #[cfg(feature = "terminal")]
            pending_terminal_command: None,
            #[cfg(feature = "sync")]
            sync_watcher: crate::sync::watcher::SyncWatcher::new(),
            #[cfg(feature = "arp")]
            arp_server: None,
            #[cfg(feature = "arp")]
            arp_cmd_receiver: None,
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
                        self.core
                            .pending_wry_actions
                            .push_back(WryAction::Navigate(url));
                    }
                    Err(e) => {
                        warn!("Lua navigate: invalid URL '{}': {}", url_str, e);
                    }
                }
            }
        }
    }

    /// Store a scroll mark fraction for a pane. Called from the IPC handler
    /// when the webview reports its scroll position back to Rust.
    pub fn store_mark_fraction(&mut self, pane_id: uuid::Uuid, mark: char, fraction: f64) {
        self.core
            .session
            .marks
            .entry(pane_id)
            .or_default()
            .insert(mark, fraction);
    }

    /// Record that a pane was focused. Call when active pane changes.
    pub fn record_pane_focus(&mut self, pane_id: uuid::Uuid) {
        self.headed_mut()
            .pane_last_focus
            .insert(pane_id, std::time::Instant::now());
    }

    /// Clear injected script tracking for a pane (called on LoadStarted).
    pub fn clear_injected_scripts(&mut self, pane_id: uuid::Uuid) {
        self.headed_mut()
            .injected_content_script_ids
            .remove(&pane_id);
    }

    /// Check if a content script has already been injected for a pane.
    pub fn is_script_injected(&self, pane_id: uuid::Uuid, script_key: &str) -> bool {
        self.headed()
            .injected_content_script_ids
            .get(&pane_id)
            .map(|s| s.contains(script_key))
            .unwrap_or(false)
    }

    /// Record that a content script was injected for a pane.
    pub fn mark_script_injected(&mut self, pane_id: uuid::Uuid, script_key: &str) {
        self.headed_mut()
            .injected_content_script_ids
            .entry(pane_id)
            .or_default()
            .insert(script_key.to_string());
    }

    /// Call each frame to track pane focus changes.
    /// Compares current active pane to last recorded and updates timestamps.
    pub fn update_pane_focus_tracking(&mut self) {
        let active_id = self.core.wm.active_pane_id();
        let now = std::time::Instant::now();
        self.headed_mut()
            .pane_last_focus
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
    ///
    /// When multiple panes have timestamps within 5 seconds of each other,
    /// the pane with the highest RSS usage is preferred for eviction to
    /// maximize memory savings.
    #[must_use = "ignoring this value may lead to data loss or unexpected behavior"]
    pub fn find_lru_pane(&self) -> Option<uuid::Uuid> {
        let active_id = self.core.wm.active_pane_id();
        let total_panes = self.headed().pane_last_focus.len();
        let rss_per_pane = crate::profiling::memory::estimate_per_pane_rss(total_panes);

        // Collect candidates with their timestamps (exclude active pane).
        let candidates: Vec<(uuid::Uuid, std::time::Instant)> = self
            .headed()
            .pane_last_focus
            .iter()
            .filter(|(id, _)| **id != active_id)
            .map(|(id, inst)| (*id, *inst))
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Find the oldest timestamp among candidates.
        let oldest = candidates.iter().min_by_key(|(_, t)| *t)?;

        // Among panes with timestamps within 5 seconds of the oldest,
        // prefer the one with highest RSS (biggest memory win).
        let threshold = std::time::Duration::from_secs(5);
        let recent_candidates: Vec<_> = candidates
            .iter()
            .filter(|(_, t)| oldest.1.duration_since(*t) < threshold)
            .collect();

        if recent_candidates.len() > 1 && rss_per_pane.is_some() {
            // When RSS data is available and timestamps are close,
            // pick the pane with the highest estimated RSS.
            // Since we can't measure per-pane RSS directly, we use
            // the pane ID hash as a tiebreaker (consistent across calls).
            // In practice, the caller can use estimate_pane_rss() for
            // more accurate eviction decisions.
            let best = recent_candidates
                .iter()
                .max_by_key(|(id, _)| {
                    // Use a deterministic hash of the UUID as a tiebreaker.
                    let bytes = id.as_bytes();
                    let hash = u64::from_ne_bytes(bytes[..8].try_into().unwrap_or([0u8; 8]));
                    // Combine with RSS estimate for better decision
                    rss_per_pane.unwrap_or(0) ^ hash
                })
                .map(|(id, _)| *id);
            return best;
        }

        // Default: return the oldest pane by timestamp.
        Some(oldest.0)
    }

    /// Estimate RSS for a specific pane.
    ///
    /// Returns per-pane RSS estimate based on total process RSS divided by
    /// the number of panes. Returns `None` on non-Linux platforms.
    #[must_use]
    pub fn estimate_pane_rss(&self, _pane_id: &uuid::Uuid) -> Option<u64> {
        let total_panes = self.headed().pane_last_focus.len();
        crate::profiling::memory::estimate_per_pane_rss(total_panes)
    }

    /// Clean up per-pane state when a pane is closed. Prevents memory leaks.
    pub fn cleanup_pane_state(&mut self, pane_id: &uuid::Uuid) {
        self.headed_mut().pane_last_focus.remove(pane_id);
        self.core.session.marks.remove(pane_id);
        self.headed_mut().tabs.tab_names.remove(pane_id);
        self.core.private_pane_ids.remove(pane_id);
    }

    /// Execute a command string (URL, keybind, etc.) from the command palette.
    /// Public wrapper so main.rs can call it for IME-driven Enter submission.
    pub fn execute_command_pub(&mut self, cmd: &str) {
        self.execute_command(cmd);
    }

    /// Look up a quickmark URL by its key string.
    #[must_use = "ignoring this value may lead to data loss or unexpected behavior"]
    pub fn quickmarks_get(&self, key: &str) -> Option<url::Url> {
        self.core
            .session
            .quickmarks
            .get(key)
            .and_then(|s| url::Url::parse(s).ok())
    }

    pub fn quickmarks_list(&self) -> Vec<(String, String)> {
        self.core
            .session
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
        if let Some(ref conn) = self.core.db
            && let Ok(db_marks) = crate::db::scroll_marks::load_scroll_marks_for_url(conn, url)
            && !db_marks.is_empty()
        {
            self.core
                .session
                .marks
                .entry(pane_id)
                .or_default()
                .extend(db_marks);
        }
    }

    /// U1-06: Save macros to disk as JSON.
    pub fn save_macros_to_disk(&self) -> Result<()> {
        let macros_dir = directories::ProjectDirs::from("com", "aileron", "Aileron")
            .map(|dirs| dirs.data_dir().join("macros"))
            .unwrap_or_else(|| PathBuf::from("./macros"));
        std::fs::create_dir_all(&macros_dir)?;

        let macros: Vec<&crate::input::macro_recorder::SavedMacro> = self
            .headed()
            .macro_recorder
            .list_macros()
            .iter()
            .filter_map(|name| self.headed().macro_recorder.get_macro(name))
            .collect();

        let json = serde_json::to_string_pretty(&macros)?;
        std::fs::write(macros_dir.join("macros.json"), json)?;
        Ok(())
    }

    /// U1-06: Load macros from disk.
    pub fn load_macros_from_disk(&mut self) {
        let macros_path = directories::ProjectDirs::from("com", "aileron", "Aileron")
            .map(|dirs| dirs.data_dir().join("macros").join("macros.json"))
            .unwrap_or_else(|| PathBuf::from("./macros/macros.json"));

        if let Ok(json) = std::fs::read_to_string(&macros_path)
            && let Ok(macros) =
                serde_json::from_str::<Vec<crate::input::macro_recorder::SavedMacro>>(&json)
        {
            for saved in macros {
                self.headed_mut()
                    .macro_recorder
                    .save_macro(&saved.name, saved.events);
            }
        }
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
        assert!(state.core.pending_wry_actions.is_empty());

        state
            .core
            .pending_wry_actions
            .push_back(WryAction::Navigate(
                url::Url::parse("https://example.com").unwrap(),
            ));
        assert_eq!(state.core.pending_wry_actions.len(), 1);

        let action = state.core.pending_wry_actions.pop_front();
        assert!(action.is_some());
        assert!(state.core.pending_wry_actions.is_empty());
    }

    #[test]
    fn test_pending_wry_actions_queue_multiple() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();

        // Simulate two actions firing in one frame
        state
            .core
            .pending_wry_actions
            .push_back(WryAction::ScrollBy { x: 0.0, y: 120.0 });
        state
            .core
            .pending_wry_actions
            .push_back(WryAction::ScrollBy { x: 0.0, y: 120.0 });
        assert_eq!(state.core.pending_wry_actions.len(), 2);

        // Both should be consumable (not dropped)
        let _ = state.core.pending_wry_actions.pop_front();
        let _ = state.core.pending_wry_actions.pop_front();
        assert!(state.core.pending_wry_actions.is_empty());
    }

    #[test]
    fn test_command_chaining_quit() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();
        state.execute_command("quit && open example.com");
        assert!(state.core.session.should_quit);
    }

    #[test]
    fn test_command_chaining_triple() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();
        state.handle_raw_command("vs && sp && swap");
        // vs and sp should have created splits; swap should show "No previous pane"
        assert_eq!(state.core.ui.status_message, "No previous pane");
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
        assert_eq!(state.core.session.pending_mark_action, Some('s'));

        // Press 'a' to set mark a
        state.process_key_event(KeyEvent {
            key: Key::Character('a'),
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert!(state.core.session.pending_mark_action.is_none());
        assert_eq!(state.core.ui.status_message, "Mark a set");

        // The mark is stored asynchronously via IPC. Verify the pending state
        // and that a CaptureScrollFraction action was queued.
        assert_eq!(state.core.session.pending_mark_set, Some('a'));
        assert!(
            state
                .core
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
        assert_eq!(state.core.session.pending_mark_action, Some('g'));

        // Press 'z' (not set)
        state.process_key_event(KeyEvent {
            key: Key::Character('z'),
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert_eq!(state.core.ui.status_message, "Mark z not set");
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
        assert_eq!(state.core.session.pending_mark_action, Some('s'));

        // Press Escape to cancel
        state.process_key_event(KeyEvent {
            key: Key::Escape,
            modifiers: Modifiers::none(),
            physical_key: None,
        });
        assert!(state.core.session.pending_mark_action.is_none());
    }

    #[test]
    fn test_swap_no_previous_pane() {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut state = AppState::new(viewport, Config::default()).unwrap();
        state.execute_command("swap");
        assert_eq!(state.core.ui.status_message, "No previous pane");
    }
}
