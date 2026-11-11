//! Servo-based web engine implementation (Architecture D).
//!
//! This is a skeleton/stub for the future Servo integration.
//! When Servo's embedder API stabilizes (target: Q3 2026), this module
//! will be fleshed out to render web content directly to wgpu textures,
//! eliminating the CPU readback bottleneck of Architecture B.
//!
//! See servo_pane_design.md for the full Architecture D design.
//! The texture sharing abstraction lives in texture_share.rs.

use std::cell::RefCell;
use tracing::warn;
use url::Url;
use uuid::Uuid;
use wry::Rect;

use crate::servo::texture_share::{ShareStrategy, TextureShareHandle};

/// Servo-based pane renderer.
///
/// Stub implementation — will render directly to wgpu texture when
/// Servo's embedder API is available. Currently stores a TextureShareHandle
/// for the compositor integration path.
pub struct ServoPane {
    pane_id: Uuid,
    url: Option<Url>,
    title: String,
    texture: RefCell<TextureShareHandle>,
}

impl ServoPane {
    pub fn new(pane_id: Uuid) -> Self {
        Self {
            pane_id,
            url: None,
            title: String::new(),
            texture: RefCell::new(TextureShareHandle::new(
                800,
                600,
                ShareStrategy::CpuReadback,
            )),
        }
    }

    pub fn texture_handle(&self) -> std::cell::Ref<'_, TextureShareHandle> {
        self.texture.borrow()
    }

    pub fn texture_handle_mut(&self) -> std::cell::RefMut<'_, TextureShareHandle> {
        self.texture.borrow_mut()
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

    fn set_bounds(&self, bounds: Rect) {
        let (width, height) = match bounds.size {
            wry::dpi::Size::Logical(logical) => (logical.width, logical.height),
            wry::dpi::Size::Physical(physical) => (physical.width as f64, physical.height as f64),
        };
        let w = width.max(1.0) as u32;
        let h = height.max(1.0) as u32;
        self.texture.borrow_mut().resize(w, h);
    }

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

    #[test]
    fn test_servo_pane_texture_handle() {
        let id = Uuid::new_v4();
        let pane = ServoPane::new(id);
        let handle = pane.texture_handle();
        assert_eq!(handle.texture.width, 800);
        assert_eq!(handle.texture.height, 600);
        assert!(handle.texture.dirty);
    }

    #[test]
    fn test_servo_pane_texture_handle_mut() {
        let id = Uuid::new_v4();
        let pane = ServoPane::new(id);
        {
            let mut handle = pane.texture_handle_mut();
            handle.mark_clean();
            assert!(!handle.texture.dirty);
        }
        assert!(!pane.texture_handle().texture.dirty);
    }

    #[test]
    fn test_servo_pane_set_bounds_resizes_texture() {
        let id = Uuid::new_v4();
        let pane = ServoPane::new(id);
        assert_eq!(pane.texture_handle().texture.width, 800);
        assert_eq!(pane.texture_handle().texture.height, 600);

        let new_bounds = Rect {
            position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
            size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(1024.0, 768.0)),
        };
        pane.set_bounds(new_bounds);
        assert_eq!(pane.texture_handle().texture.width, 1024);
        assert_eq!(pane.texture_handle().texture.height, 768);
        assert!(pane.texture_handle().texture.dirty);
    }

    #[test]
    fn test_servo_pane_set_bounds_clamps_to_one() {
        let id = Uuid::new_v4();
        let pane = ServoPane::new(id);
        let zero_bounds = Rect {
            position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
            size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(0.0, 0.0)),
        };
        pane.set_bounds(zero_bounds);
        assert_eq!(pane.texture_handle().texture.width, 1);
        assert_eq!(pane.texture_handle().texture.height, 1);
    }
}
