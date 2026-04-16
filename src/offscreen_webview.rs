//! Offscreen webview rendering module.
//!
//! Architecture B: webviews render into `gtk::OffscreenWindow` buffers,
//! pixel data is captured via `get_pixbuf()`, uploaded to wgpu textures,
//! and displayed as egui `Image` widgets.
//!
//! This eliminates the winit+GTK toolkit conflict that caused crashes
//! on Wayland and required XWayland workarounds.

use std::collections::HashMap;

use tracing::{info, warn};
use url::Url;
use uuid::Uuid;
use wry::WebViewBuilderExtUnix;

#[cfg(target_os = "linux")]
use gtk::prelude::{GtkWindowExt, OffscreenWindowExt, WidgetExt};

/// Pixel data captured from an offscreen webview.
#[derive(Debug, Clone)]
pub struct FrameData {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bytes per row (may include padding).
    pub rowstride: u32,
    /// BGRA pixel data (matches `gdk::Pixbuf` format on little-endian).
    pub pixels: Vec<u8>,
}

/// A single webview pane rendered offscreen.
///
/// The wry `WebView` is embedded in a `gtk::OffscreenWindow` via
/// `build_gtk`. No visible window is created — rendering happens
/// entirely in the offscreen buffer.
pub struct OffscreenWebView {
    /// GTK offscreen window that hosts the webview.
    #[cfg(target_os = "linux")]
    offscreen: gtk::OffscreenWindow,
    /// The wry WebView (same API as before, just different container).
    webview: wry::WebView,
    /// Pane identifier (matches BSP tree node).
    pane_id: Uuid,
    /// Current URL.
    url: Url,
    /// Page title.
    title: String,
    /// Cached pixel data from last frame capture.
    frame: Option<FrameData>,
    /// Current dimensions.
    width: i32,
    height: i32,
    /// Whether the webview content has changed since last capture.
    dirty: bool,
}

impl OffscreenWebView {
    /// Create a new offscreen webview pane.
    ///
    /// The webview is embedded in a `gtk::OffscreenWindow` via wry's
    /// `build_gtk` method. No visible window is created.
    #[cfg(target_os = "linux")]
    pub fn new(
        pane_id: Uuid,
        initial_url: &Url,
        width: i32,
        height: i32,
    ) -> Result<Self, wry::Error> {
        // Create the offscreen container
        let offscreen = gtk::OffscreenWindow::new();
        offscreen.set_default_size(width, height);

        // Build a minimal wry WebView for the prototype.
        // TODO: During migration, wire up custom protocols, IPC, navigation handler, etc.
        let builder = wry::WebViewBuilder::new()
            .with_url(initial_url.as_str())
            .with_devtools(cfg!(debug_assertions));

        // Embed the webview into the offscreen window (not visible on screen)
        let webview = builder.build_gtk(&offscreen)?;

        // Show the offscreen window so GTK renders the webview
        offscreen.show_all();

        info!(
            "OffscreenWebView {} created ({}x{}) -> {}",
            &pane_id.to_string()[..8],
            width,
            height,
            initial_url.as_str()
        );

        Ok(Self {
            offscreen,
            webview,
            pane_id,
            url: initial_url.clone(),
            title: String::new(),
            frame: None,
            width,
            height,
            dirty: true,
        })
    }

    /// Navigate to a URL.
    pub fn navigate(&mut self, url: &Url) {
        if let Err(e) = self.webview.load_url(url.as_str()) {
            warn!("Failed to navigate to {}: {}", url, e);
        } else {
            self.url = url.clone();
            self.dirty = true;
        }
    }

    /// Execute JavaScript (fire-and-forget).
    pub fn execute_js(&self, js: &str) {
        if let Err(e) = self.webview.evaluate_script(js) {
            warn!("JS evaluation error: {}", e);
        }
        // Mark dirty since JS might change the page
        // Note: we can't set self.dirty here because &self
    }

    /// Get the current URL.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Get the pane ID.
    pub fn pane_id(&self) -> &Uuid {
        &self.pane_id
    }

    /// Get the page title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Get the current dimensions.
    pub fn dimensions(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    /// Capture the current frame as pixel data.
    ///
    /// Pumps the GTK event loop to ensure the webview has rendered,
    /// then reads the offscreen buffer via `get_pixbuf()`.
    #[cfg(target_os = "linux")]
    pub fn capture_frame(&mut self) -> Option<&FrameData> {
        // Pump GTK event loop to process pending renders
        while gtk::events_pending() {
            gtk::main_iteration();
        }

        // Read pixel data from the offscreen window
        let pixbuf = self.offscreen.pixbuf()?;
        let width = pixbuf.width() as u32;
        let height = pixbuf.height() as u32;
        let rowstride = pixbuf.rowstride() as u32;
        // SAFETY: pixbuf.pixels() returns a pointer to the pixbuf's internal data,
        // which is valid as long as the pixbuf is alive (scoped to this fn).
        let pixels = unsafe { pixbuf.pixels().to_vec() };

        self.frame = Some(FrameData {
            width,
            height,
            rowstride,
            pixels,
        });
        self.dirty = false;

        self.frame.as_ref()
    }

    /// Get the last captured frame without re-capturing.
    pub fn frame(&self) -> Option<&FrameData> {
        self.frame.as_ref()
    }

    /// Resize the offscreen window and webview.
    #[cfg(target_os = "linux")]
    pub fn resize(&mut self, width: i32, height: i32) {
        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.offscreen.set_default_size(width, height);
            self.dirty = true;
        }
    }

    /// Navigate back in history.
    pub fn back(&self) {
        let _ = self.webview.evaluate_script("window.history.back()");
    }

    /// Navigate forward in history.
    pub fn forward(&self) {
        let _ = self.webview.evaluate_script("window.history.forward()");
    }

    /// Reload the current page.
    pub fn reload(&self) {
        let _ = self.webview.evaluate_script("window.location.reload()");
    }

    /// Whether the content has changed since last capture.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the webview as needing re-capture.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

/// Manages all offscreen webview panes.
pub struct OffscreenWebViewManager {
    panes: HashMap<Uuid, OffscreenWebView>,
}

impl Default for OffscreenWebViewManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OffscreenWebViewManager {
    pub fn new() -> Self {
        Self {
            panes: HashMap::new(),
        }
    }

    /// Create a new offscreen webview pane.
    #[cfg(target_os = "linux")]
    pub fn create_pane(
        &mut self,
        pane_id: Uuid,
        initial_url: &Url,
        width: i32,
        height: i32,
    ) -> Result<(), wry::Error> {
        let pane = OffscreenWebView::new(pane_id, initial_url, width, height)?;
        self.panes.insert(pane_id, pane);
        Ok(())
    }

    /// Remove a pane.
    pub fn remove_pane(&mut self, pane_id: &Uuid) {
        if self.panes.remove(pane_id).is_some() {
            info!("Removed OffscreenWebView {}", &pane_id.to_string()[..8]);
        }
    }

    /// Get a mutable reference to a pane.
    pub fn get_mut(&mut self, pane_id: &Uuid) -> Option<&mut OffscreenWebView> {
        self.panes.get_mut(pane_id)
    }

    /// Get an immutable reference to a pane.
    pub fn get(&self, pane_id: &Uuid) -> Option<&OffscreenWebView> {
        self.panes.get(pane_id)
    }

    /// Number of active panes.
    pub fn len(&self) -> usize {
        self.panes.len()
    }

    /// Check if there are no panes.
    pub fn is_empty(&self) -> bool {
        self.panes.is_empty()
    }

    /// Capture frames for all dirty panes.
    #[cfg(target_os = "linux")]
    pub fn capture_dirty_frames(&mut self) {
        for pane in self.panes.values_mut() {
            if pane.is_dirty() {
                pane.capture_frame();
            }
        }
    }

    /// Navigate back in a pane.
    pub fn back(&self, pane_id: &Uuid) {
        if let Some(pane) = self.panes.get(pane_id) {
            pane.back();
        }
    }

    /// Navigate forward in a pane.
    pub fn forward(&self, pane_id: &Uuid) {
        if let Some(pane) = self.panes.get(pane_id) {
            pane.forward();
        }
    }

    /// Resize a pane.
    pub fn resize(&mut self, pane_id: &Uuid, width: i32, height: i32) {
        if let Some(pane) = self.panes.get_mut(pane_id) {
            pane.resize(width, height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_data_fields() {
        let frame = FrameData {
            width: 800,
            height: 600,
            rowstride: 3200,
            pixels: vec![0u8; 800 * 600 * 4],
        };
        assert_eq!(frame.width, 800);
        assert_eq!(frame.height, 600);
        assert_eq!(frame.rowstride, 3200);
        assert_eq!(frame.pixels.len(), 800 * 600 * 4);
    }

    #[test]
    fn test_offscreen_manager_empty() {
        let manager = OffscreenWebViewManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
        assert!(manager.get(&Uuid::new_v4()).is_none());
    }
}
