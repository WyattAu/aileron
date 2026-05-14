//! Wry-based web engine implementation.
//!
//! Uses wry (Tauri's system WebView wrapper) to render web content.
//!
//! Two embedding strategies:
//! - **X11 path** (`build_as_child`): WebView is a child window inside our winit window.
//!   Supports positioning via `set_bounds()`.
//! - **Wayland fallback** (`build_gtk`): WebView is embedded in a standalone GTK window
//!   with a `gtk::Fixed` container. Positioning works via `set_bounds()` but window
//!   placement is compositor-controlled on Wayland.
//!
//! Architecture notes:
//! - `wry::WebView` is `!Send + !Sync` (GTK thread affinity), so WryPane instances
//!   must live on the main thread.
//! - `WryPaneManager` is stored directly in the AileronApp struct in main.rs.
//! - Navigation events are collected via channels since wry callbacks are `Fn` closures.

#[cfg(target_os = "linux")]
use glib_sys;
#[cfg(target_os = "linux")]
use gtk::prelude::{ContainerExt, GtkWindowExt, WidgetExt};
use std::collections::HashMap;
use std::sync::{Arc, mpsc};
use tracing::{info, warn};
use url::Url;
use uuid::Uuid;
#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;
use wry::dpi::{LogicalPosition, LogicalSize, Position, Size};
use wry::raw_window_handle::HasWindowHandle;
use wry::{PageLoadEvent, Rect, WebView, WebViewBuilder};

use super::wry_pages::*;

/// Whether the webview is embedded as a child or in a standalone GTK window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedMode {
    /// Child window inside our winit window (X11 only).
    ChildWindow,
    /// Standalone GTK window (Wayland fallback).
    GtkWindow,
}

/// A single web view pane backed by wry.
///
/// Not Send/Sync — must live on the main thread (GTK requirement on Linux).
pub struct WryPane {
    /// The wry WebView handle.
    webview: WebView,
    /// The pane's BSP UUID.
    pane_id: Uuid,
    /// Current URL (tracked locally for fast access).
    url: Url,
    /// Current page title.
    title: String,
    /// Receiver for navigation events from wry callbacks.
    event_rx: mpsc::Receiver<WryEvent>,
    /// How this pane is embedded.
    embed_mode: EmbedMode,
    /// The GTK window handle (Some on Wayland fallback, None on X11 child).
    #[cfg(target_os = "linux")]
    gtk_window: Option<gtk::Window>,
    /// The GTK Fixed container (Some on Wayland fallback, None on X11 child).
    #[cfg(target_os = "linux")]
    gtk_fixed: Option<gtk::Fixed>,
}

/// Events emitted by the wry webview, sent via channel.
#[derive(Debug, Clone)]
pub enum WryEvent {
    /// Page started loading.
    LoadStarted { pane_id: Uuid, url: String },
    /// Page finished loading.
    LoadComplete { pane_id: Uuid, url: String },
    /// Page title changed.
    TitleChanged { pane_id: Uuid, title: String },
    /// A download was started.
    DownloadStarted {
        pane_id: Uuid,
        url: String,
        filename: String,
    },
    /// Request to open a file (from file browser).
    OpenFile { path: String },
    /// HTTP URL was upgraded to HTTPS.
    HttpsUpgraded {
        pane_id: Uuid,
        from: String,
        to: String,
    },
    /// IPC message from a webview page.
    IpcMessage { pane_id: Uuid, message: String },
}

impl WryPane {
    /// Create a new WryPane, trying the X11 child window path first,
    /// then falling back to a standalone GTK window for Wayland.
    ///
    /// # Arguments
    /// * `parent` - A reference to the parent window (for X11 child embedding).
    /// * `pane_id` - The UUID for this pane (matches BSP tree).
    /// * `initial_url` - The URL to load initially.
    /// * `bounds` - Position and size within the parent window.
    /// * `blocked_domains` - List of domains to block (cloned into closure).
    #[allow(clippy::too_many_arguments)]
    pub fn new<W>(
        parent: &W,
        pane_id: Uuid,
        initial_url: Url,
        bounds: Rect,
        blocked_domains: Vec<String>,
        https_safe_list: std::collections::HashSet<String>,
        devtools: bool,
        popup_blocker: bool,
        interceptor_registry: Option<
            Arc<crate::extensions::web_request::WebRequestInterceptorRegistry>,
        >,
    ) -> Result<Self, wry::Error>
    where
        W: HasWindowHandle,
    {
        let pid = pane_id;
        let url_str = initial_url.as_str().to_string();
        let (event_tx, event_rx) = mpsc::channel();

        let interceptor = interceptor_registry.clone();

        // === Path 1: Try build_as_child (X11) ===
        // Builder is built inline so event_tx isn't lost if this path fails.
        match Self::make_builder_with_privacy(
            &url_str,
            pid,
            event_tx.clone(),
            blocked_domains.clone(),
            https_safe_list.clone(),
            true,
            true,
            devtools,
            popup_blocker,
            interceptor,
        )
        .with_bounds(bounds)
        .build_as_child(parent)
        {
            Ok(webview) => {
                info!(
                    "WryPane {} created as child window -> {}",
                    &pane_id.to_string()[..8],
                    url_str
                );
                return Ok(Self {
                    webview,
                    pane_id,
                    url: initial_url,
                    title: String::new(),
                    event_rx,
                    embed_mode: EmbedMode::ChildWindow,
                    #[cfg(target_os = "linux")]
                    gtk_window: None,
                    #[cfg(target_os = "linux")]
                    gtk_fixed: None,
                });
            }
            Err(e) => {
                warn!(
                    "build_as_child failed for pane {}: {} — trying GTK fallback",
                    &pane_id.to_string()[..8],
                    e
                );
            }
        }

        // === Path 2: GTK window fallback (Wayland) ===
        #[cfg(target_os = "linux")]
        {
            Self::create_gtk_pane(
                pid,
                initial_url,
                bounds,
                event_tx,
                event_rx,
                https_safe_list,
                devtools,
                popup_blocker,
                interceptor_registry,
            )
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(wry::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Offscreen webview not supported on this platform",
            )))
        }
    }

    /// Create a standalone GTK window with embedded wry webview (Wayland-compatible).
    #[cfg(target_os = "linux")]
    #[allow(clippy::too_many_arguments)]
    fn create_gtk_pane(
        pane_id: Uuid,
        initial_url: Url,
        bounds: Rect,
        event_tx: mpsc::Sender<WryEvent>,
        event_rx: mpsc::Receiver<WryEvent>,
        https_safe_list: std::collections::HashSet<String>,
        devtools: bool,
        popup_blocker: bool,
        interceptor_registry: Option<
            Arc<crate::extensions::web_request::WebRequestInterceptorRegistry>,
        >,
    ) -> Result<Self, wry::Error> {
        let url_str = initial_url.as_str().to_string();

        // Extract size from bounds
        let (width, height) = match bounds.size {
            Size::Logical(s) => (s.width, s.height),
            Size::Physical(s) => (s.width as f64, s.height as f64),
        };

        // Create a GTK window
        let gtk_window = gtk::Window::new(gtk::WindowType::Toplevel);
        gtk_window.set_title("Aileron");
        gtk_window.set_default_size(width as i32, height as i32);
        gtk_window.set_decorated(false);

        // Create a Fixed container for the webview
        let fixed = gtk::Fixed::new();
        fixed.set_size_request(width as i32, height as i32);
        fixed.show();

        // Add the Fixed container to the window
        gtk_window.set_child(Some(&fixed));

        // Build the webview inside the GTK container using the SAME event_tx
        let builder = Self::make_builder_with_privacy(
            &url_str,
            pane_id,
            event_tx,
            Vec::new(),
            https_safe_list,
            true,
            true,
            devtools,
            popup_blocker,
            interceptor_registry,
        );

        let webview = builder.build_gtk(&fixed)?;

        #[cfg(target_os = "linux")]
        crate::platform::spellcheck::configure_webkit_spellcheck();

        gtk_window.show();

        info!(
            "WryPane {} created as GTK window (Wayland fallback) -> {}",
            &pane_id.to_string()[..8],
            url_str
        );

        Ok(Self {
            webview,
            pane_id,
            url: initial_url,
            title: String::new(),
            event_rx,
            embed_mode: EmbedMode::GtkWindow,
            gtk_window: Some(gtk_window),
            gtk_fixed: Some(fixed),
        })
    }

    /// Build a WebViewBuilder with common configuration and privacy settings.
    /// The event_tx is moved into the builder's closures.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn make_builder_with_privacy(
        url_str: &str,
        pid: Uuid,
        event_tx: mpsc::Sender<WryEvent>,
        blocked_domains: Vec<String>,
        https_safe_list: std::collections::HashSet<String>,
        https_upgrade_enabled: bool,
        tracking_protection_enabled: bool,
        devtools: bool,
        popup_blocker: bool,
        interceptor_registry: Option<
            Arc<crate::extensions::web_request::WebRequestInterceptorRegistry>,
        >,
    ) -> WebViewBuilder<'static> {
        let https_upgrade = https_upgrade_enabled;

        let upgrade_tx = event_tx.clone();

        let privacy_script =
            crate::net::privacy::privacy_initialization_script(tracking_protection_enabled);

        let devtools = cfg!(debug_assertions) || devtools;

        WebViewBuilder::new()
            .with_url(url_str)
            .with_devtools(devtools)
            .with_initialization_script(ERROR_MONITOR_JS)
            .with_initialization_script(&privacy_script)
            // Custom protocol for aileron:// internal pages
            .with_custom_protocol("aileron".into(), {
                let open_tx = event_tx.clone();
                move |_webview_id, req| {
                    // Extract the path from the request URI to serve different pages
                    let path = req.uri().path().trim_start_matches('/');
                    let html = match path {
                        "new" => aileron_new_tab_page(),
                        "terminal" => aileron_terminal_page(),
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
                        "reader" => aileron_reader_page(),
                        _ => aileron_404_page(&req.uri().to_string()), // "welcome" and anything else
                    };
                    wry::http::Response::builder()
                        .header("Content-Type", "text/html")
                        .body(html.into_bytes().into())
                        .expect("valid http response builder with known header and body")
                }
            })
            .with_ipc_handler({
                let ipc_tx = event_tx.clone();
                let ipc_pid = pid;
                move |req: wry::http::Request<String>| {
                    let _ = ipc_tx.send(WryEvent::IpcMessage {
                        pane_id: ipc_pid,
                        message: req.into_body(),
                    });
                }
            })
            // Block navigation to ad/tracker URLs and upgrade HTTP to HTTPS
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
                            host_lower == d_lower || host_lower.ends_with(&format!(".{d_lower}"))
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
                // If popup blocker is enabled, block all new window requests.
                // Users can still open links in new tabs via keybindings.
                if popup_blocker {
                    warn!("Popup blocked: {}", _url);
                    wry::NewWindowResponse::Deny
                } else {
                    wry::NewWindowResponse::Allow
                }
            })
            // Track page load events
            .with_on_page_load_handler({
                let tx = event_tx.clone();
                move |event: PageLoadEvent, url: String| {
                    let _ = tx.send(match event {
                        PageLoadEvent::Started => WryEvent::LoadStarted {
                            pane_id: pid,
                            url: url.clone(),
                        },
                        PageLoadEvent::Finished => {
                            // Check for error state: if _aileron_last_error is set,
                            // or if the title looks like a WebKit error page, send an event
                            // that the frame loop can use to show a custom error page.
                            WryEvent::LoadComplete { pane_id: pid, url }
                        }
                    });
                }
            })
            // Track title changes
            .with_document_title_changed_handler({
                let title_tx = event_tx.clone();
                move |title: String| {
                    let _ = title_tx.send(WryEvent::TitleChanged {
                        pane_id: pid,
                        title,
                    });
                }
            })
            // Handle downloads: save to ~/Downloads/
            .with_download_started_handler({
                let dl_tx = event_tx.clone();
                move |url: String, suggested_path: &mut std::path::PathBuf| {
                    if is_pdf_url(&url) {
                        return false;
                    }
                    if let Some(downloads_dir) = directories::UserDirs::new()
                        .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
                    {
                        // Extract filename from the URL or suggested path
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
                        // Notify the UI about the download
                        let _ = dl_tx.send(WryEvent::DownloadStarted {
                            pane_id: pid,
                            url: url.clone(),
                            filename: filename.clone(),
                        });
                    }
                    // Allow the download
                    true
                }
            })
    }

    /// Navigate to a URL.
    pub fn navigate(&mut self, url: &Url) {
        if let Err(e) = self.webview.load_url(url.as_str()) {
            warn!("Failed to navigate to {}: {}", url, e);
        } else {
            self.url = url.clone();
            #[cfg(target_os = "linux")]
            if let Some(ref win) = self.gtk_window {
                win.set_title(&format!("Aileron - {}", url.as_str()));
            }
        }
    }

    /// Get the current URL.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Get the pane ID.
    pub fn pane_id(&self) -> Uuid {
        self.pane_id
    }

    /// Execute JavaScript (fire-and-forget).
    pub fn execute_js(&self, js: &str) {
        if let Err(e) = self.webview.evaluate_script(js) {
            warn!("JS evaluation error: {}", e);
            crate::debug_capturer::capture_js_error(&self.pane_id.to_string(), &format!("{e}"));
        }
    }

    /// Execute JavaScript with a callback that receives the result as JSON string.
    pub fn execute_js_with_callback(&self, js: &str, callback: impl Fn(String) + Send + 'static) {
        if let Err(e) = self.webview.evaluate_script_with_callback(js, callback) {
            warn!("JS evaluation error: {}", e);
            crate::debug_capturer::capture_js_error(&self.pane_id.to_string(), &format!("{e}"));
        }
    }

    /// Update the position and size of this pane.
    pub fn set_bounds(&self, bounds: Rect) {
        if let Err(e) = self.webview.set_bounds(bounds) {
            warn!(
                "Failed to set bounds for pane {}: {}",
                &self.pane_id.to_string()[..8],
                e
            );
        }
        // Also resize the GTK window + Fixed container on Wayland
        #[cfg(target_os = "linux")]
        if let Some(ref win) = self.gtk_window {
            let (w, h) = match bounds.size {
                Size::Logical(s) => (s.width as i32, s.height as i32),
                Size::Physical(s) => (s.width as i32, s.height as i32),
            };
            win.set_default_size(w, h);
            // Resize the Fixed container to match
            if let Some(ref fixed) = self.gtk_fixed {
                fixed.set_size_request(w, h);
            }
        }
    }

    /// Show or hide the webview.
    pub fn set_visible(&self, visible: bool) {
        if let Err(e) = self.webview.set_visible(visible) {
            warn!("Failed to set visibility: {}", e);
        }
        #[cfg(target_os = "linux")]
        if let Some(ref win) = self.gtk_window {
            if visible {
                win.show();
            } else {
                win.hide();
            }
        }
    }

    /// Focus the webview (for keyboard input in Insert mode).
    pub fn focus(&self) {
        if let Err(e) = self.webview.focus() {
            warn!("Failed to focus webview: {}", e);
        }
        #[cfg(target_os = "linux")]
        if let Some(ref win) = self.gtk_window {
            win.present();
        }
    }

    /// Move focus back to the parent window (for Normal/Command mode).
    pub fn focus_parent(&self) {
        if let Err(e) = self.webview.focus_parent() {
            warn!("Failed to focus parent: {}", e);
        }
    }

    /// Get the current title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Poll for pending events from the webview.
    pub fn poll_events(&mut self) -> Vec<WryEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            match &event {
                WryEvent::LoadComplete { url, .. } => {
                    if let Ok(parsed) = Url::parse(url) {
                        self.url = parsed;
                    }
                }
                WryEvent::TitleChanged { title, .. } => {
                    self.title = title.clone();
                    // Update GTK window title on Wayland
                    #[cfg(target_os = "linux")]
                    if let Some(ref win) = self.gtk_window {
                        win.set_title(&format!("Aileron - {title}"));
                    }
                }
                WryEvent::HttpsUpgraded { to, .. } => {
                    if let Ok(https_url) = Url::parse(to) {
                        self.url = https_url;
                        if let Err(e) = self.webview.load_url(to) {
                            warn!(%e, "HTTPS upgrade redirect failed");
                        }
                    }
                }
                _ => {}
            }
            events.push(event);
        }
        events
    }

    /// Navigate back in history (uses JS workaround — wry has no back() API).
    pub fn back(&self) {
        if let Err(e) = self
            .webview
            .evaluate_script("if (window.history.length > 1) window.history.back()")
        {
            warn!(%e, "History back navigation failed");
        }
    }

    /// Navigate forward in history (uses JS workaround — wry has no forward() API).
    pub fn forward(&self) {
        if let Err(e) = self
            .webview
            .evaluate_script("if (window.history.length > 1) window.history.forward()")
        {
            warn!(%e, "History forward navigation failed");
        }
    }

    /// Reload the current page.
    pub fn reload(&self) {
        if let Err(e) = self.webview.reload() {
            warn!("Failed to reload: {}", e);
        }
    }

    /// Get the actual URL from the webview (may differ due to redirects).
    #[must_use]
    pub fn actual_url(&self) -> Option<String> {
        self.webview.url().ok()
    }

    /// Get the embedding mode.
    pub fn embed_mode(&self) -> EmbedMode {
        self.embed_mode
    }

    /// Open the WebKit developer tools inspector for this pane.
    #[cfg(target_os = "linux")]
    pub fn open_devtools(&self) {
        self.webview.open_devtools();
    }
}

impl super::engine::PaneRenderer for WryPane {
    fn navigate(&mut self, url: &Url) {
        WryPane::navigate(self, url);
    }
    fn current_url(&self) -> Option<&Url> {
        Some(WryPane::url(self))
    }
    fn title(&self) -> &str {
        WryPane::title(self)
    }
    fn execute_js(&self, js: &str) {
        WryPane::execute_js(self, js);
    }
    fn reload(&self) {
        WryPane::reload(self);
    }
    fn back(&self) {
        WryPane::back(self);
    }
    fn forward(&self) {
        WryPane::forward(self);
    }
    fn set_bounds(&self, bounds: Rect) {
        WryPane::set_bounds(self, bounds);
    }
    fn set_visible(&self, visible: bool) {
        WryPane::set_visible(self, visible);
    }
    fn focus(&self) {
        WryPane::focus(self);
    }
    fn focus_parent(&self) {
        WryPane::focus_parent(self);
    }
    fn pane_id(&self) -> Uuid {
        WryPane::pane_id(self)
    }
}

/// Manages multiple WryPane instances (one per BSP leaf).
///
/// Not Send/Sync because wry::WebView is !Send + !Sync (GTK thread affinity).
pub struct WryPaneManager {
    panes: HashMap<Uuid, WryPane>,
}

impl WryPaneManager {
    pub fn new() -> Self {
        Self {
            panes: HashMap::new(),
        }
    }

    /// Create a new WryPane. Tries X11 child first, falls back to GTK window.
    #[allow(clippy::too_many_arguments)]
    pub fn create_pane<W>(
        &mut self,
        parent: &W,
        pane_id: Uuid,
        initial_url: Url,
        bounds: Rect,
        blocked_domains: Vec<String>,
        https_safe_list: std::collections::HashSet<String>,
        devtools: bool,
        popup_blocker: bool,
        interceptor_registry: Option<
            Arc<crate::extensions::web_request::WebRequestInterceptorRegistry>,
        >,
    ) -> Result<(), wry::Error>
    where
        W: HasWindowHandle,
    {
        let pane = WryPane::new(
            parent,
            pane_id,
            initial_url,
            bounds,
            blocked_domains,
            https_safe_list,
            devtools,
            popup_blocker,
            interceptor_registry,
        )?;
        self.panes.insert(pane_id, pane);
        Ok(())
    }

    /// Remove a pane (e.g., when a BSP leaf is closed).
    pub fn remove_pane(&mut self, pane_id: &Uuid) {
        if self.panes.remove(pane_id).is_some() {
            info!("Removed WryPane {}", &pane_id.to_string()[..8]);
        }
    }

    /// Get a mutable reference to a pane.
    #[must_use]
    pub fn get_mut(&mut self, pane_id: &Uuid) -> Option<&mut WryPane> {
        self.panes.get_mut(pane_id)
    }

    /// Get an immutable reference to a pane.
    #[must_use]
    pub fn get(&self, pane_id: &Uuid) -> Option<&WryPane> {
        self.panes.get(pane_id)
    }

    /// Check if a pane exists.
    pub fn contains(&self, pane_id: &Uuid) -> bool {
        self.panes.contains_key(pane_id)
    }

    /// Poll all panes for events.
    pub fn poll_all_events(&mut self) -> Vec<WryEvent> {
        let mut events = Vec::with_capacity(self.panes.len());
        for pane in self.panes.values_mut() {
            events.extend(pane.poll_events());
        }
        events
    }

    /// Navigate back in the active pane's history.
    pub fn back(&self, pane_id: &Uuid) {
        if let Some(pane) = self.panes.get(pane_id) {
            pane.back();
        }
    }

    /// Navigate forward in the active pane's history.
    pub fn forward(&self, pane_id: &Uuid) {
        if let Some(pane) = self.panes.get(pane_id) {
            pane.forward();
        }
    }

    /// Reload the active pane.
    pub fn reload(&self, pane_id: &Uuid) {
        if let Some(pane) = self.panes.get(pane_id) {
            pane.reload();
        }
    }

    /// Open devtools for a pane.
    #[cfg(target_os = "linux")]
    pub fn open_devtools(&self, pane_id: &Uuid) {
        if let Some(pane) = self.panes.get(pane_id) {
            pane.open_devtools();
        }
    }

    /// Number of active panes.
    pub fn len(&self) -> usize {
        self.panes.len()
    }

    /// Check if there are no panes.
    pub fn is_empty(&self) -> bool {
        self.panes.is_empty()
    }

    /// Remove all panes (used before workspace restore).
    pub fn remove_all(&mut self) {
        let count = self.panes.len();
        self.panes.clear();
        if count > 0 {
            info!("Removed {} wry pane(s)", count);
        }
    }

    /// Get the current URL for a pane, if it exists.
    #[must_use]
    pub fn url_for(&self, pane_id: &Uuid) -> Option<Url> {
        self.panes.get(pane_id).map(|p| p.url().clone())
    }

    /// Iterate over all pane IDs.
    pub fn pane_ids(&self) -> Vec<Uuid> {
        self.panes.keys().copied().collect()
    }
}

impl Default for WryPaneManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a BSP Rect (f64) to a wry Rect for positioning a child window.
///
/// The top offset uses `status_bar_height` (the status bar sits at the very top
/// of the window). The URL bar is rendered below the status bar by egui, so
/// it is accounted for by reducing the available height — not by shifting the
/// y position. Both `status_bar_height` and `url_bar_height` happen to be 32.0
/// in the current UI, but they represent distinct UI elements.
pub fn bsp_rect_to_wry_rect(
    rect: &crate::wm::Rect,
    status_bar_height: f64,
    url_bar_height: f64,
    sidebar_width: f64,
    sidebar_on_right: bool,
) -> Rect {
    let (x_offset, available_width) = if sidebar_width > 0.0 {
        if sidebar_on_right {
            (0.0, (rect.w - sidebar_width).max(100.0))
        } else {
            (sidebar_width, (rect.w - sidebar_width).max(100.0))
        }
    } else {
        (0.0, rect.w)
    };

    Rect {
        position: Position::Logical(LogicalPosition::new(
            rect.x + x_offset,
            rect.y + status_bar_height,
        )),
        size: Size::Logical(LogicalSize::new(
            available_width,
            (rect.h - status_bar_height - url_bar_height).max(100.0),
        )),
    }
}

/// Initialize GTK (required by wry on Linux).
/// Must be called once before creating any WebView.
pub fn init_gtk() {
    #[cfg(target_os = "linux")]
    {
        gtk::init().expect("Failed to initialize GTK (required by wry on Linux)");
        info!("GTK initialized for wry WebKitGTK backend");
    }
    #[cfg(not(target_os = "linux"))]
    {
        info!("GTK init skipped (not needed on this platform)");
    }
}

/// Pump the GTK event loop (required by wry on Linux).
/// Must be called regularly (e.g., in `about_to_wait`).
pub fn pump_gtk() {
    #[cfg(target_os = "linux")]
    {
        // Capture GLib log messages to diagnose WebKitGTK crashes.
        // WebKitGTK sometimes emits G_LOG_LEVEL_ERROR messages during draw
        // propagation that default to SIGTRAP. We intercept these, log them via
        // tracing, and suppress the fatal signal.
        // SAFETY: FFI call to g_log_set_handler. log_domain is a valid CString pointer.
        // glib_log_handler is a valid function pointer matching GLogFunc signature.
        unsafe {
            let log_domain = std::ffi::CString::new("WebKitGTK").unwrap();
            glib_sys::g_log_set_handler(
                log_domain.as_ptr(),
                glib_sys::G_LOG_LEVEL_MASK | glib_sys::G_LOG_FLAG_RECURSION,
                Some(glib_log_handler),
                std::ptr::null_mut(),
            );
        }

        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }
}

/// Custom GLib log handler that captures WebKitGTK critical/error messages
/// and prevents them from crashing the app via SIGTRAP.
///
/// WebKitGTK sometimes emits G_LOG_LEVEL_ERROR messages during draw propagation
/// when the rendering backend encounters issues. We intercept these, log them via
/// tracing, and suppress the fatal signal by returning without calling the
/// default handler.
/// Custom GLib log handler that captures WebKitGTK critical/error messages
/// and prevents them from crashing the app via SIGTRAP.
///
/// WebKitGTK sometimes emits G_LOG_LEVEL_ERROR messages during draw propagation
/// when the rendering backend encounters issues. We intercept these, log them via
/// tracing, and suppress the fatal signal by returning without calling the
/// default handler.
#[cfg(target_os = "linux")]
// SAFETY: FFI callback registered via g_log_set_handler. Signature matches GLogFunc typedef.
unsafe extern "C" fn glib_log_handler(
    log_domain: *const std::os::raw::c_char,
    log_level: glib_sys::GLogLevelFlags,
    message: *const std::os::raw::c_char,
    _user_data: glib_sys::gpointer,
) {
    use tracing::{error, warn};

    let domain = if log_domain.is_null() {
        "*".to_string()
    } else {
        // SAFETY: Pointer validity guaranteed by null check above. Provided by GLib logging machinery.
        unsafe { std::ffi::CStr::from_ptr(log_domain) }
            .to_string_lossy()
            .into_owned()
    };

    let level_bits = log_level;
    let level = if level_bits & glib_sys::G_LOG_LEVEL_ERROR != 0 {
        "ERROR"
    } else if level_bits & glib_sys::G_LOG_LEVEL_CRITICAL != 0 {
        "CRITICAL"
    } else if level_bits & glib_sys::G_LOG_LEVEL_WARNING != 0 {
        "WARNING"
    } else if level_bits & glib_sys::G_LOG_LEVEL_MESSAGE != 0 {
        "MESSAGE"
    } else if level_bits & glib_sys::G_LOG_LEVEL_INFO != 0 {
        "INFO"
    } else if level_bits & glib_sys::G_LOG_LEVEL_DEBUG != 0 {
        "DEBUG"
    } else {
        "UNKNOWN"
    };

    let msg = if message.is_null() {
        "(null)".to_string()
    } else {
        // SAFETY: Pointer validity guaranteed by null check above. Provided by GLib logging machinery.
        unsafe { std::ffi::CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    };

    match level {
        "ERROR" | "CRITICAL" => {
            error!("[GLib {}::{}] {}", domain, level, msg);
            crate::debug_capturer::capture_glib(level, &domain, &msg);
        }
        "WARNING" if domain.contains("WebKit") || domain.contains("Gtk") => {
            warn!("[GLib {}::{}] {}", domain, level, msg);
            crate::debug_capturer::capture_glib(level, &domain, &msg);
        }
        _ => {}
    }
}

// ─── JS constants, HTML page generators, and helpers are in wry_pages.rs ───

#[cfg(test)]
mod tests {
    use super::*;

    // ─── bsp_rect_to_wry_rect tests ────────────────────────────────

    #[test]
    fn test_basic_transform() {
        let rect = crate::wm::Rect::new(0.0, 0.0, 800.0, 600.0);
        let status_h = 32.0;
        let url_h = 32.0;
        let result = bsp_rect_to_wry_rect(&rect, status_h, url_h, 0.0, false);

        let pos = match result.position {
            Position::Logical(p) => p,
            _ => panic!("Expected Logical position"),
        };
        assert_eq!(pos.x, 0.0);
        assert_eq!(pos.y, 32.0);

        let size = match result.size {
            Size::Logical(s) => s,
            _ => panic!("Expected Logical size"),
        };
        assert_eq!(size.width, 800.0);
        assert_eq!(size.height, 600.0 - 32.0 - 32.0);
    }

    #[test]
    fn test_nonzero_origin() {
        let rect = crate::wm::Rect::new(100.0, 50.0, 400.0, 300.0);
        let result = bsp_rect_to_wry_rect(&rect, 32.0, 32.0, 0.0, false);

        let pos = match result.position {
            Position::Logical(p) => p,
            _ => panic!("Expected Logical position"),
        };
        assert_eq!(pos.x, 100.0);
        assert_eq!(pos.y, 50.0 + 32.0);

        let size = match result.size {
            Size::Logical(s) => s,
            _ => panic!("Expected Logical size"),
        };
        assert_eq!(size.width, 400.0);
        assert_eq!(size.height, 300.0 - 32.0 - 32.0);
    }

    #[test]
    fn test_height_clamped_to_minimum() {
        let rect = crate::wm::Rect::new(0.0, 0.0, 800.0, 50.0);
        let result = bsp_rect_to_wry_rect(&rect, 32.0, 32.0, 0.0, false);

        let size = match result.size {
            Size::Logical(s) => s,
            _ => panic!("Expected Logical size"),
        };
        assert_eq!(
            size.height, 100.0,
            "Height should be clamped to minimum 100px"
        );
    }

    #[test]
    fn test_zero_bar_heights() {
        let rect = crate::wm::Rect::new(10.0, 20.0, 500.0, 400.0);
        let result = bsp_rect_to_wry_rect(&rect, 0.0, 0.0, 0.0, false);

        let pos = match result.position {
            Position::Logical(p) => p,
            _ => panic!("Expected Logical position"),
        };
        assert_eq!(pos.x, 10.0);
        assert_eq!(pos.y, 20.0);

        let size = match result.size {
            Size::Logical(s) => s,
            _ => panic!("Expected Logical size"),
        };
        assert_eq!(size.width, 500.0);
        assert_eq!(size.height, 400.0);
    }

    #[test]
    fn test_large_bar_heights() {
        let rect = crate::wm::Rect::new(0.0, 0.0, 800.0, 100.0);
        let result = bsp_rect_to_wry_rect(&rect, 60.0, 60.0, 0.0, false);

        let size = match result.size {
            Size::Logical(s) => s,
            _ => panic!("Expected Logical size"),
        };
        assert_eq!(size.height, 100.0);
    }

    #[test]
    fn test_sidebar_left_offset() {
        let rect = crate::wm::Rect::new(0.0, 0.0, 800.0, 600.0);
        let result = bsp_rect_to_wry_rect(&rect, 32.0, 32.0, 180.0, false);

        let pos = match result.position {
            Position::Logical(p) => p,
            _ => panic!("Expected Logical position"),
        };
        assert_eq!(pos.x, 180.0, "X should be offset by sidebar width");
        assert_eq!(pos.y, 32.0);

        let size = match result.size {
            Size::Logical(s) => s,
            _ => panic!("Expected Logical size"),
        };
        assert_eq!(size.width, 620.0, "Width should be reduced by sidebar");
        assert_eq!(size.height, 536.0);
    }

    #[test]
    fn test_sidebar_right_offset() {
        let rect = crate::wm::Rect::new(0.0, 0.0, 800.0, 600.0);
        let result = bsp_rect_to_wry_rect(&rect, 32.0, 32.0, 180.0, true);

        let pos = match result.position {
            Position::Logical(p) => p,
            _ => panic!("Expected Logical position"),
        };
        assert_eq!(
            pos.x, 0.0,
            "X should not be offset when sidebar is on right"
        );
        assert_eq!(pos.y, 32.0);

        let size = match result.size {
            Size::Logical(s) => s,
            _ => panic!("Expected Logical size"),
        };
        assert_eq!(size.width, 620.0, "Width should be reduced by sidebar");
    }
}
