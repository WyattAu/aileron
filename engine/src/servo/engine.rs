//! The `PaneRenderer` trait — the contract for a web rendering backend.
//!
//! S4 stage 1b: the pane *metadata* types (`EngineType`, `PaneState`,
//! `PaneStateManager`) moved to aileron-core `state` — they carry no
//! headed dependency. This module keeps only the trait, which mentions
//! `wry::Rect` (headed) and is implemented by the headed engines
//! (`WryPane`, `ServoPane`).

/// Re-exported so `super::engine::EngineType` paths keep resolving.
pub use aileron_core::state::EngineType;

use url::Url;
use uuid::Uuid;

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
