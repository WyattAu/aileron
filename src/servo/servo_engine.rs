//! Servo-based web engine implementation (Architecture D).
//!
//! This is a skeleton/stub for the future Servo integration.
//! When Servo's embedder API stabilizes (target: Q3 2026), this module
//! will be fleshed out to render web content directly to wgpu textures,
//! eliminating the CPU readback bottleneck of Architecture B.
//!
//! See ADR-005 for the full Architecture D design.

use tracing::warn;
use url::Url;
use uuid::Uuid;
use wry::Rect;

/// Servo-based pane renderer.
///
/// Stub implementation — will render directly to wgpu texture when
/// Servo's embedder API is available.
pub struct ServoPane {
    pane_id: Uuid,
    url: Option<Url>,
    title: String,
}

impl ServoPane {
    pub fn new(pane_id: Uuid) -> Self {
        Self {
            pane_id,
            url: None,
            title: String::new(),
        }
    }
}

impl super::engine::PaneRenderer for ServoPane {
    fn navigate(&mut self, url: &Url) {
        self.url = Some(url.clone());
    }

    fn current_url(&self) -> Option<&Url> {
        self.url.as_ref()
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn execute_js(&self, _js: &str) {
        warn!("ServoPane: execute_js not yet implemented");
    }

    fn reload(&self) {
        warn!("ServoPane: reload not yet implemented");
    }

    fn back(&self) {
        warn!("ServoPane: back not yet implemented");
    }

    fn forward(&self) {
        warn!("ServoPane: forward not yet implemented");
    }

    fn set_bounds(&self, _bounds: Rect) {}

    fn set_visible(&self, _visible: bool) {}

    fn focus(&self) {}

    fn focus_parent(&self) {}

    fn pane_id(&self) -> Uuid {
        self.pane_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::servo::engine::PaneRenderer;

    #[test]
    fn test_servo_pane_new() {
        let id = Uuid::new_v4();
        let pane = ServoPane::new(id);
        assert_eq!(pane.pane_id(), id);
        assert!(pane.current_url().is_none());
        assert!(pane.title().is_empty());
    }

    #[test]
    fn test_servo_pane_navigate() {
        let id = Uuid::new_v4();
        let mut pane = ServoPane::new(id);
        let url = Url::parse("https://example.com").unwrap();
        pane.navigate(&url);
        assert_eq!(pane.current_url(), Some(&url));
    }
}
