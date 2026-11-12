//! Agent-runtime state types (S4 stage 1b).
//!
//! These are the state pieces the agent (MCP) runtime touches: session
//! capability registry, pane metadata, session/auto-save flags, and the
//! https safe-list cache. They live in `aileron-core` so `CoreState`
//! (and eventually the whole agent runtime) can depend on them without
//! any headed (winit/wry/WebKit) dependency.
//!
//! Headed-only sub-states (UiState, PanelState, TabState, …) stay in the
//! `aileron` crate and move behind `HeadedState`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use url::Url;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// MCP session capabilities (YP-MCP-SESSIONS-002 kappa/phi)
// ---------------------------------------------------------------------------

/// Shared session registry: `Uuid` (session id) → capability set.
///
/// An `Arc<Mutex<…>>` alias so the TCP MCP server thread and the main
/// thread share one map without holding the whole bridge.
pub type SessionRegistry = Arc<std::sync::Mutex<HashMap<Uuid, SessionCaps>>>;

/// Capability set for one agent session (YP-MCP-SESSIONS-002 kappa/phi).
///
/// `None` = unrestricted (the default session and fresh connections).
/// Narrowing is monotone: `session_scope` can only shrink a set that is
/// already `Some`, and can convert `None` to `Some` once. Web content has
/// no path to widen a grant - the only writers are MCP tools carrying the
/// session's own tag, and the router validates every write is a subset.
#[derive(Debug, Clone, Default)]
pub struct SessionCaps {
    /// Allowed pane ids; `None` = every pane (phi).
    pub panes: Option<HashSet<Uuid>>,
    /// Allowed tool classes (read | mutate | irreversible); `None` = all (kappa).
    pub tool_classes: Option<HashSet<String>>,
}

impl SessionCaps {
    /// True when `self` can be narrowed to `narrower` (monotone shrinking).
    #[must_use]
    pub fn allows_narrowing_to(&self, narrower: &SessionCaps) -> bool {
        let panes_ok = match (&self.panes, &narrower.panes) {
            (None, _) => true,
            (Some(base), Some(new)) => new.is_subset(base),
            (Some(_), None) => false, // widening None->Some back is a widen
        };
        let classes_ok = match (&self.tool_classes, &narrower.tool_classes) {
            (None, _) => true,
            (Some(base), Some(new)) => new.is_subset(base),
            (Some(_), None) => false,
        };
        panes_ok && classes_ok
    }
}

// ---------------------------------------------------------------------------
// Pane metadata (moved from src/servo/engine.rs; PaneRenderer trait stays
// in the main crate — it mentions wry::Rect, a headed dependency)
// ---------------------------------------------------------------------------

/// Which rendering engine a pane uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineType {
    /// WebKitGTK via wry (current, Architecture B).
    #[default]
    WebKit,
    /// Servo (future, Architecture D).
    Servo,
}

/// Per-pane state: tracks the URL and title for a pane.
///
/// Stores metadata for a pane so that `AppState` can access it
/// without touching the `!Send + !Sync` `WryPaneManager`.
pub struct PaneState {
    pane_id: Uuid,
    url: Url,
    title: String,
    engine_type: EngineType,
}

impl PaneState {
    pub fn new(pane_id: Uuid, url: Url) -> Self {
        let title = url.to_string();
        Self {
            pane_id,
            url,
            title,
            engine_type: EngineType::default(),
        }
    }
}

impl PaneState {
    /// Navigate to a URL.
    pub fn navigate(&mut self, url: &Url) {
        self.url = url.clone();
        self.title = url.to_string();
    }

    /// Get the current URL.
    #[must_use]
    pub fn current_url(&self) -> Option<&Url> {
        Some(&self.url)
    }

    /// Get the current page title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Get the pane ID this state is associated with.
    pub fn pane_id(&self) -> Uuid {
        self.pane_id
    }

    /// Get the rendering engine type for this pane.
    pub fn engine_type(&self) -> EngineType {
        self.engine_type
    }
}

/// Registry of pane states, one per BSP leaf.
///
/// Manages URL/title metadata for each pane. The actual web rendering
/// is handled by the headed `WryPaneManager`.
pub struct PaneStateManager {
    panes: HashMap<Uuid, PaneState>,
}

impl PaneStateManager {
    pub fn new() -> Self {
        Self {
            panes: HashMap::new(),
        }
    }

    /// Create state tracking for a new pane.
    pub fn create_pane(
        &mut self,
        pane_id: Uuid,
        initial_url: Url,
        _interceptor_registry: Option<
            std::sync::Arc<crate::extensions::web_request::WebRequestInterceptorRegistry>,
        >,
    ) {
        let pane = PaneState::new(pane_id, initial_url);
        self.panes.insert(pane_id, pane);
    }

    /// Remove state tracking for a closed pane.
    pub fn remove_pane(&mut self, pane_id: &Uuid) {
        self.panes.remove(pane_id);
    }

    /// Get a mutable reference to the pane state.
    #[must_use]
    pub fn get_mut(&mut self, pane_id: &Uuid) -> Option<&mut PaneState> {
        self.panes.get_mut(pane_id)
    }

    /// Get an immutable reference to the pane state.
    #[must_use]
    pub fn get(&self, pane_id: &Uuid) -> Option<&PaneState> {
        self.panes.get(pane_id)
    }

    /// Get all registered pane IDs.
    pub fn pane_ids(&self) -> Vec<Uuid> {
        self.panes.keys().copied().collect()
    }
}

impl Default for PaneStateManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Session/auto-save flags (moved from src/app/mod.rs; agent runtime writes
// `session_dirty` from MCP command arms)
// ---------------------------------------------------------------------------

/// Session bookkeeping: quit flags, marks, quickmarks, auto-save dirty bit.
pub struct SessionState {
    pub should_quit: bool,
    pub session_dirty: bool,
    pub last_auto_save: std::time::Instant,
    /// Per-pane `f`/`'` marks (pane id → char → fraction of scrollable height).
    pub marks: HashMap<Uuid, HashMap<char, f64>>,
    pub pending_mark_action: Option<char>,
    pub pending_mark_set: Option<char>,
    pub pending_mark_jump: Option<f64>,
    pub quickmarks: HashMap<String, String>,
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

/// Derived-value cache so status-bar/statusline renders avoid recompute.
pub struct CacheState {
    pub cached_pane_count: usize,
    pub pane_count_dirty: bool,
    pub config_json_cache: String,
    pub config_json_dirty: bool,
    pub https_safe_list_cache: Option<std::sync::Arc<HashSet<String>>>,
    pub https_safe_list_debug_flag: bool,
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

// ---------------------------------------------------------------------------
// CoreState — the agent-runtime state (S4 stage 1b)
// ---------------------------------------------------------------------------

/// The subset of `AppState` the agent (MCP) runtime touches: pane tree,
/// database, session registry, engine metadata, config, downloads,
/// extensions, session flags, and the derived-value caches.
///
/// Headed-only state (palette, keybindings, UI sub-states, profilers)
/// lives in `HeadedState` in the `aileron` crate; `AppState` is
/// `{ core: CoreState, headed: Option<HeadedState> }` with `headed: None`
/// in agent mode.
pub struct CoreState {
    pub wm: crate::wm::BspTree,
    pub db: Option<rusqlite::Connection>,
    /// Session capability registry (Era-2, YP-MCP-SESSIONS-002): shared with
    /// the TCP MCP server thread; the router enforces phi from here.
    pub session_registry: SessionRegistry,
    /// Enterprise audit ring (Era-3): every dispatched or denied MCP command
    /// with provenance (session, tool, class, pane, epoch). Cap 1000, main-
    /// thread only (the router writes; the audit tool reads via command arm).
    pub audit_log: std::cell::RefCell<std::collections::VecDeque<String>>,
    /// Web engine manager — one engine instance per pane.
    pub engines: PaneStateManager,
    /// User configuration.
    pub config: crate::config::Config,
    /// MCP-requested pane creations: (pane_id, url). Consumed by the
    /// headed main loop which creates the real offscreen webview.
    pub pending_mcp_pane_creations: std::collections::VecDeque<(Uuid, Url)>,
    /// Per-pane navigation epoch (YP-MCP-NAV-EPOCH-001): advanced by every
    /// MCP navigate; reads can check freshness against the committed value.
    pub mcp_nav_epoch: HashMap<Uuid, u64>,
    /// Highest navigation epoch known to have finished loading per pane.
    /// Reads gate on nav_epoch <= settled_epoch (read-your-writes).
    pub mcp_settled_epoch: HashMap<Uuid, u64>,
    /// Workspace name requested for restore. Consumed by the headed loop.
    pub pending_workspace_restore: Option<String>,
    /// Name of the currently active workspace. Updated on workspace
    /// save, load, and restore.
    pub current_workspace_name: String,
    /// Extension manager — loads and manages WebExtensions.
    /// Wrapped in Arc<RwLock<>> so readers don't block each other.
    pub extension_manager: Arc<parking_lot::RwLock<crate::extensions::ExtensionManager>>,
    /// Download manager — handles file downloads with progress tracking.
    pub download_manager: crate::downloads::DownloadManager,
    /// D1-04: Update checker state.
    pub update_checker: crate::update_check::UpdateChecker,
    /// Queue of pending wry actions requested by the user (headed or
    /// offscreen consumer drains it each frame). Lives on CoreState
    /// because the MCP command arm queues actions in agent mode too.
    pub pending_wry_actions: std::collections::VecDeque<WryAction>,
    /// Pane ID pending close from tab sidebar click. Consumed by the
    /// headed loop in about_to_wait.
    pub pending_tab_close: Option<Uuid>,
    /// Path to save reader mode content to (set by :reader-save command).
    pub pending_reader_save_path: Option<std::path::PathBuf>,
    /// Frame pacing (agent offscreen renders also advance these).
    pub first_frame: bool,
    pub resize_pending: bool,
    pub frame_count: u64,
    /// Session bookkeeping: quickmarks, marks, auto-save dirty bit.
    pub session: SessionState,
    /// Derived-value cache (pane count, config JSON, https safe-list).
    pub cache: CacheState,
    /// Autofill credentials state (offscreen IPC writes it from the
    /// bitwarden mobile lane — agent-reachable).
    pub autofill: AutofillState,
    /// Per-pane tracking of already-injected content script IDs (the
    /// offscreen event path injects in agent mode).
    pub injected_content_script_ids: HashMap<Uuid, std::collections::HashSet<String>>,
    /// Panes in private mode (no history/visit recording). Pane privacy
    /// metadata — read by the visit recorder and the MCP pane tools.
    pub private_pane_ids: std::collections::HashSet<Uuid>,
    /// Bitwarden password manager client (S4 2d: the offscreen IPC
    /// autofill arm reads it).
    #[cfg(feature = "passwords")]
    pub bitwarden: crate::passwords::BitwardenClient,
    /// UI feedback state: status message, url-bar text, omnibox results,
    /// find-bar state. Written from agent-reachable paths (offscreen IPC,
    /// MCP command arms) — moved to core in stage 2d.
    pub ui: UiState,
    /// Pane ids that should be terminal panes (S4 2c: moved here — the
    /// MCP dispatch mirrors terminal URLs during workspace restore).
    #[cfg(feature = "terminal")]
    pub terminal_pane_ids: std::collections::HashSet<Uuid>,
}

fn core_db_path() -> anyhow::Result<std::path::PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "aileron", "Aileron")
        .ok_or_else(|| anyhow::anyhow!("Failed to determine project directories"))?;
    Ok(dirs.data_dir().to_path_buf().join("aileron.db"))
}

fn core_extensions_dir() -> std::path::PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.data_dir().join("extensions"))
        .unwrap_or_else(|| std::path::PathBuf::from("./extensions"))
}

impl CoreState {
    /// Build the agent-runtime state. `viewport` sizes the initial BSP pane.
    pub fn new(viewport: crate::wm::Rect, config: crate::config::Config) -> anyhow::Result<Self> {
        // Use homepage from config
        let initial_url = Url::parse(&config.homepage)
            .unwrap_or_else(|_| Url::parse("aileron://welcome").unwrap());
        let wm = crate::wm::BspTree::new(viewport, initial_url.clone());

        let db_path = core_db_path()?;
        let db = match std::fs::create_dir_all(
            db_path
                .parent()
                .expect("db_path must have a parent directory"),
        ) {
            Ok(_) => match crate::db::open_database(&db_path) {
                Ok(conn) => Some(conn),
                Err(e) => {
                    tracing::warn!("Failed to open database: {}", e);
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Failed to create database directory: {}", e);
                None
            }
        };

        // Create web engine manager (wry via WebEngine trait; Servo planned per ADR-001)
        let mut engines = PaneStateManager::new();
        let root_pane_id = wm.active_pane_id();
        engines.create_pane(root_pane_id, initial_url, None);

        // Load quickmarks from database; seed defaults if none exist
        let mut quickmarks = if let Some(ref conn) = db {
            crate::db::quickmarks::load_quickmarks(conn).unwrap_or_default()
        } else {
            HashMap::new()
        };
        if quickmarks.is_empty() {
            let defaults: &[(&str, &str)] = &[
                ("gh", "https://github.com"),
                ("gl", "https://gitlab.com"),
                ("rd", "https://reddit.com"),
            ];
            if let Some(ref conn) = db {
                for &(key, url) in defaults {
                    if let Err(e) = crate::db::quickmarks::set_quickmark(conn, key, url) {
                        tracing::warn!(%e, "Failed to set default quickmark");
                    }
                    quickmarks.insert(key.to_string(), url.to_string());
                }
            } else {
                for &(key, url) in defaults {
                    quickmarks.insert(key.to_string(), url.to_string());
                }
            }
        }

        let extension_manager = Arc::new(parking_lot::RwLock::new(
            crate::extensions::ExtensionManager::new(core_extensions_dir()),
        ));

        Ok(Self {
            wm,
            db,
            session_registry: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            audit_log: std::cell::RefCell::new(std::collections::VecDeque::new()),
            engines,
            config,
            pending_mcp_pane_creations: std::collections::VecDeque::new(),
            mcp_nav_epoch: HashMap::new(),
            mcp_settled_epoch: HashMap::new(),
            pending_workspace_restore: None,
            current_workspace_name: "default".into(),
            extension_manager,
            download_manager: crate::downloads::DownloadManager::new(
                directories::UserDirs::new()
                    .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
                    .unwrap_or_else(|| std::path::PathBuf::from("./Downloads")),
            ),
            update_checker: crate::update_check::UpdateChecker::new(),
            pending_wry_actions: std::collections::VecDeque::new(),
            pending_tab_close: None,
            pending_reader_save_path: None,
            first_frame: true,
            resize_pending: false,
            frame_count: 0,
            session: SessionState {
                quickmarks,
                ..Default::default()
            },
            cache: CacheState::default(),
            injected_content_script_ids: HashMap::new(),
            private_pane_ids: std::collections::HashSet::new(),
            #[cfg(feature = "passwords")]
            bitwarden: crate::passwords::BitwardenClient::new(),
            autofill: AutofillState::default(),
            ui: UiState::default(),
            #[cfg(feature = "terminal")]
            terminal_pane_ids: std::collections::HashSet::new(),
        })
    }
}

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
    pub omnibox_results: Vec<crate::ui::search::SearchItem>,
    pub omnibox_selected: usize,
    pub last_omnibox_query: String,
    pub status_message: String,
    pub accessibility_text: String,
}

/// Autofill credentials state (offscreen IPC writes it from the
/// bitwarden mobile lane — agent-reachable).
#[derive(Default)]
pub struct AutofillState {
    pub available: bool,
    pub username_id: String,
    pub password_id: String,
    pub js: Option<String>,
    pub status_msg: String,
}

impl CoreState {
    /// Clear injected-script tracking for a closed pane.
    pub fn clear_injected_scripts(&mut self, pane_id: uuid::Uuid) {
        self.injected_content_script_ids.remove(&pane_id);
    }

    /// Check if a content script has already been injected for a pane.
    #[must_use]
    pub fn is_script_injected(&self, pane_id: uuid::Uuid, script_key: &str) -> bool {
        self.injected_content_script_ids
            .get(&pane_id)
            .is_some_and(|s| s.contains(script_key))
    }

    /// Record that a content script was injected for a pane.
    pub fn mark_script_injected(&mut self, pane_id: uuid::Uuid, script_key: &str) {
        self.injected_content_script_ids
            .entry(pane_id)
            .or_default()
            .insert(script_key.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pane_state_navigate() {
        let id = Uuid::new_v4();
        let url = Url::parse("https://example.com").unwrap();
        let mut pane = PaneState::new(id, url.clone());
        assert_eq!(pane.current_url().unwrap(), &url);

        let new_url = Url::parse("https://rust-lang.org").unwrap();
        pane.navigate(&new_url);
        assert_eq!(pane.current_url().unwrap(), &new_url);
    }

    #[test]
    fn test_pane_state_pane_id() {
        let id = Uuid::new_v4();
        let url = Url::parse("https://example.com").unwrap();
        let pane = PaneState::new(id, url);
        assert_eq!(pane.pane_id(), id);
    }

    #[test]
    fn test_pane_state_manager() {
        let mut manager = PaneStateManager::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let url = Url::parse("https://example.com").unwrap();

        manager.create_pane(id1, url.clone(), None);
        manager.create_pane(id2, url.clone(), None);

        assert!(manager.get(&id1).is_some());
        assert!(manager.get(&id2).is_some());
        assert!(manager.get(&Uuid::new_v4()).is_none());

        manager.remove_pane(&id1);
        assert!(manager.get(&id1).is_none());
        assert!(manager.get(&id2).is_some());
    }

    /// Era-2 S39 injection-confinement vectors: the taint argument as tests.
    /// A page-influenced agent can call session_scope, but every widening
    /// shape must be rejected (structural taint, YP-SEC-CONFINEMENT-001).
    #[test]
    fn test_narrowing_rejects_all_widening_shapes() {
        let unrestricted = SessionCaps::default();
        // none -> none: identity is fine
        assert!(unrestricted.allows_narrowing_to(&SessionCaps::default()));
        // none -> some: first narrowing is allowed
        assert!(unrestricted.allows_narrowing_to(&SessionCaps {
            panes: None,
            tool_classes: Some(HashSet::from(["read".to_string()])),
        }));
        // some -> none: dropping a grant is WIDENING - rejected
        let scoped = SessionCaps {
            panes: None,
            tool_classes: Some(HashSet::from(["read".to_string()])),
        };
        assert!(!scoped.allows_narrowing_to(&SessionCaps::default()));
        // some -> larger some: rejected
        let wider = SessionCaps {
            panes: None,
            tool_classes: Some(HashSet::from(["read".to_string(), "mutate".to_string()])),
        };
        assert!(!scoped.allows_narrowing_to(&wider));
        // some -> subset: allowed
        assert!(scoped.allows_narrowing_to(&SessionCaps {
            panes: None,
            tool_classes: Some(HashSet::new()),
        }));
    }

    #[test]
    fn test_pane_scope_narrowing() {
        let s1 = uuid::Uuid::new_v4();
        let s2 = uuid::Uuid::new_v4();
        let base = SessionCaps {
            panes: Some(HashSet::from([s1, s2])),
            tool_classes: None,
        };
        // subset of panes: ok
        assert!(base.allows_narrowing_to(&SessionCaps {
            panes: Some(HashSet::from([s1])),
            tool_classes: None,
        }));
        // adding a pane: widening - rejected
        let s3 = uuid::Uuid::new_v4();
        assert!(!base.allows_narrowing_to(&SessionCaps {
            panes: Some(HashSet::from([s1, s3])),
            tool_classes: None,
        }));
        // un-scoping panes entirely: widening - rejected
        assert!(!base.allows_narrowing_to(&SessionCaps::default()));
    }
}
