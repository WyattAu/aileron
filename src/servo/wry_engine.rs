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
use gtk::prelude::{ContainerExt, GtkWindowExt, WidgetExt};
#[cfg(target_os = "linux")]
use glib_sys;
use std::collections::HashMap;
use std::sync::mpsc;
use tracing::{info, warn};
use url::Url;
use uuid::Uuid;
use wry::dpi::{LogicalPosition, LogicalSize, Position, Size};
use wry::raw_window_handle::HasWindowHandle;
#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;
use wry::{PageLoadEvent, Rect, WebView, WebViewBuilder};

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
    DownloadStarted { pane_id: Uuid, url: String, filename: String },
    /// Request to open a file (from file browser).
    OpenFile { path: String },
    /// HTTP URL was upgraded to HTTPS.
    HttpsUpgraded { pane_id: Uuid, from: String, to: String },
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
    pub fn new<W>(
        parent: &W,
        pane_id: Uuid,
        initial_url: Url,
        bounds: Rect,
        blocked_domains: Vec<String>,
        devtools: bool,
        popup_blocker: bool,
    ) -> Result<Self, wry::Error>
    where
        W: HasWindowHandle,
    {
        let pid = pane_id;
        let url_str = initial_url.as_str().to_string();
        let (event_tx, event_rx) = mpsc::channel();

        // === Path 1: Try build_as_child (X11) ===
        // Builder is built inline so event_tx isn't lost if this path fails.
        match Self::make_builder(&url_str, pid, event_tx.clone(), blocked_domains.clone(), devtools, popup_blocker)
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
            Self::create_gtk_pane(pid, initial_url, bounds, event_tx, event_rx, devtools, popup_blocker)
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(wry::Error::Message(
                "Failed to create webview: window handle kind not supported on this platform"
                    .into(),
            ))
        }
    }

    /// Create a standalone GTK window with embedded wry webview (Wayland-compatible).
    #[cfg(target_os = "linux")]
    fn create_gtk_pane(
        pane_id: Uuid,
        initial_url: Url,
        bounds: Rect,
        event_tx: mpsc::Sender<WryEvent>,
        event_rx: mpsc::Receiver<WryEvent>,
        devtools: bool,
        popup_blocker: bool,
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
        let builder = Self::make_builder(&url_str, pane_id, event_tx, Vec::new(), devtools, popup_blocker);

        let webview = builder.build_gtk(&fixed)?;

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

    /// Build a WebViewBuilder with common configuration.
    /// The event_tx is moved into the builder's closures.
    fn make_builder(
        url_str: &str,
        pid: Uuid,
        event_tx: mpsc::Sender<WryEvent>,
        blocked_domains: Vec<String>,
        devtools: bool,
        popup_blocker: bool,
    ) -> WebViewBuilder<'static> {
        Self::make_builder_with_privacy(url_str, pid, event_tx, blocked_domains, true, true, devtools, popup_blocker)
    }

    /// Build a WebViewBuilder with common configuration and privacy settings.
    /// The event_tx is moved into the builder's closures.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn make_builder_with_privacy(
        url_str: &str,
        pid: Uuid,
        event_tx: mpsc::Sender<WryEvent>,
        blocked_domains: Vec<String>,
        https_upgrade_enabled: bool,
        tracking_protection_enabled: bool,
        devtools: bool,
        popup_blocker: bool,
    ) -> WebViewBuilder<'static> {
        let https_safe_list = if https_upgrade_enabled {
            crate::net::privacy::load_https_safe_list()
        } else {
            std::collections::HashSet::new()
        };
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
                        _ => aileron_404_page(&req.uri().to_string()), // "welcome" and anything else
                    };
                    wry::http::Response::builder()
                        .header("Content-Type", "text/html")
                        .body(html.into_bytes().into())
                        .unwrap()
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
        }
    }

    /// Execute JavaScript with a callback that receives the result as JSON string.
    pub fn execute_js_with_callback(&self, js: &str, callback: impl Fn(String) + Send + 'static) {
        if let Err(e) = self.webview.evaluate_script_with_callback(js, callback) {
            warn!("JS evaluation error: {}", e);
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
                        win.set_title(&format!("Aileron - {}", title));
                    }
                }
                WryEvent::HttpsUpgraded { to, .. } => {
                    if let Ok(https_url) = Url::parse(to) {
                        self.url = https_url;
                        let _ = self.webview.load_url(to);
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
        let _ = self
            .webview
            .evaluate_script("if (window.history.length > 1) window.history.back()");
    }

    /// Navigate forward in history (uses JS workaround — wry has no forward() API).
    pub fn forward(&self) {
        let _ = self
            .webview
            .evaluate_script("if (window.history.length > 1) window.history.forward()");
    }

    /// Reload the current page.
    pub fn reload(&self) {
        if let Err(e) = self.webview.reload() {
            warn!("Failed to reload: {}", e);
        }
    }

    /// Get the actual URL from the webview (may differ due to redirects).
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
        devtools: bool,
        popup_blocker: bool,
    ) -> Result<(), wry::Error>
    where
        W: HasWindowHandle,
    {
        let pane = WryPane::new(parent, pane_id, initial_url, bounds, blocked_domains, devtools, popup_blocker)?;
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
    pub fn get_mut(&mut self, pane_id: &Uuid) -> Option<&mut WryPane> {
        self.panes.get_mut(pane_id)
    }

    /// Get an immutable reference to a pane.
    pub fn get(&self, pane_id: &Uuid) -> Option<&WryPane> {
        self.panes.get(pane_id)
    }

    /// Check if a pane exists.
    pub fn contains(&self, pane_id: &Uuid) -> bool {
        self.panes.contains_key(pane_id)
    }

    /// Poll all panes for events.
    pub fn poll_all_events(&mut self) -> Vec<WryEvent> {
        let mut events = Vec::new();
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
        unsafe { std::ffi::CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    };

    match level {
        "ERROR" | "CRITICAL" => {
            error!("[GLib {}::{}] {}", domain, level, msg);
            // Do NOT call the default handler — that's what causes SIGTRAP.
            // By returning, we suppress the fatal signal.
        }
        "WARNING" => {
            if domain.contains("WebKit") || domain.contains("Gtk") {
                warn!("[GLib {}::{}] {}", domain, level, msg);
            }
        }
        _ => {}
    }
}

/// JavaScript that intercepts network requests for monitoring.
/// Stores captured requests in window._aileron_requests[].
pub const NETWORK_MONITOR_JS: &str = r#"
(function() {
    if (window._aileron_network_monitor) return;
    window._aileron_network_monitor = true;
    window._aileron_requests = [];
    
    var origFetch = window.fetch;
    window.fetch = function() {
        var url = arguments[0] instanceof Request ? arguments[0].url : String(arguments[0]);
        var method = arguments[1] && arguments[1].method ? arguments[1].method : 'GET';
        var entry = { url: url, method: method, type: 'fetch', time: new Date().toISOString(), status: null };
        window._aileron_requests.push(entry);
        return origFetch.apply(this, arguments).then(function(resp) {
            entry.status = resp.status;
            return resp;
        }).catch(function(err) {
            entry.status = 'ERR';
            throw err;
        });
    };
    
    var origOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function(method, url) {
        this._aileron_entry = { url: String(url), method: method, type: 'xhr', time: new Date().toISOString(), status: null };
        window._aileron_requests.push(this._aileron_entry);
        return origOpen.apply(this, arguments);
    };
    var origSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.send = function() {
        var self = this;
        this.addEventListener('load', function() {
            if (self._aileron_entry) self._aileron_entry.status = self.status;
        });
        this.addEventListener('error', function() {
            if (self._aileron_entry) self._aileron_entry.status = 'ERR';
        });
        return origSend.apply(this, arguments);
    };
})();
"#;

pub const NETWORK_LOG_JS: &str = r#"
JSON.stringify(window._aileron_requests || [])
"#;

pub const NETWORK_CLEAR_JS: &str = r#"
window._aileron_requests = [];
'Network log cleared';
"#;

pub const CONSOLE_CAPTURE_JS: &str = r#"
(function() {
    if (window._aileron_console_capture) return;
    window._aileron_console_capture = true;
    window._aileron_console = [];
    
    ['log', 'warn', 'error', 'info'].forEach(function(level) {
        var orig = console[level];
        console[level] = function() {
            var msg = Array.prototype.slice.call(arguments).map(function(a) {
                try { return typeof a === 'object' ? JSON.stringify(a).slice(0, 200) : String(a); }
                catch(e) { return String(a); }
            }).join(' ');
            window._aileron_console.push({ level: level, msg: msg, time: new Date().toISOString() });
            if (window._aileron_console.length > 200) window._aileron_console.shift();
            return orig.apply(console, arguments);
        };
    });
})();
"#;

pub const CONSOLE_LOG_JS: &str = r#"
JSON.stringify(window._aileron_console || [])
"#;

/// JavaScript that monitors for navigation errors and stores them
/// for detection after page load completes.
pub const ERROR_MONITOR_JS: &str = r#"
(function() {
    if (window._aileron_error_monitor) return;
    window._aileron_error_monitor = true;

    // Send a navigation error report to Aileron via IPC.
    // Uses window.location.href for the URL since it's the failed destination.
    function reportNavError(message) {
        try {
            window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke('__aileron_ipc', {
                message: '__aileron_nav_error__|' + (window.location.href || '') + '|' + message
            });
        } catch(e) {
            // Fallback: wry IPC via postMessage to aileron:// scheme
            try { window.postMessage('__aileron_nav_error__|' + (window.location.href || '') + '|' + message, '*'); } catch(e2) {}
        }
    }

    // Check on load complete whether the page looks like a WebKitGTK error page.
    // WebKitGTK error pages have titles like "Problem loading page" or contain
    // short error messages in the body with no real content.
    function checkForErrorPage() {
        var title = (document.title || '').toLowerCase();
        var isLikelyError = false;
        var errorMsg = '';

        // WebKitGTK error page indicators
        if (title.indexOf('problem loading') !== -1
            || title.indexOf('unable to connect') !== -1
            || title.indexOf('could not connect') !== -1
            || title.indexOf('network error') !== -1
            || title.indexOf('connection refused') !== -1
            || title.indexOf('ssl') !== -1 && title.indexOf('error') !== -1
            || title.indexOf('certificate') !== -1
            || title.indexOf('not found') !== -1
            || title.indexOf('server not found') !== -1
            || title.indexOf('host not found') !== -1
            || title.indexOf('timed out') !== -1
            || title.indexOf('unauthorized') !== -1
            || title.indexOf('forbidden') !== -1) {
            isLikelyError = true;
            errorMsg = document.title || title;
        }

        // Also check for very short pages with error-like content
        if (!isLikelyError) {
            var body = document.body ? document.body.innerText : '';
            if (body.length < 300 && body.length > 0) {
                var bodyLower = body.toLowerCase();
                if (bodyLower.indexOf('error') !== -1
                    || bodyLower.indexOf('could not') !== -1
                    || bodyLower.indexOf('failed to') !== -1
                    || bodyLower.indexOf('unable to') !== -1) {
                    isLikelyError = true;
                    errorMsg = body.substring(0, 200).trim();
                }
            }
        }

        // Also detect blank/empty pages that aren't our own pages
        if (!isLikelyError && document.body) {
            var html = document.body.innerHTML.trim();
            if (html.length === 0 && window.location.protocol !== 'aileron:') {
                isLikelyError = true;
                errorMsg = 'Empty page — possible DNS or connection failure';
            }
        }

        if (isLikelyError) {
            reportNavError(errorMsg);
        }
    }

    // Run check after DOM is ready and also after full load
    if (document.readyState === 'complete' || document.readyState === 'interactive') {
        setTimeout(checkForErrorPage, 100);
    }
    window.addEventListener('load', function() {
        setTimeout(checkForErrorPage, 100);
    });

    // Also monitor for runtime errors during page lifecycle
    window.addEventListener('error', function(e) {
        // Only report if it looks like a network/resource error, not a JS bug
        var target = e.target || {};
        if (target.tagName === 'IMG' || target.tagName === 'LINK' || target.tagName === 'SCRIPT') {
            // Resource loading failure — don't report individual resource errors
            return;
        }
        var msg = (e.message || 'Unknown error').toString();
        if (msg.indexOf('net::') !== -1
            || msg.indexOf('ERR_') !== -1
            || msg.indexOf('NetworkError') !== -1
            || msg.indexOf('Failed to fetch') !== -1) {
            reportNavError(msg);
        }
    }, true);
})();
"#;

pub const CONSOLE_CLEAR_JS: &str = r#"
window._aileron_console = [];
'Console cleared';
"#;

/// JavaScript that saves the current scroll position before navigation.
/// Called before back/forward navigation.
pub const SCROLL_SAVE_JS: &str = r#"
(function() {
    window._aileron_scroll_pos = {
        x: window.scrollX || document.documentElement.scrollLeft || 0,
        y: window.scrollY || document.documentElement.scrollTop || 0
    };
})();
"#;

/// JavaScript that restores the saved scroll position after navigation.
pub const SCROLL_RESTORE_JS: &str = r#"
(function() {
    if (window._aileron_scroll_pos) {
        window.scrollTo(window._aileron_scroll_pos.x, window._aileron_scroll_pos.y);
    }
})();
"#;

// ─── Internal page HTML generators ───────────────────────────────────

/// Welcome page shown at `aileron://welcome` (default homepage).
pub(crate) fn aileron_welcome_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<title>Aileron</title>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #141414; color: #aaa; font-family: 'SF Mono', 'Fira Code', monospace;
         display: flex; align-items: center; justify-content: center; height: 100vh; }
  .container { text-align: center; max-width: 680px; padding: 2em; }
  h1 { color: #4db4ff; font-size: 2.5em; margin-bottom: 0.3em; letter-spacing: 0.05em; }
  .subtitle { color: #888; font-size: 1.1em; margin-bottom: 1.5em; }
  .section-title { color: #4db4ff; font-size: 0.85em; text-transform: uppercase;
                   letter-spacing: 0.1em; margin: 1em 0 0.4em; border-bottom: 1px solid #2a2a2a;
                   padding-bottom: 0.3em; }
  .keys { text-align: left; display: inline-block; background: #1a1a1a;
          border-radius: 8px; padding: 1.2em 1.8em; border: 1px solid #2a2a2a; }
  .key-row { display: flex; justify-content: space-between; padding: 0.25em 0; }
  .key-row kbd { color: #4db4ff; background: #222; padding: 2px 8px; border-radius: 3px;
                 font-family: inherit; font-size: 0.9em; border: 1px solid #333; }
  .key-row span { color: #888; }
  .footer { margin-top: 1.5em; color: #444; font-size: 0.85em; }
</style>
</head>
<body>
<div class="container" role="main">
  <h1>Aileron</h1>
  <p class="subtitle">Keyboard-Driven Web Environment</p>
  <div class="keys" role="list" aria-label="Keyboard shortcuts">
    <div class="section-title">Navigation</div>
    <div class="key-row"><span>Scroll down / up</span><kbd>j</kbd> / <kbd>k</kbd></div>
    <div class="key-row"><span>Scroll left / right</span><kbd>h</kbd> / <kbd>l</kbd></div>
    <div class="key-row"><span>Half page down / up</span><kbd>Ctrl+D</kbd> / <kbd>Ctrl+U</kbd></div>
    <div class="key-row"><span>Top of page</span><kbd>Ctrl+G</kbd></div>
    <div class="key-row"><span>Bottom of page</span><kbd>G</kbd></div>
    <div class="key-row"><span>Back / Forward</span><kbd>H</kbd> / <kbd>L</kbd></div>
    <div class="key-row"><span>Reload</span><kbd>r</kbd></div>

    <div class="section-title">Modes</div>
    <div class="key-row"><span>Enter Insert mode</span><kbd>i</kbd></div>
    <div class="key-row"><span>Return to Normal mode</span><kbd>Esc</kbd></div>
    <div class="key-row"><span>Command palette</span><kbd>:</kbd> / <kbd>Ctrl+P</kbd></div>
    <div class="key-row"><span>Open terminal</span><kbd>`</kbd></div>

    <div class="section-title">Tiling</div>
    <div class="key-row"><span>Split vertical</span><kbd>Ctrl+W</kbd></div>
    <div class="key-row"><span>Split horizontal</span><kbd>Ctrl+S</kbd></div>
    <div class="key-row"><span>Switch panes</span><kbd>Ctrl+H</kbd> / <kbd>J</kbd> / <kbd>K</kbd> / <kbd>L</kbd></div>
    <div class="key-row"><span>Resize panes</span><kbd>Ctrl+Alt+H</kbd> / <kbd>J</kbd> / <kbd>K</kbd> / <kbd>L</kbd></div>
    <div class="key-row"><span>Close pane</span><kbd>q</kbd></div>
    <div class="key-row"><span>New tab</span><kbd>Ctrl+T</kbd></div>
    <div class="key-row"><span>New window</span><kbd>Ctrl+N</kbd></div>

    <div class="section-title">Tools</div>
    <div class="key-row"><span>DevTools</span><kbd>F12</kbd></div>
    <div class="key-row"><span>Find in page</span><kbd>Ctrl+F</kbd></div>
    <div class="key-row"><span>Link hints</span><kbd>f</kbd></div>
    <div class="key-row"><span>Copy URL</span><kbd>y</kbd></div>
    <div class="key-row"><span>Reload</span><kbd>r</kbd></div>
    <div class="key-row"><span>Bookmark</span><kbd>Ctrl+B</kbd></div>
    <div class="key-row"><span>External browser</span><kbd>Ctrl+E</kbd></div>
    <div class="key-row"><span>Zoom in / out / reset</span><kbd>Ctrl+=</kbd> / <kbd>-</kbd> / <kbd>0</kbd></div>
    <div class="key-row"><span>Reader mode</span><kbd>Ctrl+Shift+R</kbd></div>
    <div class="key-row"><span>Minimal mode</span><kbd>Ctrl+Shift+M</kbd></div>
    <div class="key-row"><span>Network log</span><kbd>Ctrl+Shift+N</kbd></div>
    <div class="key-row"><span>Console log</span><kbd>Ctrl+Shift+J</kbd></div>
    <div class="key-row"><span>Detach pane to window</span><kbd>Ctrl+Shift+D</kbd></div>
    <div class="key-row"><span>Password manager</span><kbd>bw-unlock</kbd> / <kbd>bw-search</kbd></div>
    <div class="key-row"><span>Quickmark</span><kbd>:m</kbd><kbd>a</kbd> <kbd>url</kbd> / <kbd>:g</kbd><kbd>a</kbd></div>

    <div class="section-title">Commands (:palette)</div>
    <div class="key-row"><span>Shell command</span><kbd>:!</kbd> <kbd>cmd</kbd></div>
    <div class="key-row"><span>Print page</span><kbd>:print</kbd></div>
    <div class="key-row"><span>Mute / unmute</span><kbd>:mute</kbd> / <kbd>:unmute</kbd></div>
    <div class="key-row"><span>Theme</span><kbd>:theme</kbd> <kbd>name</kbd></div>
    <div class="key-row"><span>Site settings</span><kbd>:site-settings</kbd></div>
    <div class="key-row"><span>PDF export</span><kbd>:pdf</kbd></div>
    <div class="key-row"><span>Popups</span><kbd>:popups</kbd></div>
    <div class="key-row"><span>Cookies</span><kbd>:cookies</kbd></div>
    <div class="key-row"><span>File browser</span><kbd>files</kbd> in palette</div>
    <div class="key-row"><span>SSH connect</span><kbd>ssh</kbd> <kbd>host</kbd></div>

    <div class="section-title">Terminal</div>
    <div class="key-row"><span>Position cursor</span>click</div>
    <div class="key-row"><span>Select text</span>drag</div>
    <div class="key-row"><span>Clear selection</span>right-click</div>
    <div class="key-row"><span>Paste</span>middle-click</div>
  </div>
  <p class="footer">Type a URL in the command palette and press Enter to navigate. Use <kbd style="color:#4db4ff;background:#222;padding:2px 8px;border-radius:3px;border:1px solid #333">`</kbd> for a terminal, <kbd style="color:#4db4ff;background:#222;padding:2px 8px;border-radius:3px;border:1px solid #333">files</kbd> to browse, or <kbd style="color:#4db4ff;background:#222;padding:2px 8px;border-radius:3px;border:1px solid #333">ssh user@host</kbd> to connect remotely</p>
</div>
<div aria-live="polite" id="status-region"></div>
</body>
</html>"#.to_string()
}

/// New tab page shown at `aileron://new`.
pub(crate) fn aileron_new_tab_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<title>New Tab</title>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #141414; color: #e0e0e0; font-family: 'SF Mono', 'Fira Code', monospace;
         display: flex; flex-direction: column; align-items: center; padding-top: 8vh; }
  h1 { color: #4db4ff; font-size: 1.8em; margin-bottom: 0.8em; letter-spacing: 0.05em; }
  .search-box { display: flex; margin-bottom: 1.5em; }
  .search-box input {
    background: #1a1a1a; border: 1px solid #333; color: #e0e0e0; padding: 10px 16px;
    font-size: 14px; font-family: inherit; width: 400px; border-radius: 4px; outline: none;
  }
  .search-box input:focus { border-color: #4db4ff; }
  .section { max-width: 540px; width: 100%; margin-bottom: 1.5em; }
  .section-title { color: #666; font-size: 11px; text-transform: uppercase; letter-spacing: 0.1em;
                    margin-bottom: 8px; padding-left: 2px; }
  .links { display: flex; flex-wrap: wrap; gap: 8px; }
  .link {
    background: #1a1a1a; border: 1px solid #2a2a2a; border-radius: 6px; padding: 10px 14px;
    text-align: center; cursor: pointer; text-decoration: none; color: #e0e0e0;
    transition: border-color 0.15s; max-width: 120px; min-width: 80px;
  }
  .link:hover { border-color: #4db4ff; }
  .link:focus { outline: 2px solid #4db4ff; outline-offset: 2px; }
  .link .name { font-size: 11px; margin-top: 4px; color: #888; overflow: hidden;
                text-overflow: ellipsis; white-space: nowrap; }
  .link .icon { font-size: 16px; }
  .history-item {
    display: block; padding: 6px 10px; color: #aaa; text-decoration: none;
    border-radius: 4px; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .history-item:hover { background: #1a1a1a; color: #e0e0e0; }
  .history-item .htitle { color: #ccc; }
  .history-item .hurl { color: #555; font-size: 10px; margin-left: 8px; }
  .hint { color: #444; font-size: 11px; margin-top: 1em; }
  .hint kbd { background: #1a1a1a; border: 1px solid #333; border-radius: 3px; padding: 1px 5px;
              font-family: inherit; font-size: 10px; color: #888; }
</style>
</head>
<body role="main">
<h1>Aileron</h1>
<div class="search-box">
  <label for="search" class="sr-only">Search</label>
  <input type="text" id="search" placeholder="Search or enter URL..." autofocus aria-label="Search or enter URL">
</div>
<div class="section" id="bookmarks-section" style="display:none">
  <div class="section-title">Bookmarks</div>
  <nav class="links" id="bookmarks-list" aria-label="Bookmarks"></nav>
</div>
<div class="section" id="shortcuts-section">
  <nav class="links" aria-label="Quick links">
    <a class="link" href="aileron://files" tabindex="0" aria-label="Files">
      <div class="icon">&#128193;</div>
      <div class="name">Files</div>
    </a>
    <a class="link" href="aileron://terminal" tabindex="0" aria-label="Terminal">
      <div class="icon">&#9000;</div>
      <div class="name">Terminal</div>
    </a>
    <a class="link" href="aileron://bookmarks" tabindex="0" aria-label="Bookmarks">
      <div class="icon">&#9733;</div>
      <div class="name">Bookmarks</div>
    </a>
    <a class="link" href="aileron://history" tabindex="0" aria-label="History">
      <div class="icon">&#128336;</div>
      <div class="name">History</div>
    </a>
  </nav>
</div>
<div class="section" id="history-section" style="display:none">
  <div class="section-title">Recent</div>
  <div id="history-list" aria-label="Recent history"></div>
</div>
<p class="hint"><kbd>Ctrl+P</kbd> commands &middot; <kbd>gt</kbd> switch tabs &middot; <kbd>gi</kbd> insert mode</p>
<script>
// Request bookmark/history data from Aileron via IPC
try {
    if (window.ipc) {
        window.ipc.postMessage(JSON.stringify({ t: 'get-newtab-data' }));
    }
} catch(e) {}

// Callback to populate data when Aileron responds
window._onNewTabData = function(data) {
    // Bookmarks
    if (data.bookmarks && data.bookmarks.length > 0) {
        var el = document.getElementById('bookmarks-section');
        el.style.display = 'block';
        var list = document.getElementById('bookmarks-list');
        data.bookmarks.forEach(function(b) {
            var a = document.createElement('a');
            a.className = 'link';
            a.href = b.url;
            a.title = b.title || b.url;
            a.tabIndex = 0;
            var initial = (b.title || b.url || '?')[0].toUpperCase();
            a.innerHTML = '<div class="icon">' + initial + '</div><div class="name">' +
                (b.title || b.url).substring(0, 16) + '</div>';
            list.appendChild(a);
        });
    }
    // History
    if (data.history && data.history.length > 0) {
        var el = document.getElementById('history-section');
        el.style.display = 'block';
        var list = document.getElementById('history-list');
        data.history.forEach(function(h) {
            var a = document.createElement('a');
            a.className = 'history-item';
            a.href = h.url;
            a.title = h.title + ' — ' + h.url;
            var host = '';
            try { host = new URL(h.url).hostname; } catch(e) {}
            a.innerHTML = '<span class="htitle">' + (h.title || h.url) + '</span>' +
                '<span class="hurl">' + host + '</span>';
            list.appendChild(a);
        });
    }
};

// Search / URL navigation
document.getElementById('search').addEventListener('keydown', function(e) {
  if (e.key === 'Enter') {
    var q = this.value.trim();
    if (!q) return;
    if (q.indexOf('://') !== -1 || (q.indexOf('.') !== -1 && q.indexOf(' ') === -1)) {
      window.location.href = q.indexOf('://') !== -1 ? q : 'https://' + q;
    } else {
      window.location.href = 'https://duckduckgo.com/?q=' + encodeURIComponent(q);
    }
  }
});
</script>
</body>
</html>"#.to_string()
}

pub fn percent_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'/' || b == b'-' || b == b'_' || b == b'.' || b == b'~'
        {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub(crate) fn percent_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(
                &String::from_utf8_lossy(&bytes[i + 1..i + 3]),
                16,
            )
        {
            result.push(byte);
            i += 3;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(result).unwrap_or_else(|_| s.to_string())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    match bytes {
        0..1024 => format!("{} B", bytes),
        n if n < MB => format!("{:.1} KB", n as f64 / KB as f64),
        n if n < GB => format!("{:.1} MB", n as f64 / MB as f64),
        n => format!("{:.1} GB", n as f64 / GB as f64),
    }
}

pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn format_modified_time(meta: &std::fs::Metadata) -> String {
    match meta.modified() {
        Ok(time) => {
            let datetime: chrono::DateTime<chrono::Local> = time.into();
            datetime.format("%Y-%m-%d %H:%M").to_string()
        }
        Err(_) => "-".to_string(),
    }
}

fn file_browser_error_page(path: &str, error: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Files: Error</title>
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: #141414; color: #e0e0e0; font-family: 'SF Mono', 'Fira Code', monospace; padding: 16px; }}
  .error {{ color: #ff6b6b; padding: 20px; }}
  .path {{ color: #888; margin-bottom: 12px; font-size: 14px; }}
  .breadcrumb a {{ color: #4db4ff; text-decoration: none; }}
  .breadcrumb a:hover {{ text-decoration: underline; }}
  a {{ color: #4db4ff; text-decoration: none; }}
</style>
</head>
<body>
<div class="path">{}</div>
<div class="error">Error: {}</div>
<p style="margin-top:16px"><a href="aileron://files">Go to home directory</a></p>
</body>
</html>"#,
        html_escape(path),
        html_escape(error)
    )
}

pub(crate) fn file_browser_page(uri: &wry::http::Uri) -> String {
    use std::path::Path;

    let dir_path = uri
        .query()
        .and_then(|q| {
            q.split('&')
                .find(|pair| pair.starts_with("path="))
                .map(|pair| percent_decode(&pair[5..]))
        })
        .unwrap_or_else(|| {
            directories::UserDirs::new()
                .map(|d| d.home_dir().to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("/"))
                .to_string_lossy()
                .to_string()
        });

    let path = Path::new(&dir_path);

    let entries = match std::fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect::<Vec<_>>(),
        Err(e) => return file_browser_error_page(&dir_path, &e.to_string()),
    };

    let mut dirs: Vec<(String, &std::fs::DirEntry)> = Vec::new();
    let mut files: Vec<(String, &std::fs::DirEntry)> = Vec::new();

    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if entry.path().is_dir() {
            dirs.push((name, entry));
        } else {
            files.push((name, entry));
        }
    }

    dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    let mut breadcrumb_parts = Vec::new();
    if dir_path == "/" {
        breadcrumb_parts.push(
            "<a href=\"aileron://files?path=%2F\">/</a>".to_string()
        );
    } else {
        breadcrumb_parts.push(
            "<a href=\"aileron://files?path=%2F\">/</a><span class=\"sep\">/</span>".to_string()
        );
        let segments: Vec<&str> = dir_path.trim_start_matches('/').split('/').collect();
        let mut accumulated = String::new();
        for (i, seg) in segments.iter().enumerate() {
            accumulated.push_str(seg);
            let encoded = percent_encode_path(&format!("/{}", accumulated));
            if i < segments.len() - 1 {
                breadcrumb_parts.push(format!(
                    "<a href=\"aileron://files?path={}\">{}</a><span class=\"sep\">/</span>",
                    encoded,
                    html_escape(seg)
                ));
            } else {
                breadcrumb_parts.push(format!(
                    "<span>{}</span>",
                    html_escape(seg)
                ));
            }
            accumulated.push('/');
        }
    }
    let breadcrumb_html = breadcrumb_parts.join("");

    let parent_url = if dir_path == "/" {
        String::new()
    } else {
        let parent = path.parent().unwrap_or(Path::new("/"));
        let parent_str = parent.to_string_lossy().to_string();
        if parent_str.is_empty() {
            "aileron://files?path=%2F".to_string()
        } else {
            format!(
                "aileron://files?path={}",
                percent_encode_path(&parent_str)
            )
        }
    };

    let mut rows_html = String::new();
    let mut index: usize = 0;

    if !parent_url.is_empty() {
        rows_html.push_str(&format!(
            "<tr data-index=\"{}\"><td class=\"dir\"><a href=\"{}\" data-parent>..</a></td><td class=\"size\">-</td><td class=\"modified\">-</td></tr>\n",
            index, parent_url
        ));
        index += 1;
    }

    for (name, entry) in &dirs {
        let full_path = entry.path();
        let encoded = percent_encode_path(&full_path.to_string_lossy());
        let meta = entry.metadata().ok();
        let modified = meta.as_ref().map_or("-".to_string(), format_modified_time);
        rows_html.push_str(&format!(
            "<tr data-index=\"{}\"><td class=\"dir\"><a href=\"aileron://files?path={}\">{}/</a></td><td class=\"size\">-</td><td class=\"modified\">{}</td></tr>\n",
            index, encoded, html_escape(name), modified
        ));
        index += 1;
    }

    for (name, entry) in &files {
        let full_path = entry.path();
        let encoded = percent_encode_path(&full_path.to_string_lossy());
        let meta = entry.metadata().ok();
        let size = meta.as_ref().map_or("-".to_string(), |m| format_size(m.len()));
        let modified = meta.as_ref().map_or("-".to_string(), format_modified_time);
        rows_html.push_str(&format!(
            "<tr data-index=\"{}\"><td class=\"file\"><a href=\"aileron://open?path={}\">{}</a></td><td class=\"size\">{}</td><td class=\"modified\">{}</td></tr>\n",
            index, encoded, html_escape(name), size, modified
        ));
        index += 1;
    }

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Files: {}</title>
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: #141414; color: #e0e0e0; font-family: 'SF Mono', 'Fira Code', monospace; padding: 16px; }}
  .breadcrumb {{ color: #888; margin-bottom: 12px; font-size: 14px; }}
  .breadcrumb a {{ color: #4db4ff; text-decoration: none; }}
  .breadcrumb a:hover {{ text-decoration: underline; }}
  .breadcrumb .sep {{ color: #555; margin: 0 4px; }}
  table {{ width: 100%; border-collapse: collapse; }}
  th {{ text-align: left; color: #888; font-weight: normal; padding: 4px 8px; border-bottom: 1px solid #333; font-size: 12px; }}
  td {{ padding: 4px 8px; font-size: 13px; }}
  tr {{ cursor: pointer; }}
  tr:hover {{ background: #1e1e2e; }}
  tr.selected {{ background: #264f78; }}
  a {{ color: inherit; text-decoration: none; }}
  .dir {{ color: #74c0fc; }}
  .file {{ color: #e0e0e0; }}
  .size {{ color: #888; text-align: right; width: 100px; }}
  .modified {{ color: #888; width: 180px; }}
  .error {{ color: #ff6b6b; padding: 20px; }}
</style>
</head>
<body>
<div class="breadcrumb">{}</div>
<table><thead><tr><th>Name</th><th class="size">Size</th><th class="modified">Modified</th></tr></thead>
<tbody>
{}
</tbody></table>
<script>
(function() {{
  var selected = 0;
  var rows = document.querySelectorAll('tbody tr[data-index]');
  function updateSelection() {{
    rows.forEach(function(r) {{ r.classList.remove('selected'); }});
    if (rows[selected]) {{
      rows[selected].classList.add('selected');
      rows[selected].scrollIntoView({{ block: 'nearest' }});
    }}
  }}
  document.addEventListener('keydown', function(e) {{
    if (e.target.tagName === 'INPUT') return;
    switch(e.key) {{
      case 'j': case 'ArrowDown':
        e.preventDefault();
        if (selected < rows.length - 1) {{ selected++; updateSelection(); }}
        break;
      case 'k': case 'ArrowUp':
        e.preventDefault();
        if (selected > 0) {{ selected--; updateSelection(); }}
        break;
      case 'Enter':
        e.preventDefault();
        if (rows[selected]) {{
          var link = rows[selected].querySelector('a');
          if (link) link.click();
        }}
        break;
      case 'Backspace': case 'h':
        e.preventDefault();
        var parentLink = document.querySelector('a[data-parent]');
        if (parentLink) parentLink.click();
        break;
    }}
  }});
  updateSelection();
}})();
</script>
</body>
</html>"#,
        html_escape(&dir_path),
        breadcrumb_html,
        rows_html
    )
}

pub(crate) fn aileron_404_page(requested_url: &str) -> String {
    format!(r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Page Not Found</title>
<style>
body {{ font-family: monospace; background: #1a1a2e; color: #ccc; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; }}
.container {{ text-align: center; }}
h1 {{ color: #ff6b6b; font-size: 3em; margin-bottom: 0.3em; }}
p {{ color: #888; margin: 0.5em 0; }}
a {{ color: #4db4ff; }}
.url {{ color: #666; font-size: 0.9em; margin-top: 1em; word-break: break-all; }}
</style></head><body>
<div class="container">
<h1>404</h1>
<p>Page not found</p>
<p class="url">{url}</p>
<p><a href="aileron://new">Go to new tab</a></p>
</div></body></html>"#, url = html_escape(requested_url))
}

pub(crate) fn aileron_terminal_page() -> String {
    r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Terminal</title>
<style>
body {{ font-family: monospace; background: #1a1a2e; color: #ccc; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; }}
.container {{ text-align: center; }}
h1 {{ color: #4db4ff; }}
p {{ color: #888; }}
kbd {{ background: #333; padding: 2px 6px; border-radius: 3px; border: 1px solid #555; }}
</style></head><body>
<div class="container">
<h1>Terminal</h1>
<p>Use <kbd>Ctrl+Shift+T</kbd> or <kbd>:terminal</kbd> to open a terminal pane.</p>
<p>The terminal renders directly in this window with native performance.</p>
</div></body></html>"#.to_string()
}

pub(crate) fn aileron_settings_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Aileron Settings</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #1a1a2e; color: #e0e0e0; font-family: 'SF Mono', 'Fira Code', monospace; padding: 2em; max-width: 700px; }
  h1 { color: #4db4ff; margin-bottom: 0.5em; font-size: 1.8em; }
  .subtitle { color: #666; margin-bottom: 1.5em; font-size: 0.85em; }
  h2 { color: #4db4ff; margin: 1.5em 0 0.5em; font-size: 1.1em; border-bottom: 1px solid #333; padding-bottom: 0.3em; }
  .field { margin: 0.6em 0; }
  label { display: block; margin-bottom: 0.2em; color: #999; font-size: 0.85em; }
  input[type="text"], input[type="url"], input[type="number"], select {
    background: #16213e; border: 1px solid #333; color: #e0e0e0;
    padding: 7px 10px; font-family: inherit; font-size: 13px;
    width: 100%; max-width: 480px; border-radius: 4px; outline: none;
  }
  input:focus, select:focus { border-color: #4db4ff; }
  .toggle-row { display: flex; align-items: center; margin: 0.5em 0; gap: 8px; }
  .toggle-row label { margin: 0; color: #e0e0e0; font-size: 0.95em; cursor: pointer; }
  button {
    background: #4db4ff; color: #000; border: none; padding: 9px 22px;
    font-family: inherit; font-size: 13px; font-weight: bold;
    border-radius: 4px; cursor: pointer; margin-top: 1.2em;
  }
  button:hover { background: #3a9fe0; }
  button:focus { outline: 2px solid #4db4ff; outline-offset: 2px; }
  #status { color: #888; margin-top: 0.6em; font-size: 0.85em; min-height: 1.2em; }
  #status.ok { color: #4caf50; }
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px;
              overflow: hidden; clip: rect(0,0,0,0); border: 0; }
</style>
</head>
<body>
<h1>Settings</h1>
<p class="subtitle">aileron://settings</p>

<form role="form" aria-label="Aileron settings">

<h2>General</h2>
<div class="field">
  <label for="homepage">Homepage URL</label>
  <input type="url" id="homepage" tabindex="1" aria-label="Homepage URL" />
</div>
<div class="field">
  <label for="search_engine">Search Engine</label>
  <select id="search_engine" tabindex="2" aria-label="Search engine">
    <!-- Populated dynamically from config.search_engines -->
  </select>
</div>
<div class="toggle-row">
  <input type="checkbox" id="restore_session" tabindex="3" role="switch" aria-checked="false" />
  <label for="restore_session">Restore previous session on startup</label>
</div>
<div class="toggle-row">
  <input type="checkbox" id="auto_save" role="switch" aria-checked="false" />
  <label for="auto_save">Auto-save workspace</label>
</div>

<h2>Engine</h2>
<div class="field">
  <label for="engine_selection">Rendering Engine</label>
  <select id="engine_selection" aria-label="Rendering engine">
    <option value="auto">auto</option>
    <option value="servo">servo</option>
    <option value="webkit">webkit</option>
  </select>
  <span class="subtitle" style="color:#666;font-size:0.8em;margin-top:0.2em;display:block">auto = best engine per site, servo = Servo when available, webkit = always WebKit</span>
</div>

<h2>Language</h2>
<div class="field">
  <label for="language">Interface Language</label>
  <select id="language" aria-label="Interface language">
    <option value="en">English</option>
    <option value="zh">中文</option>
    <option value="ja">日本語</option>
    <option value="ko">한국어</option>
    <option value="de">Deutsch</option>
    <option value="fr">Français</option>
    <option value="es">Español</option>
    <option value="pt">Português</option>
    <option value="ru">Русский</option>
  </select>
</div>

<h2>Appearance</h2>
<div class="field">
  <label for="tab_layout">Tab Layout</label>
  <select id="tab_layout" tabindex="4" aria-label="Tab layout">
    <option value="sidebar">Sidebar</option>
    <option value="topbar">Top Bar</option>
    <option value="none">None</option>
  </select>
</div>
<div class="field">
  <label for="tab_sidebar_width">Sidebar Width (px)</label>
  <input type="text" id="tab_sidebar_width" tabindex="5" aria-label="Sidebar width in pixels" />
</div>
<div class="toggle-row">
  <input type="checkbox" id="tab_sidebar_right" tabindex="6" role="switch" aria-checked="false" />
  <label for="tab_sidebar_right">Sidebar on right</label>
</div>

<h2>Theme</h2>
<div class="field">
  <label for="theme">Color Theme</label>
  <select id="theme" aria-label="Color theme">
    <option value="dark">Dark</option>
    <option value="light">Light</option>
    <option value="gruvbox-dark">Gruvbox Dark</option>
    <option value="nord">Nord</option>
    <option value="dracula">Dracula</option>
    <option value="solarized-dark">Solarized Dark</option>
    <option value="solarized-light">Solarized Light</option>
  </select>
</div>

<h2>Privacy</h2>
<div class="toggle-row">
  <input type="checkbox" id="adblock_enabled" tabindex="7" role="switch" aria-checked="false" />
  <label for="adblock_enabled">Block ads</label>
</div>
<div class="toggle-row">
  <input type="checkbox" id="https_upgrade_enabled" tabindex="8" role="switch" aria-checked="false" />
  <label for="https_upgrade_enabled">Automatic HTTPS upgrade</label>
</div>
<div class="toggle-row">
  <input type="checkbox" id="tracking_protection_enabled" tabindex="9" role="switch" aria-checked="false" />
  <label for="tracking_protection_enabled">Tracking protection</label>
</div>
<div class="toggle-row">
  <input type="checkbox" id="popup_blocker_enabled" role="switch" aria-checked="false" />
  <label for="popup_blocker_enabled">Block Popups</label>
</div>
<div class="toggle-row">
  <input type="checkbox" id="adblock_cosmetic_filtering" role="switch" aria-checked="false" />
  <label for="adblock_cosmetic_filtering">Cosmetic filtering (element hiding)</label>
</div>
<div class="field">
  <label for="adblock_update_interval_hours">Filter List Update Interval (hours)</label>
  <input type="number" id="adblock_update_interval_hours" min="1" max="168" aria-label="Filter list update interval in hours" />
  <span class="subtitle" style="color:#666;font-size:0.8em;margin-top:0.2em;display:block">How often to check for filter list updates</span>
</div>

<h2>Advanced</h2>
<div class="toggle-row">
  <input type="checkbox" id="adaptive_quality" role="switch" aria-checked="false" />
  <label for="adaptive_quality">Adaptive Quality</label>
</div>
<span style="color:#666;font-size:0.8em;display:block;margin:-0.3em 0 0.5em 28px">Automatically reduce rendering quality when frame rate drops below 60fps</span>
<div class="toggle-row">
  <input type="checkbox" id="devtools" tabindex="10" role="switch" aria-checked="false" />
  <label for="devtools">Enable DevTools</label>
</div>
<div class="field">
  <label for="proxy">Proxy URL</label>
  <input type="text" id="proxy" tabindex="11" placeholder="socks5://127.0.0.1:1080" aria-label="Proxy URL" />
</div>
<div class="field">
  <label for="custom_css">Custom CSS</label>
  <input type="text" id="custom_css" tabindex="12" placeholder="body { background: #000 !important; }" aria-label="Custom CSS to inject into pages" />
</div>

<h2>Sync</h2>
<div class="field">
  <label for="sync_target">Sync Target</label>
  <input type="text" id="sync_target" placeholder="user@host:/path or /local/path" aria-label="Sync target path or SSH destination" />
  <span class="subtitle" style="color:#666;font-size:0.8em;margin-top:0.2em;display:block">SSH target (user@host:path) or local directory. Empty to disable sync.</span>
</div>
<div class="toggle-row">
  <input type="checkbox" id="sync_encrypted" role="switch" aria-checked="false" />
  <label for="sync_encrypted">Encrypt sync data</label>
</div>
<div class="field">
  <label for="sync_passphrase">Encryption Passphrase</label>
  <input type="password" id="sync_passphrase" placeholder="Leave empty to keep current" aria-label="Sync encryption passphrase" autocomplete="new-password" />
  <span class="subtitle" style="color:#666;font-size:0.8em;margin-top:0.2em;display:block">Required if encryption is enabled. Stored in system keyring, not in config file.</span>
</div>
<div class="toggle-row">
  <input type="checkbox" id="sync_auto" role="switch" aria-checked="false" />
  <label for="sync_auto">Auto-sync on file changes</label>
</div>
<div class="field">
  <label for="sync_auto_interval_sec">Auto-sync Interval (seconds)</label>
  <input type="number" id="sync_auto_interval_sec" min="10" max="3600" aria-label="Auto-sync interval in seconds" />
</div>

<button type="button" id="save-btn" tabindex="13" aria-label="Save settings">Save Settings</button>
</form>
<div id="status" aria-live="polite"></div>

<script>
(function() {
  window._onConfigLoaded = function(cfg) {
    document.getElementById('homepage').value = cfg.homepage || '';
    document.getElementById('search_engine').value = cfg.search_engine || '';
    document.getElementById('restore_session').checked = !!cfg.restore_session;
    document.getElementById('tab_layout').value = cfg.tab_layout || 'sidebar';
    document.getElementById('tab_sidebar_width').value = cfg.tab_sidebar_width || 180;
    document.getElementById('tab_sidebar_right').checked = !!cfg.tab_sidebar_right;
    document.getElementById('adblock_enabled').checked = !!cfg.adblock_enabled;
    document.getElementById('https_upgrade_enabled').checked = !!cfg.https_upgrade_enabled;
    document.getElementById('tracking_protection_enabled').checked = !!cfg.tracking_protection_enabled;
    document.getElementById('devtools').checked = !!cfg.devtools;
    document.getElementById('proxy').value = cfg.proxy || '';
    document.getElementById('custom_css').value = cfg.custom_css || '';
    document.getElementById('engine_selection').value = cfg.engine_selection || 'auto';
    document.getElementById('language').value = cfg.language || 'en';
    document.getElementById('adaptive_quality').checked = !!cfg.adaptive_quality;
    document.getElementById('popup_blocker_enabled').checked = !!cfg.popup_blocker_enabled;
    document.getElementById('adblock_update_interval_hours').value = cfg.adblock_update_interval_hours || 24;
    document.getElementById('theme').value = cfg.theme || 'dark';
    document.getElementById('adblock_cosmetic_filtering').checked = !!cfg.adblock_cosmetic_filtering;
    document.getElementById('auto_save').checked = !!cfg.auto_save;
    document.getElementById('sync_target').value = cfg.sync_target || '';
    document.getElementById('sync_encrypted').checked = !!cfg.sync_encrypted;
    document.getElementById('sync_passphrase').value = '';
    document.getElementById('sync_auto').checked = !!cfg.sync_auto;
    document.getElementById('sync_auto_interval_sec').value = cfg.sync_auto_interval_sec || 300;
    // Populate search engine dropdown from config
    (function() {
      var sel = document.getElementById('search_engine');
      sel.innerHTML = '';
      var engines = cfg.search_engines || {};
      var current = cfg.search_engine || '';
      var found = false;
      Object.keys(engines).forEach(function(name) {
        var opt = document.createElement('option');
        opt.value = engines[name];
        opt.textContent = name;
        if (engines[name] === current) { opt.selected = true; found = true; }
        sel.appendChild(opt);
      });
      // If current engine not in the list, add it as a custom option
      if (!found && current) {
        var opt = document.createElement('option');
        opt.value = current;
        opt.textContent = 'Custom';
        opt.selected = true;
        sel.appendChild(opt);
      }
    })();
    document.querySelectorAll('input[role="switch"]').forEach(function(el) {
      el.setAttribute('aria-checked', el.checked ? 'true' : 'false');
      el.addEventListener('change', function() {
        el.setAttribute('aria-checked', el.checked ? 'true' : 'false');
      });
    });
  };
  window._onConfigSaved = function() {
    var s = document.getElementById('status');
    s.textContent = 'Settings saved';
    s.className = 'ok';
    setTimeout(function() { s.textContent = ''; s.className = ''; }, 3000);
  };
  function collectConfig() {
    return {
      homepage: document.getElementById('homepage').value,
      search_engine: document.getElementById('search_engine').value,
      restore_session: document.getElementById('restore_session').checked,
      tab_layout: document.getElementById('tab_layout').value,
      tab_sidebar_width: parseFloat(document.getElementById('tab_sidebar_width').value) || 180,
      tab_sidebar_right: document.getElementById('tab_sidebar_right').checked,
      adblock_enabled: document.getElementById('adblock_enabled').checked,
      https_upgrade_enabled: document.getElementById('https_upgrade_enabled').checked,
      tracking_protection_enabled: document.getElementById('tracking_protection_enabled').checked,
      devtools: document.getElementById('devtools').checked,
      proxy: document.getElementById('proxy').value || null,
      custom_css: document.getElementById('custom_css').value || null,
      engine_selection: document.getElementById('engine_selection').value,
      language: document.getElementById('language').value || null,
      adaptive_quality: document.getElementById('adaptive_quality').checked,
      popup_blocker_enabled: document.getElementById('popup_blocker_enabled').checked,
      adblock_update_interval_hours: parseInt(document.getElementById('adblock_update_interval_hours').value) || 24,
      theme: document.getElementById('theme').value,
      adblock_cosmetic_filtering: document.getElementById('adblock_cosmetic_filtering').checked,
      auto_save: document.getElementById('auto_save').checked,
      sync_target: document.getElementById('sync_target').value || '',
      sync_encrypted: document.getElementById('sync_encrypted').checked,
      sync_passphrase: document.getElementById('sync_passphrase').value || null,
      sync_auto: document.getElementById('sync_auto').checked,
      sync_auto_interval_sec: parseInt(document.getElementById('sync_auto_interval_sec').value) || 300
    };
  }
  document.getElementById('save-btn').addEventListener('click', function() {
    window.ipc.postMessage(JSON.stringify({t:'set-config', config: collectConfig()}));
  });
  document.addEventListener('keydown', function(e) {
    if (e.key === 'Enter' && e.target.tagName !== 'BUTTON') {
      e.preventDefault();
      document.getElementById('save-btn').click();
    }
  });
  window.ipc.postMessage(JSON.stringify({t:'get-config'}));
})();
</script>
</body>
</html>"#.to_string()
}

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

        // Position should be offset by status bar height
        let pos = match result.position {
            Position::Logical(p) => p,
            _ => panic!("Expected Logical position"),
        };
        assert_eq!(pos.x, 0.0);
        assert_eq!(pos.y, 32.0);

        // Width unchanged, height reduced by both bars
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
        // If the rect is too short, height should be clamped to 100.0
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
        // Bars taller than the rect — should clamp height to 100.0
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
        assert_eq!(pos.x, 0.0, "X should not be offset when sidebar is on right");
        assert_eq!(pos.y, 32.0);

        let size = match result.size {
            Size::Logical(s) => s,
            _ => panic!("Expected Logical size"),
        };
        assert_eq!(size.width, 620.0, "Width should be reduced by sidebar");
    }

    // ─── HTML page generator tests ────────────────────────────────

    #[test]
    fn test_welcome_page_is_valid_html() {
        let html = aileron_welcome_page();
        assert!(
            html.starts_with("<!DOCTYPE html>"),
            "Should start with DOCTYPE"
        );
        assert!(
            html.contains("<title>Aileron</title>"),
            "Should have Aileron title"
        );
        assert!(html.contains("</html>"), "Should close html tag");
        assert!(html.len() > 100, "Should be substantial content");
    }

    #[test]
    fn test_welcome_page_contains_keybinding_hints() {
        let html = aileron_welcome_page();
        assert!(
            html.contains("kbd"),
            "Should contain keyboard shortcut hints"
        );
        assert!(
            html.contains("Command palette"),
            "Should mention command palette"
        );
        assert!(
            html.contains("Split vertical"),
            "Should mention split vertical"
        );
    }

    #[test]
    fn test_new_tab_page_is_valid_html() {
        let html = aileron_new_tab_page();
        assert!(
            html.starts_with("<!DOCTYPE html>"),
            "Should start with DOCTYPE"
        );
        assert!(
            html.contains("<title>New Tab</title>"),
            "Should have New Tab title"
        );
        assert!(html.contains("</html>"), "Should close html tag");
    }

    #[test]
    fn test_new_tab_page_contains_navigation_hint() {
        let html = aileron_new_tab_page();
        assert!(
            html.contains("Ctrl+P"),
            "Should mention Ctrl+P for navigation"
        );
        assert!(html.contains("Search"), "Should have search functionality");
    }

    #[test]
    fn test_file_browser_page_generates_valid_html() {
        let uri = wry::http::Uri::from_static("aileron://files?path=%2Ftmp");
        let html = file_browser_page(&uri);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Files:"));
        assert!(html.contains("<table"));
    }

    #[test]
    fn test_file_browser_page_uses_home_dir_as_default() {
        let uri = wry::http::Uri::from_static("aileron://files");
        let html = file_browser_page(&uri);
        assert!(html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn test_file_browser_page_handles_invalid_path() {
        let uri = wry::http::Uri::from_static("aileron://files?path=%2Fnonexistent_dir_xyz");
        let html = file_browser_page(&uri);
        assert!(html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn test_percent_encode_path() {
        assert_eq!(percent_encode_path("/home/user"), "/home/user");
        assert_eq!(
            percent_encode_path("/home/user/my file.txt"),
            "/home/user/my%20file.txt"
        );
        assert_eq!(
            percent_encode_path("/home/user/dir with spaces/"),
            "/home/user/dir%20with%20spaces/"
        );
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("/home/user"), "/home/user");
        assert_eq!(
            percent_decode("/home/user/my%20file.txt"),
            "/home/user/my file.txt"
        );
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1_048_576), "1.0 MB");
        assert_eq!(format_size(1_073_741_824), "1.0 GB");
    }
}
