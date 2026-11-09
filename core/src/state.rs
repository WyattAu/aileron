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
