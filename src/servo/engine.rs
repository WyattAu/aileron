//! Pane metadata tracking for the window manager.
//!
//! This module provides lightweight per-pane state tracking (URL, title)
//! that can be accessed from `AppState` (which is `Send + Sync`) without
//! touching the `!Send + !Sync` `WryPaneManager`.
//!
//! The actual web rendering is handled by `WryPaneManager` in wry_engine.rs.

use std::collections::HashMap;
use url::Url;
use uuid::Uuid;

/// Which rendering engine a pane uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineType {
    /// WebKitGTK via wry (current, Architecture B).
    #[default]
    WebKit,
    /// Servo (future, Architecture D).
    Servo,
}

/// Trait defining the contract for a web rendering backend.
///
/// Any future rendering engine (wry, Servo, etc.) must implement this trait.
/// This enables per-URL engine routing and clean engine swapping.
///
/// Note: Implementors are typically !Send + !Sync (e.g., wry::WebView has GTK thread affinity).
pub trait PaneRenderer {
    /// Navigate to a URL.
    fn navigate(&mut self, url: &Url);

    /// Get the current URL.
    fn current_url(&self) -> Option<&Url>;

    /// Get the current page title.
    fn title(&self) -> &str;

    /// Execute JavaScript (fire-and-forget).
    fn execute_js(&self, js: &str);

    /// Reload the current page.
    fn reload(&self);

    /// Navigate back in history.
    fn back(&self);

    /// Navigate forward in history.
    fn forward(&self);

    /// Set the position and size of this pane.
    fn set_bounds(&self, bounds: wry::Rect);

    /// Show or hide the pane.
    fn set_visible(&self, visible: bool);

    /// Focus the pane for keyboard input.
    fn focus(&self);

    /// Move focus back to the parent window.
    fn focus_parent(&self);

    /// Get the pane ID.
    fn pane_id(&self) -> Uuid;
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
/// is handled by WryPaneManager in main.rs.
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
    pub fn create_pane(&mut self, pane_id: Uuid, initial_url: Url) {
        let pane = PaneState::new(pane_id, initial_url);
        self.panes.insert(pane_id, pane);
    }

    /// Remove state tracking for a closed pane.
    pub fn remove_pane(&mut self, pane_id: &Uuid) {
        self.panes.remove(pane_id);
    }

    /// Get a mutable reference to the pane state.
    pub fn get_mut(&mut self, pane_id: &Uuid) -> Option<&mut PaneState> {
        self.panes.get_mut(pane_id)
    }

    /// Get an immutable reference to the pane state.
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

        manager.create_pane(id1, url.clone());
        manager.create_pane(id2, url.clone());

        assert!(manager.get(&id1).is_some());
        assert!(manager.get(&id2).is_some());
        assert!(manager.get(&Uuid::new_v4()).is_none());

        manager.remove_pane(&id1);
        assert!(manager.get(&id1).is_none());
        assert!(manager.get(&id2).is_some());
    }
}
