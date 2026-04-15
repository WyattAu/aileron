//! Standalone popup window management.
//!
//! Popup windows are independent wry webviews without egui overlay or tiling.
//! They are created by Ctrl+N or `:detach` and manage their own lifecycle.

use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};
use winit::window::{Window, WindowId};

/// A standalone popup browser window — just a wry webview, no egui overlay.
pub struct PopupWindow {
    #[allow(dead_code)]
    pub window: Arc<Window>,
    pub wry_pane: Option<crate::servo::WryPane>,
}

/// Manages standalone popup windows.
pub struct PopupManager {
    windows: HashMap<WindowId, PopupWindow>,
    pub pending_new_window: bool,
    pub pending_popup_window: Option<(WindowId, Arc<Window>)>,
}

impl PopupManager {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            pending_new_window: false,
            pending_popup_window: None,
        }
    }

    /// Create a wry webview for a standalone popup window.
    pub fn init_popup_window(
        &mut self,
        window_id: WindowId,
        window: Arc<Window>,
        url: url::Url,
        blocked_domains: Vec<String>,
        terminal_input_tx: std::sync::Arc<
            std::sync::Mutex<
                std::collections::HashMap<uuid::Uuid, std::sync::mpsc::Sender<String>>,
            >,
        >,
        terminal_resize_tx: std::sync::mpsc::Sender<(uuid::Uuid, u16, u16)>,
    ) {
        let size = window.inner_size();
        let bounds = wry::Rect {
            position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
            size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(
                size.width as f64,
                size.height as f64,
            )),
        };

        match crate::servo::WryPane::new(
            &*window,
            uuid::Uuid::new_v4(),
            url,
            bounds,
            blocked_domains,
            terminal_input_tx,
            terminal_resize_tx,
        ) {
            Ok(wry_pane) => {
                self.windows.insert(
                    window_id,
                    PopupWindow {
                        window,
                        wry_pane: Some(wry_pane),
                    },
                );
                info!("Popup window created");
            }
            Err(e) => {
                warn!("Failed to create popup wry pane: {}", e);
            }
        }
    }

    /// Handle a window event for a popup window.
    pub fn handle_popup_event(&mut self, window_id: WindowId, event: &winit::event::WindowEvent) {
        match event {
            winit::event::WindowEvent::CloseRequested => {
                info!("Popup window closed");
                if let Some(mut popup) = self.windows.remove(&window_id) {
                    popup.wry_pane.take();
                }
            }
            winit::event::WindowEvent::Resized(physical_size) => {
                if physical_size.width > 0 && physical_size.height > 0
                    && let Some(ref mut popup) = self.windows.get_mut(&window_id)
                    && let Some(ref mut pane) = popup.wry_pane
                {
                    let bounds = wry::Rect {
                        position: wry::dpi::Position::Logical(
                            wry::dpi::LogicalPosition::new(0.0, 0.0),
                        ),
                        size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(
                            physical_size.width as f64,
                            physical_size.height as f64,
                        )),
                    };
                    pane.set_bounds(bounds);
                }
            }
            winit::event::WindowEvent::Destroyed => {
                self.windows.remove(&window_id);
            }
            _ => {}
        }
    }

    pub fn contains_key(&self, window_id: &WindowId) -> bool {
        self.windows.contains_key(window_id)
    }
}

impl Default for PopupManager {
    fn default() -> Self {
        Self::new()
    }
}
