//! Offscreen webview rendering module.
//!
//! Architecture B: webviews render into `gtk::OffscreenWindow` buffers,
//! pixel data is captured via WebKitGTK's `snapshot()` API (which correctly
//! captures GL-composited content), uploaded to wgpu textures, and displayed
//! as egui `Image` widgets.
//!
//! This eliminates the winit+GTK toolkit conflict that caused crashes
//! on Wayland and required XWayland workarounds.

use std::collections::HashMap;
use std::sync::{Arc, mpsc};

use tracing::{info, warn};
use url::Url;
use uuid::Uuid;
use wry::WebViewBuilderExtUnix;
use wry::{PageLoadEvent, WebViewBuilder};

use crate::servo::wry_engine::{
    WryEvent, aileron_new_tab_page, aileron_settings_page, aileron_welcome_page, file_browser_page,
    html_escape, percent_decode,
};

#[cfg(target_os = "linux")]
use gtk::glib::Cast;
#[cfg(target_os = "linux")]
use gtk::prelude::{BinExt, GtkWindowExt, OffscreenWindowExt, WidgetExt};
#[cfg(target_os = "linux")]
use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};

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
    /// Last time we received any event or frame update from this pane.
    last_activity_time: std::time::Instant,
    /// Whether a load is in progress (set on LoadStarted, cleared on LoadComplete).
    loading: bool,
    /// Reusable buffer for RGBA frame data (avoids per-frame allocation).
    rgba_buffer: Vec<u8>,
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
        popup_blocker: bool,
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
            popup_blocker,
            None,
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
        popup_blocker: bool,
        interceptor_registry: Option<
            Arc<crate::extensions::web_request::WebRequestInterceptorRegistry>,
        >,
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
            .with_initialization_script(crate::servo::wry_engine::ERROR_MONITOR_JS)
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
                // Fire extension onBeforeRequest hooks BEFORE adblock checks
                if let Some(ref registry) = interceptor_registry
                    && registry.has_interceptors()
                {
                    let details = crate::extensions::web_request::RequestDetails {
                        request_id: crate::extensions::types::RequestId(0),
                        url: url::Url::parse(&url).unwrap_or_else(|_| {
                            url::Url::parse("about:blank").unwrap()
                        }),
                        method: "GET".into(),
                        frame_id: crate::extensions::types::FrameId(0),
                        parent_frame_id: crate::extensions::types::FrameId(u32::MAX),
                        tab_id: None,
                        type_: crate::extensions::web_request::ResourceType::MainFrame,
                        origin_url: None,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs_f64() * 1000.0)
                            .unwrap_or(0.0),
                        request_headers: None,
                    };
                    let response = registry.fire_on_before_request(&details);
                    if response.cancel == Some(true) {
                        return false;
                    }
                    if let Some(ref redirect) = response.redirect_url {
                        let _ = upgrade_tx.send(WryEvent::HttpsUpgraded {
                            pane_id: pid,
                            from: url,
                            to: redirect.as_str().to_string(),
                        });
                        return false;
                    }
                }

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
            // Popup blocker: block window.open() / target="_blank" navigations
            .with_new_window_req_handler(move |_url: String, _features: wry::NewWindowFeatures| {
                if popup_blocker {
                    wry::NewWindowResponse::Deny
                } else {
                    wry::NewWindowResponse::Allow
                }
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
                    if is_pdf_url(&url) {
                        return false;
                    }
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
            last_activity_time: std::time::Instant::now(),
            loading: false,
            rgba_buffer: Vec::new(),
        })
    }

    /// Drain pending events from the webview.
    pub fn drain_events(&mut self) -> Vec<WryEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            match &event {
                WryEvent::LoadStarted { .. } => {
                    self.loading = true;
                    self.last_activity_time = std::time::Instant::now();
                }
                WryEvent::LoadComplete { .. } => {
                    self.loading = false;
                    self.last_activity_time = std::time::Instant::now();
                }
                WryEvent::TitleChanged { .. } => {
                    self.last_activity_time = std::time::Instant::now();
                }
                WryEvent::HttpsUpgraded { to, .. } => {
                    if let Ok(https_url) = Url::parse(to) {
                        self.url = https_url;
                        self.dirty = true;
                        let _ = self.webview.load_url(to);
                    }
                    self.last_activity_time = std::time::Instant::now();
                }
                _ => {}
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
            self.loading = true;
            self.last_activity_time = std::time::Instant::now();
        }
    }

    /// Execute JavaScript (fire-and-forget).
    pub fn execute_js(&self, js: &str) {
        if let Err(e) = self.webview.evaluate_script(js) {
            warn!("JS evaluation error: {}", e);
        }
    }

    /// Execute JavaScript with a callback that receives the result as JSON string.
    pub fn execute_js_with_callback(&self, js: &str, callback: impl Fn(String) + Send + 'static) {
        if let Err(e) = self.webview.evaluate_script_with_callback(js, callback) {
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
    ///
    /// Uses WebKitGTK's `snapshot()` API which correctly captures GL-composited
    /// content (unlike `OffscreenWindow::pixbuf()` which only sees cairo surfaces).
    /// Falls back to pixbuf for internal `aileron://` pages (which don't need
    /// snapshot since they're rendered by GTK directly).
    #[cfg(target_os = "linux")]
    pub fn capture_frame(&mut self) -> Option<&FrameData> {
        // Pump the GTK event loop so pending renders complete.
        while gtk::events_pending() {
            gtk::main_iteration();
        }

        // Use snapshot for real web content (captures GL-composited content).
        if self.url.scheme() != "aileron" {
            if let Some(frame) = self.capture_frame_snapshot() {
                self.frame = Some(frame);
                self.dirty = false;
                return self.frame.as_ref();
            }
            // Snapshot failed — fall through to pixbuf.
            warn!(
                "capture_frame: snapshot failed for pane {}, trying pixbuf fallback",
                &self.pane_id.to_string()[..8],
            );
        }

        // Fallback: pixbuf (works for aileron:// pages rendered via cairo).
        self.capture_frame_pixbuf()
    }

    /// Capture frame via WebKitGTK's snapshot API.
    ///
    /// This is the CORRECT way to capture web content from WebKitGTK, as it
    /// handles both software-rendered and GL-composited content. The snapshot
    /// API produces a cairo ImageSurface (ARGB32) regardless of the rendering
    /// pipeline used internally.
    #[cfg(target_os = "linux")]
    fn capture_frame_snapshot(&self) -> Option<FrameData> {
        // Get the WebKitWebView widget embedded in our OffscreenWindow.
        let child = self.offscreen.child()?;
        let webview = child.downcast_ref::<webkit2gtk::WebView>()?;

        // Verify we own the GLib MainContext (required by snapshot()).
        let main_context = gtk::glib::MainContext::ref_thread_default();
        if !main_context.is_owner() {
            warn!("capture_frame_snapshot: not MainContext owner");
            return None;
        }

        // Set up a channel to receive the async snapshot result.
        let (tx, rx) = mpsc::channel::<Result<cairo::Surface, gtk::glib::Error>>();

        // Request snapshot of the visible region.
        webview.snapshot(
            SnapshotRegion::Visible,
            SnapshotOptions::NONE,
            // No cancellable — use gtk::gio to match the gio version webkit2gtk uses.
            None::<&gtk::gio::Cancellable>,
            move |result| {
                let _ = tx.send(result);
            },
        );

        // Pump the GLib main loop until the snapshot callback fires.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let result = loop {
            if let Ok(r) = rx.try_recv() {
                break r;
            }
            if std::time::Instant::now() > deadline {
                warn!("capture_frame_snapshot: timed out after 2s");
                return None;
            }
            if gtk::events_pending() {
                gtk::main_iteration();
            } else {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        };

        let surface: cairo::Surface = match result {
            Ok(s) => s,
            Err(e) => {
                warn!("capture_frame_snapshot: error: {}", e);
                return None;
            }
        };

        // Convert the cairo surface to raw pixel data.
        // snapshot() returns a cairo_image_surface_t (ARGB32 format).
        let raw = surface.to_raw_none();

        // Verify it's actually an image surface.
        // cairo_surface_type_t::Image == 0 (per cairo spec).
        unsafe {
            let surface_type = cairo::ffi::cairo_surface_get_type(raw);
            if surface_type != 0 {
                warn!(
                    "capture_frame_snapshot: surface is not Image (type={})",
                    surface_type
                );
                return None;
            }
        }

        let width = unsafe { cairo::ffi::cairo_image_surface_get_width(raw) as u32 };
        let height = unsafe { cairo::ffi::cairo_image_surface_get_height(raw) as u32 };
        let stride = unsafe { cairo::ffi::cairo_image_surface_get_stride(raw) as u32 };

        if width == 0 || height == 0 {
            warn!(
                "capture_frame_snapshot: zero dimensions {}x{}",
                width, height
            );
            return None;
        }

        // Copy pixel data from the cairo surface.
        // ARGB32 on little-endian = BGRA byte order, matching our FrameData format.
        let pixels = unsafe {
            let data_ptr = cairo::ffi::cairo_image_surface_get_data(raw);
            if data_ptr.is_null() {
                warn!("capture_frame_snapshot: null pixel data");
                return None;
            }
            let len = (stride as usize) * (height as usize);
            std::slice::from_raw_parts(data_ptr, len).to_vec()
        };

        info!(
            "capture_frame_snapshot: pane {} ok: {}x{} stride={} pixels={}",
            &self.pane_id.to_string()[..8],
            width,
            height,
            stride,
            pixels.len(),
        );

        Some(FrameData {
            width,
            height,
            rowstride: stride,
            pixels,
        })
    }

    /// Fallback: capture via OffscreenWindow::pixbuf() (cairo-only rendering).
    ///
    /// Only captures GTK widget-level cairo drawing. Does NOT capture
    /// GL-composited WebKitGTK content. Suitable for aileron:// internal pages
    /// which are rendered through GTK's software path.
    #[cfg(target_os = "linux")]
    fn capture_frame_pixbuf(&mut self) -> Option<&FrameData> {
        let pixbuf = match self.offscreen.pixbuf() {
            Some(p) => p,
            None => {
                warn!(
                    "capture_frame: pixbuf() returned None for pane {}",
                    &self.pane_id.to_string()[..8]
                );
                return None;
            }
        };
        let width = pixbuf.width() as u32;
        let height = pixbuf.height() as u32;
        let rowstride = pixbuf.rowstride() as u32;
        let pixels = unsafe { pixbuf.pixels().to_vec() };
        if width == 0 || height == 0 || pixels.is_empty() {
            warn!(
                "capture_frame: empty pixbuf {}x{} ({} bytes) for pane {}",
                width,
                height,
                pixels.len(),
                &self.pane_id.to_string()[..8]
            );
            return None;
        }
        info!(
            "capture_frame: pane {} ok: {}x{} rowstride={} pixels={}",
            &self.pane_id.to_string()[..8],
            width,
            height,
            rowstride,
            pixels.len(),
        );
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
    ///
    /// Reuses an internal buffer across calls to avoid repeated allocation.
    /// Only reallocates when the frame dimensions change.
    pub fn frame_rgba(&mut self) -> Option<&[u8]> {
        self.frame.as_ref().map(|f| {
            let needed = (f.width as usize) * (f.height as usize) * 4;
            if self.rgba_buffer.capacity() < needed {
                self.rgba_buffer = Vec::with_capacity(needed);
            }
            self.rgba_buffer.clear();

            let bgra = &f.pixels;
            let width = f.width as usize;
            let height = f.height as usize;
            let stride = f.rowstride as usize;
            let row_bytes = width * 4;
            for row in 0..height {
                let src_start = row * stride;
                self.rgba_buffer
                    .extend_from_slice(&bgra[src_start..src_start + row_bytes]);
            }
            for chunk in self.rgba_buffer.chunks_exact_mut(4) {
                chunk.swap(0, 2);
            }

            self.rgba_buffer.as_slice()
        })
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
        self.last_activity_time = std::time::Instant::now();
    }

    /// Check if this pane appears crashed: loading started but no
    /// activity for longer than the given timeout.
    pub fn is_crashed(&self, timeout: std::time::Duration) -> bool {
        self.loading && self.last_activity_time.elapsed() > timeout
    }

    /// Whether a page load is currently in progress.
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// Mark that loading has completed (used by external callers).
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
        if loading {
            self.last_activity_time = std::time::Instant::now();
        }
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

    /// Scroll the webview by the given delta with smooth animation.
    pub fn scroll_by(&mut self, dx: f64, dy: f64) {
        let js = format!(
            "window.scrollBy({{top: {}, left: {}, behavior: 'smooth'}})",
            dy, dx
        );
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

/// Check if a URL points to a PDF resource (by file extension in path).
/// Used to prevent downloading PDFs so WebKitGTK renders them inline.
pub fn is_pdf_url(url: &str) -> bool {
    url::Url::parse(url)
        .map(|u| u.path().to_lowercase().ends_with(".pdf"))
        .unwrap_or(false)
}

#[cfg(test)]
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
        popup_blocker: bool,
    ) -> Result<(), wry::Error> {
        let pane = OffscreenWebView::new(
            pane_id,
            initial_url,
            width,
            height,
            blocked_domains,
            devtools,
            popup_blocker,
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
        popup_blocker: bool,
        interceptor_registry: Option<
            Arc<crate::extensions::web_request::WebRequestInterceptorRegistry>,
        >,
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
            popup_blocker,
            interceptor_registry,
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
            0x01, 0x02, 0x03, 0xFF, 0x11, 0x12, 0x13, 0xFE, 0xAA, 0xBB, 0xCC, 0xDD, 0x04, 0x05,
            0x06, 0xFF, 0x14, 0x15, 0x16, 0xFE, 0xEE, 0xFF, 0x00, 0x11,
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
        assert_eq!(rgba[8], 0x06); // R
        assert_eq!(rgba[9], 0x05); // G
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

    /// Integration test: BSP tree lifecycle without display.
    /// Exercises split, close, navigate, and swap operations.
    #[test]
    fn test_bsp_lifecycle() {
        use crate::wm::rect::Direction;
        use crate::wm::rect::{Rect, SplitDirection};
        use crate::wm::tree::BspTree;

        let initial_url = url::Url::parse("aileron://new").unwrap();
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        let mut tree = BspTree::new(viewport, initial_url);
        let root = tree.active_pane_id();
        assert_eq!(tree.leaf_count(), 1);

        // Split into 2 panes
        let right_id = tree
            .split(root, SplitDirection::Horizontal, 0.5)
            .expect("split should succeed");
        assert_eq!(tree.leaf_count(), 2);

        // Split again for 3 panes
        let _bottom_id = tree
            .split(right_id, SplitDirection::Vertical, 0.5)
            .expect("second split should succeed");
        assert_eq!(tree.leaf_count(), 3);

        // Navigate between panes
        assert!(tree.navigate(Direction::Left).is_some());

        // Verify panes() returns all
        let pane_list = tree.panes();
        assert_eq!(pane_list.len(), 3);

        // Swap pane IDs
        assert!(tree.swap_pane_ids(root, right_id));

        // Close a pane
        tree.close(right_id).expect("close should succeed");
        assert_eq!(tree.leaf_count(), 2);
    }

    /// Integration test: Bookmark + history DB lifecycle.
    #[test]
    fn test_bookmark_history_lifecycle() {
        use crate::db::open_database;

        let file = tempfile::NamedTempFile::new().expect("temp file");
        let conn = open_database(file.path()).expect("open db");

        // Add bookmarks
        let id1 = crate::db::bookmarks::add_bookmark(&conn, "https://github.com", "GitHub")
            .expect("add bookmark");
        assert!(id1 > 0);

        let id2 = crate::db::bookmarks::add_bookmark_with_folder(
            &conn,
            "https://docs.rs",
            "Docs.rs",
            "rust",
        )
        .expect("add bookmark with folder");
        assert!(id2 > 0);

        // List all
        let all = crate::db::bookmarks::all_bookmarks(&conn).expect("list bookmarks");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].url, "https://github.com");
        assert_eq!(all[1].folder, "rust");

        // Search
        let results =
            crate::db::bookmarks::search_bookmarks(&conn, "github", 10).expect("search bookmarks");
        assert_eq!(results.len(), 1);

        // Add history
        let url1 = url::Url::parse("https://example.com").unwrap();
        let url2 = url::Url::parse("https://example.com/page").unwrap();
        let url3 = url::Url::parse("https://rust-lang.org").unwrap();
        crate::db::history::record_visit(&conn, &url1, "Example").expect("record visit");
        crate::db::history::record_visit(&conn, &url2, "Example Page").expect("record visit 2");
        crate::db::history::record_visit(&conn, &url3, "Rust").expect("record visit 3");

        let recent = crate::db::history::recent_entries(&conn, 10).expect("recent history");
        assert_eq!(recent.len(), 3);
        // Most recent first
        assert_eq!(recent[0].url, url3.as_str());

        // Search history
        let found = crate::db::history::search(&conn, "example", 10).expect("search history");
        assert_eq!(found.len(), 2);

        // Clear
        crate::db::history::clear_history(&conn).expect("clear history");
        assert!(
            crate::db::history::recent_entries(&conn, 10)
                .unwrap()
                .is_empty()
        );
    }

    /// Integration test: Site settings per-domain lifecycle.
    #[test]
    fn test_site_settings_lifecycle() {
        use crate::db::open_database;

        let file = tempfile::NamedTempFile::new().expect("temp file");
        let conn = open_database(file.path()).expect("open db");

        // Set zoom for a domain
        crate::db::site_settings::set_site_field(
            &conn,
            "example.com",
            "exact",
            "zoom",
            Some("1.5"),
        )
        .expect("set zoom");

        // Set adblock for another domain
        crate::db::site_settings::set_site_field(
            &conn,
            "ads.example.com",
            "exact",
            "adblock",
            Some("true"),
        )
        .expect("set adblock");

        // Retrieve
        let settings =
            crate::db::site_settings::get_site_settings_for_url(&conn, "https://example.com/page")
                .expect("get settings");
        assert!(!settings.is_empty());
        assert_eq!(settings[0].zoom_level, Some(1.5));

        // Clear zoom by deleting
        crate::db::site_settings::delete_site_settings_for_domain(&conn, "example.com")
            .expect("clear zoom");
    }

    #[test]
    fn test_is_pdf_url() {
        assert!(super::is_pdf_url("https://example.com/doc.pdf"));
        assert!(super::is_pdf_url("https://example.com/path/to/FILE.PDF"));
        assert!(super::is_pdf_url("http://example.com/document.pdf?query=1"));
        assert!(!super::is_pdf_url("https://example.com/page.html"));
        assert!(!super::is_pdf_url("https://example.com/"));
        assert!(!super::is_pdf_url("not a url"));
        assert!(!super::is_pdf_url("https://example.com/pdfhandler"));
    }

    #[test]
    fn test_modifiers_js() {
        assert_eq!(super::modifiers_js(false, false, false, false), "");
        let m = super::modifiers_js(true, false, false, false);
        assert!(m.contains("ctrlKey: true"));
        let m = super::modifiers_js(true, true, true, true);
        assert!(m.contains("ctrlKey: true"));
        assert!(m.contains("altKey: true"));
        assert!(m.contains("shiftKey: true"));
        assert!(m.contains("metaKey: true"));
    }
}
