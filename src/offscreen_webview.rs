//! Offscreen webview rendering module.
//!
//! Architecture B: webviews render into `gtk::OffscreenWindow` buffers,
//! pixel data is captured via `get_pixbuf()`, uploaded to wgpu textures,
//! and displayed as egui `Image` widgets.
//!
//! This eliminates the winit+GTK toolkit conflict that caused crashes
//! on Wayland and required XWayland workarounds.

use std::collections::HashMap;
use std::sync::mpsc;

use tracing::{info, warn};
use url::Url;
use uuid::Uuid;
use wry::{PageLoadEvent, WebViewBuilder};
use wry::WebViewBuilderExtUnix;

use crate::servo::wry_engine::{WryEvent, aileron_welcome_page, aileron_new_tab_page, aileron_settings_page, file_browser_page, percent_decode, html_escape};

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
    /// Receiver for navigation events from wry callbacks.
    event_rx: mpsc::Receiver<WryEvent>,
}

impl OffscreenWebView {
    /// Create a new offscreen webview pane with full wry builder configuration.
    ///
    /// The webview is embedded in a `gtk::OffscreenWindow` via wry's
    /// `build_gtk` method. No visible window is created.
    #[cfg(target_os = "linux")]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pane_id: Uuid,
        initial_url: &Url,
        width: i32,
        height: i32,
        blocked_domains: Vec<String>,
        devtools: bool,
    ) -> Result<Self, wry::Error> {
        Self::new_with_privacy(
            pane_id,
            initial_url,
            width,
            height,
            blocked_domains,
            true,
            true,
            devtools,
        )
    }

    /// Create a new offscreen webview pane with privacy settings.
    #[cfg(target_os = "linux")]
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_privacy(
        pane_id: Uuid,
        initial_url: &Url,
        width: i32,
        height: i32,
        blocked_domains: Vec<String>,
        https_upgrade_enabled: bool,
        tracking_protection_enabled: bool,
        devtools: bool,
    ) -> Result<Self, wry::Error> {
        let offscreen = gtk::OffscreenWindow::new();
        offscreen.set_default_size(width, height);

        let pid = pane_id;
        let url_str = initial_url.as_str().to_string();
        let (event_tx, event_rx) = mpsc::channel();

        let devtools = cfg!(debug_assertions) || devtools;

        let https_safe_list = if https_upgrade_enabled {
            crate::net::privacy::load_https_safe_list()
        } else {
            std::collections::HashSet::new()
        };
        let https_upgrade = https_upgrade_enabled;

        let upgrade_tx = event_tx.clone();

        let privacy_script =
            crate::net::privacy::privacy_initialization_script(tracking_protection_enabled);

        let builder = WebViewBuilder::new()
            .with_url(&url_str)
            .with_devtools(devtools)
            .with_initialization_script(&privacy_script)
            .with_custom_protocol("aileron".into(), {
                let open_tx = event_tx.clone();
                move |_webview_id, req| {
                    let path = req.uri().path().trim_start_matches('/');
                    let html = match path {
                        "new" => aileron_new_tab_page(),
                        "terminal" => r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Terminal</title><style>body{background:#141414;color:#4db4ff;font-family:monospace;display:flex;align-items:center;justify-content:center;height:100vh;margin:0}</style></head><body><p>Native terminal active</p></body></html>"#.into(),
                        "open" => {
                            if let Some(query) = req.uri().query()
                                && let Some(path_param) = query.split('&')
                                    .find(|p| p.starts_with("path="))
                                    .map(|p| &p[5..])
                            {
                                let filepath = percent_decode(path_param);
                                let _ = open_tx.send(WryEvent::OpenFile {
                                    path: filepath.clone(),
                                });
                            }
                            "<!DOCTYPE html><html><body style='background:#141414;color:#4db4ff;font-family:monospace;padding:2em'>Opening file...</body></html>".into()
                        }
                        "files" => file_browser_page(req.uri()),
                        "error" => {
                            let msg = req.uri().query()
                                .and_then(|q| q.split('&')
                                    .find(|p| p.starts_with("msg="))
                                    .map(|p| percent_decode(&p[4..])))
                                .unwrap_or_else(|| "Unknown error".into());
                            format!(
                                r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Error</title>
<style>
body {{ background: #141414; color: #ff6b6b; font-family: monospace; display: flex; align-items: center; justify-content: center; height: 100vh; }}
.error {{ text-align: center; padding: 2em; background: #1a1a1a; border-radius: 8px; border: 1px solid #ff6b6b; }}
h2 {{ color: #ff6b6b; }} p {{ color: #888; margin-top: 1em; }}
a {{ color: #4db4ff; }}
</style></head><body>
<div class="error"><h2>Pane Error</h2><p>{}</p>
<p><a href="aileron://new">Open new tab</a></p></div>
</body></html>"#,
                                html_escape(&msg)
                            )
                        }
                        "settings" => aileron_settings_page(),
                        _ => aileron_welcome_page(),
                    };
                    wry::http::Response::builder()
                        .header("Content-Type", "text/html")
                        .body(html.into_bytes().into())
                        .unwrap()
                }
            })
            .with_ipc_handler({
                let ipc_tx = event_tx.clone();
                let ipc_pid = pane_id;
                move |req: wry::http::Request<String>| {
                    let _ = ipc_tx.send(WryEvent::IpcMessage {
                        pane_id: ipc_pid,
                        message: req.into_body(),
                    });
                }
            })
            .with_navigation_handler(move |url: String| {
                if let Ok(parsed) = url::Url::parse(&url)
                    && let Some(host) = parsed.host_str() {
                        let host_lower = host.to_lowercase();

                        if blocked_domains.iter().any(|d: &String| {
                            let d_lower = d.to_lowercase();
                            host_lower == d_lower || host_lower.ends_with(&format!(".{}", d_lower))
                        }) {
                            return false;
                        }

                        if https_upgrade
                            && parsed.scheme() == "http"
                            && crate::net::privacy::is_https_safe(host, &https_safe_list)
                        {
                            if let Some(https_url) =
                                crate::net::privacy::should_upgrade_to_https(
                                    &url, &https_safe_list,
                                )
                            {
                                let _ = upgrade_tx.send(WryEvent::HttpsUpgraded {
                                    pane_id: pid,
                                    from: url,
                                    to: https_url,
                                });
                            }
                            return false;
                        }
                    }
                true
            })
            .with_on_page_load_handler({
                let tx = event_tx.clone();
                move |event: PageLoadEvent, url: String| {
                    let _ = tx.send(match event {
                        PageLoadEvent::Started => WryEvent::LoadStarted {
                            pane_id: pid,
                            url: url.clone(),
                        },
                        PageLoadEvent::Finished => WryEvent::LoadComplete { pane_id: pid, url },
                    });
                }
            })
            .with_document_title_changed_handler({
                let title_tx = event_tx.clone();
                move |title: String| {
                    let _ = title_tx.send(WryEvent::TitleChanged {
                        pane_id: pid,
                        title,
                    });
                }
            })
            .with_download_started_handler({
                let dl_tx = event_tx.clone();
                move |url: String, suggested_path: &mut std::path::PathBuf| {
                    if let Some(downloads_dir) = directories::UserDirs::new()
                        .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
                    {
                        let filename = suggested_path
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| {
                                url::Url::parse(&url)
                                    .ok()
                                    .and_then(|u| {
                                        u.path().rsplit('/').next().map(|s| s.to_string())
                                    })
                                    .unwrap_or_else(|| "download".to_string())
                            });
                        *suggested_path = downloads_dir.join(&filename);
                        let _ = dl_tx.send(WryEvent::DownloadStarted {
                            pane_id: pid,
                            url: url.clone(),
                            filename: filename.clone(),
                        });
                    }
                    true
                }
            });

        let webview = builder.build_gtk(&offscreen)?;
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
            event_rx,
        })
    }

    /// Drain pending events from the webview.
    pub fn drain_events(&mut self) -> Vec<WryEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            if let WryEvent::HttpsUpgraded { to, .. } = &event
                && let Ok(https_url) = Url::parse(to)
            {
                self.url = https_url;
                self.dirty = true;
                let _ = self.webview.load_url(to);
            }
            events.push(event);
        }
        events
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
    #[cfg(target_os = "linux")]
    pub fn capture_frame(&mut self) -> Option<&FrameData> {
        while gtk::events_pending() {
            gtk::main_iteration();
        }

        let pixbuf = self.offscreen.pixbuf()?;
        let width = pixbuf.width() as u32;
        let height = pixbuf.height() as u32;
        let rowstride = pixbuf.rowstride() as u32;
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

    /// Get the last captured frame as RGBA8 data.
    pub fn frame_rgba(&self) -> Option<Vec<u8>> {
        self.frame
            .as_ref()
            .map(|f| bgra_to_rgba(&f.pixels, f.width as usize, f.height as usize, f.rowstride))
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

    /// Print the current page using window.print().
    pub fn print(&self) {
        let _ = self.webview.evaluate_script("window.print()");
    }

    /// Whether the content has changed since last capture.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the webview as needing re-capture.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Forward a mouse event to the webview via JavaScript.
    pub fn forward_mouse_event(
        &mut self,
        event_type: &str,
        x: f64,
        y: f64,
        button: &str,
        modifiers: &str,
    ) {
        let js = format!(
            "(function() {{ \
                var el = document.elementFromPoint({}, {}); \
                if (el) {{ \
                    el.dispatchEvent(new MouseEvent('{}', {{ \
                        clientX: {}, clientY: {}, \
                        button: {}, bubbles: true, cancelable: true, \
                        {} \
                    }})); \
                }} \
            }})()",
            x, y, event_type, x, y, button, modifiers
        );
        self.execute_js(&js);
        self.mark_dirty();
    }

    /// Forward a keyboard event to the webview via JavaScript.
    pub fn forward_key_event(&mut self, event_type: &str, key: &str, code: &str, modifiers: &str) {
        let js = format!(
            "document.dispatchEvent(new KeyboardEvent('{}', {{ \
                key: '{}', code: '{}', bubbles: true, cancelable: true, \
                {} \
            }}))",
            event_type, key, code, modifiers
        );
        self.execute_js(&js);
        self.mark_dirty();
    }

    /// Insert text at the current cursor position (for IME commits and printable chars).
    pub fn insert_text(&mut self, text: &str) {
        let escaped = text.replace('\\', "\\\\").replace('\'', "\\'");
        let js = format!("document.execCommand('insertText', false, '{}')", escaped);
        self.execute_js(&js);
        self.mark_dirty();
    }

    /// Scroll the webview by the given delta.
    pub fn scroll_by(&mut self, dx: f64, dy: f64) {
        let js = format!("window.scrollBy({}, {})", dx, dy);
        self.execute_js(&js);
        self.mark_dirty();
    }

    /// Suppress the right-click context menu.
    pub fn suppress_context_menu(&self) {
        self.execute_js(
            "document.addEventListener('contextmenu', function(e) { e.preventDefault(); });",
        );
    }
}

/// Build a JavaScript modifiers object string from boolean flags.
pub fn modifiers_js(ctrl: bool, alt: bool, shift: bool, meta: bool) -> String {
    let mut parts = Vec::new();
    if ctrl {
        parts.push("ctrlKey: true");
    }
    if alt {
        parts.push("altKey: true");
    }
    if shift {
        parts.push("shiftKey: true");
    }
    if meta {
        parts.push("metaKey: true");
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(", {}", parts.join(", "))
    }
}

/// Convert BGRA pixel data to RGBA.
/// Accounts for `rowstride` padding per row (rowstride >= width * 4).
fn bgra_to_rgba(bgra: &[u8], width: usize, height: usize, rowstride: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    let row_bytes = width * 4;
    let stride = rowstride as usize;
    for row in 0..height {
        let row_start = row * stride;
        let row_end = row_start + row_bytes;
        let row_data = &bgra[row_start..row_end];
        for chunk in row_data.chunks_exact(4) {
            rgba.push(chunk[2]); // R
            rgba.push(chunk[1]); // G
            rgba.push(chunk[0]); // B
            rgba.push(chunk[3]); // A
        }
    }
    rgba
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

    /// Create a new offscreen webview pane with full wry builder configuration.
    #[cfg(target_os = "linux")]
    #[allow(clippy::too_many_arguments)]
    pub fn create_pane(
        &mut self,
        pane_id: Uuid,
        initial_url: &Url,
        width: i32,
        height: i32,
        blocked_domains: Vec<String>,
        devtools: bool,
    ) -> Result<(), wry::Error> {
        let pane = OffscreenWebView::new(
            pane_id,
            initial_url,
            width,
            height,
            blocked_domains,
            devtools,
        )?;
        self.panes.insert(pane_id, pane);
        Ok(())
    }

    /// Create a new offscreen webview pane with privacy settings.
    #[cfg(target_os = "linux")]
    #[allow(clippy::too_many_arguments)]
    pub fn create_pane_with_privacy(
        &mut self,
        pane_id: Uuid,
        initial_url: &Url,
        width: i32,
        height: i32,
        blocked_domains: Vec<String>,
        https_upgrade_enabled: bool,
        tracking_protection_enabled: bool,
        devtools: bool,
    ) -> Result<(), wry::Error> {
        let pane = OffscreenWebView::new_with_privacy(
            pane_id,
            initial_url,
            width,
            height,
            blocked_domains,
            https_upgrade_enabled,
            tracking_protection_enabled,
            devtools,
        )?;
        self.panes.insert(pane_id, pane);
        Ok(())
    }

    /// Drain events from all panes, returning a flat list.
    pub fn drain_all_events(&mut self) -> Vec<(Uuid, WryEvent)> {
        let mut events = Vec::new();
        for (pane_id, pane) in self.panes.iter_mut() {
            for event in pane.drain_events() {
                events.push((*pane_id, event));
            }
        }
        events
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

    /// Iterate over all pane IDs and their mutable references.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&Uuid, &mut OffscreenWebView)> {
        self.panes.iter_mut()
    }

    /// Iterate over all pane IDs and their immutable references.
    pub fn iter(&self) -> impl Iterator<Item = (&Uuid, &OffscreenWebView)> {
        self.panes.iter()
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

    #[test]
    fn test_drain_all_events_empty() {
        let mut manager = OffscreenWebViewManager::new();
        let events = manager.drain_all_events();
        assert!(events.is_empty());
    }

    #[test]
    fn test_bgra_to_rgba_conversion() {
        // BGRA: [B=0x11, G=0x22, R=0x33, A=0xFF]
        let bgra: Vec<u8> = vec![0x11, 0x22, 0x33, 0xFF];
        let rgba = bgra_to_rgba(&bgra, 1, 1, 4);
        assert_eq!(rgba.len(), 4);
        assert_eq!(rgba[0], 0x33); // R
        assert_eq!(rgba[1], 0x22); // G
        assert_eq!(rgba[2], 0x11); // B
        assert_eq!(rgba[3], 0xFF); // A
    }

    #[test]
    fn test_bgra_to_rgba_with_rowstride_padding() {
        // width=2, height=2, rowstride=12 (8 bytes pixel + 4 bytes padding per row)
        // Row 0: [B0 G0 R0 A0 B1 G1 R1 A1 PP PP PP PP]
        // Row 1: [B2 G2 R2 A2 B3 G3 R3 A3 PP PP PP PP]
        let bgra: Vec<u8> = vec![
            0x01, 0x02, 0x03, 0xFF, 0x11, 0x12, 0x13, 0xFE,
            0xAA, 0xBB, 0xCC, 0xDD,
            0x04, 0x05, 0x06, 0xFF, 0x14, 0x15, 0x16, 0xFE,
            0xEE, 0xFF, 0x00, 0x11,
        ];
        let rgba = bgra_to_rgba(&bgra, 2, 2, 12);
        assert_eq!(rgba.len(), 2 * 2 * 4);
        // Row 0, pixel 0
        assert_eq!(rgba[0], 0x03); // R
        assert_eq!(rgba[1], 0x02); // G
        assert_eq!(rgba[2], 0x01); // B
        assert_eq!(rgba[3], 0xFF); // A
        // Row 0, pixel 1
        assert_eq!(rgba[4], 0x13); // R
        assert_eq!(rgba[5], 0x12); // G
        assert_eq!(rgba[6], 0x11); // B
        assert_eq!(rgba[7], 0xFE); // A
        // Row 1, pixel 0
        assert_eq!(rgba[8], 0x06);  // R
        assert_eq!(rgba[9], 0x05);  // G
        assert_eq!(rgba[10], 0x04); // B
        assert_eq!(rgba[11], 0xFF); // A
        // Row 1, pixel 1
        assert_eq!(rgba[12], 0x16); // R
        assert_eq!(rgba[13], 0x15); // G
        assert_eq!(rgba[14], 0x14); // B
        assert_eq!(rgba[15], 0xFE); // A
    }

    #[test]
    fn test_modifiers_js_none() {
        assert_eq!(modifiers_js(false, false, false, false), "");
    }

    #[test]
    fn test_modifiers_js_ctrl_shift() {
        let result = modifiers_js(true, false, true, false);
        assert!(result.contains("ctrlKey: true"));
        assert!(result.contains("shiftKey: true"));
        assert!(!result.contains("altKey"));
        assert!(!result.contains("metaKey"));
        assert!(result.starts_with(", "));
    }

    #[test]
    fn test_modifiers_js_all() {
        let result = modifiers_js(true, true, true, true);
        assert!(result.contains("ctrlKey: true"));
        assert!(result.contains("altKey: true"));
        assert!(result.contains("shiftKey: true"));
        assert!(result.contains("metaKey: true"));
    }
}
