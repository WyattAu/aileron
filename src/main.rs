use std::sync::Arc;
use tracing::{info, warn};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

use aileron::app::AppState;
use aileron::config::Config;
use aileron::frame_tasks;
use aileron::input::{KeyEvent as AileronKeyEvent, Modifiers};
#[cfg(feature = "mcp")]
use aileron::mcp::McpBridge;
use aileron::net::adblock::AdBlocker;
use aileron::offscreen_webview::OffscreenWebViewManager;
use aileron::popup::PopupManager;
use aileron::servo::{WryPaneManager, bsp_rect_to_wry_rect};
#[cfg(feature = "terminal")]
use aileron::terminal::NativeTerminalManager;
use aileron::test_harness::TestHarness;
use aileron::wm::Rect;

mod app_handler;
mod bootstrap;
mod event_handlers;

#[cfg(feature = "terminal")]
use event_handlers::key_to_escape_sequence;
use event_handlers::{clear_hints_js, hint_click_js};

/// Heights (in logical pixels) for the panels.
const STATUS_BAR_HEIGHT: f64 = 32.0;
const URL_BAR_HEIGHT: f64 = 32.0;

/// The top-level application holding window and app logic.
struct AileronApp {
    window: Option<Arc<Window>>,
    app_state: Option<AppState>,
    modifiers: Modifiers,
    config: Config,

    /// Wry webview panes — one per BSP leaf.
    /// Must live here because wry::WebView is !Send + !Sync.
    wry_panes: WryPaneManager,

    /// Ad-blocker instance shared across all wry navigation handlers.
    adblocker: AdBlocker,

    /// Bridge between MCP background thread and main thread.
    #[cfg(feature = "mcp")]
    mcp_bridge: McpBridge,

    /// Terminal manager for embedded terminal panes.
    #[cfg(feature = "terminal")]
    terminal_manager: NativeTerminalManager,

    content_scripts: aileron::scripts::ContentScriptManager,

    /// Current git status for the working directory.
    git_status: aileron::git::GitStatus,

    /// Background thread for polling git status (avoids blocking main thread).
    git_poller: Option<aileron::git::GitPoller>,

    /// Standalone popup browser windows.
    popup: PopupManager,

    /// Tracks whether the first frame has rendered (for startup timing).
    first_frame: bool,

    /// Whether a resize happened and panes need repositioning.
    resize_pending: bool,

    /// Frame counter for diagnostics.
    frame_count: u64,

    /// Instant when the app was created (for startup timing).
    startup_start: std::time::Instant,

    /// Offscreen webview panes (Architecture B).
    offscreen_panes: OffscreenWebViewManager,

    /// Atomic flag set by background thread when filter lists are updated.
    adblock_reload_pending: std::sync::Arc<std::sync::atomic::AtomicBool>,

    /// Last time adblock filter lists were updated (for periodic refresh).
    last_filter_update: std::time::Instant,

    /// Guards against double-processing: on Wayland, a single keystroke
    /// can produce both an Ime::Commit and a KeyboardInput event.
    ime_just_committed: bool,
    /// Whether the wry webview currently has X11 focus (native mode only).
    webview_has_focus: bool,

    /// Leptos WASM chrome webview (Phase 2b+).
    chrome_webview: Option<wry::WebView>,

    /// IPC channel for the chrome webview.
    chrome_ipc_rx: Option<crossbeam_channel::Receiver<String>>,

    /// Whether the chrome webview has been initialized.
    chrome_initialized: bool,

    /// Cached version string (computed once at startup, avoids per-frame format!).
    version_string: String,

    /// Whether the chrome state needs to be pushed to the WASM overlay.
    /// Set to true whenever mode, URL, tabs, or other visible state changes.
    chrome_dirty: bool,

    /// Deferred pane creation queue (TASK-K27).
    pending_pane_creates: std::collections::VecDeque<(uuid::Uuid, url::Url)>,

    /// Internal test harness for automated UI state traversal.
    test_harness: Option<TestHarness>,
}

impl AileronApp {
    fn new() -> Self {
        let config = Config::load();

        // Set proxy environment variable if configured
        if let Some(ref proxy) = config.proxy {
            // SAFETY: This runs before any threads are spawned (before event loop creation).
            unsafe { std::env::set_var("all_proxy", proxy) };
            info!("Proxy configured: {}", proxy);
        }

        #[cfg(feature = "mcp")]
        let mcp_bridge = McpBridge::new();
        Self {
            window: None,
            app_state: None,
            modifiers: Modifiers::none(),
            config,
            wry_panes: WryPaneManager::new(),
            adblocker: AdBlocker::new(),
            #[cfg(feature = "mcp")]
            mcp_bridge,
            #[cfg(feature = "terminal")]
            terminal_manager: NativeTerminalManager::new(),
            content_scripts: aileron::scripts::ContentScriptManager::new(),
            git_status: aileron::git::GitStatus::default(),
            git_poller: Some(aileron::git::GitPoller::new(
                std::path::PathBuf::from("."),
                std::time::Duration::from_secs(1),
            )),
            popup: PopupManager::new(),
            first_frame: true,
            frame_count: 0,
            startup_start: std::time::Instant::now(),
            offscreen_panes: OffscreenWebViewManager::new(),
            adblock_reload_pending: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            resize_pending: false,
            last_filter_update: std::time::Instant::now(),
            ime_just_committed: false,
            webview_has_focus: false,
            chrome_webview: None,
            chrome_ipc_rx: None,
            chrome_initialized: false,
            version_string: format!(
                "v{} ({})",
                env!("CARGO_PKG_VERSION"),
                option_env!("AILERON_GIT_HASH").unwrap_or("unknown")
            ),
            chrome_dirty: true,
            pending_pane_creates: std::collections::VecDeque::new(),
            test_harness: None,
        }
    }

    fn init_app_state(&mut self, window: Arc<Window>) {
        info!("── init_app_state(): Starting ──");

        let size = window.inner_size();
        let scale = window.scale_factor();
        let viewport = Rect::new(
            0.0,
            0.0,
            size.width as f64 / scale,
            size.height as f64 / scale,
        );
        let app_state = match AppState::new(viewport, self.config.clone()) {
            Ok(s) => {
                info!(
                    "Application state initialized with {} panes",
                    s.wm.leaf_count()
                );
                s
            }
            Err(e) => {
                tracing::error!("Failed to initialize app state: {}", e);
                return;
            }
        };

        let loaded_count = {
            let mut m = app_state.extension_manager.write();
            let count = m.load_all().len();
            m.register_builtin_adblock();
            count
        };
        if loaded_count > 0 {
            info!("Loaded {} user extension(s)", loaded_count);
        }
        {
            let mgr = app_state.extension_manager.read();
            info!(
                "Built-in adblock: {}",
                if mgr.is_builtin_adblock_enabled() {
                    "enabled"
                } else {
                    "disabled"
                }
            );
        }

        self.content_scripts.set_extension_registry(
            app_state
                .extension_manager
                .read()
                .content_script_registry()
                .clone(),
        );

        self.app_state = Some(app_state);
        self.window = Some(window);
        info!(
            "init_app_state() completed in {:?}",
            self.startup_start.elapsed()
        );
    }

    /// Initialize the Leptos WASM chrome webview.
    fn init_chrome_webview(&mut self) {
        if self.chrome_initialized {
            return;
        }
        self.chrome_initialized = true;

        let window = match &self.window {
            Some(w) => Arc::clone(w),
            None => return,
        };

        let dist_dir = aileron::chrome_bridge::find_chrome_dist_dir();

        if !dist_dir.join("index.html").exists() {
            tracing::error!(
                "Chrome webview required but trunk build not found at {:?}. \
                 Run 'trunk build' in chrome/ directory first.",
                dist_dir
            );
            return;
        }

        let (ipc_tx, ipc_rx) = crossbeam_channel::bounded::<String>(16);
        self.chrome_ipc_rx = Some(ipc_rx);

        let chrome_webview = wry::WebViewBuilder::new()
            .with_url("aileron-chrome://chrome/index.html")
            .with_custom_protocol(
                "aileron-chrome".into(),
                aileron::chrome_bridge::chrome_asset_handler(dist_dir),
            )
            .with_ipc_handler(move |req: wry::http::Request<String>| {
                let body = req.into_body();
                let _ = ipc_tx.send(body);
            })
            .with_transparent(true)
            .build_as_child(&*window);

        match chrome_webview {
            Ok(wv) => {
                info!("Chrome webview created (Leptos WASM)");
                self.chrome_webview = Some(wv);
            }
            Err(e) => {
                tracing::warn!("Failed to create chrome webview: {e}");
            }
        }
    }

    fn create_wry_pane_for(&mut self, pane_id: uuid::Uuid, url: &url::Url) {
        if self.config.is_offscreen() {
            self.create_offscreen_pane_for(pane_id, url);
            return;
        }

        let window = match &self.window {
            Some(w) => Arc::clone(w),
            None => return,
        };

        #[cfg(feature = "terminal")]
        let is_terminal = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            app_state.is_terminal_pane(&pane_id)
        };

        let wm_rect = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            let panes = app_state.wm.panes_ref();
            match panes.iter().find(|(id, _)| *id == pane_id) {
                Some((_, rect)) => *rect,
                None => {
                    warn!("BSP rect not found for pane {}", &pane_id.to_string()[..8]);
                    return;
                }
            }
        };

        let wry_rect = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            let tab_layout = app_state.config.tab_layout.as_str();
            let sidebar_width = if tab_layout == "sidebar" {
                app_state.config.tab_sidebar_width as f64
            } else {
                0.0
            };
            let sidebar_on_right = app_state.config.tab_sidebar_right;
            bsp_rect_to_wry_rect(
                &wm_rect,
                STATUS_BAR_HEIGHT,
                URL_BAR_HEIGHT,
                sidebar_width,
                sidebar_on_right,
            )
        };

        let blocked_domains: Vec<String> = self.adblocker.blocked_domains_iter();

        let https_safe_list = if self.config.https_upgrade_enabled {
            self.app_state
                .as_mut()
                .map(|s| s.get_cached_https_safe_list())
                .unwrap_or_default()
        } else {
            std::sync::Arc::new(std::collections::HashSet::new())
        };

        let interceptor_registry = self
            .app_state
            .as_ref()
            .map(|s| s.extension_manager.read().interceptor_registry.clone());

        match self.wry_panes.create_pane(
            &*window,
            pane_id,
            url.clone(),
            wry_rect,
            blocked_domains,
            https_safe_list,
            self.config.devtools,
            self.config.popup_blocker_enabled,
            interceptor_registry,
        ) {
            Ok(()) => {
                #[cfg(feature = "terminal")]
                if is_terminal {
                    match self.terminal_manager.create_terminal(pane_id, 80, 24) {
                        Ok(_size) => {
                            if let Some(app_state) = &mut self.app_state
                                && let Some(cmd) = app_state.pending_terminal_command.take()
                            {
                                self.terminal_manager.write_input(&pane_id, &cmd);
                            }
                        }
                        Err(e) => warn!("Failed to create terminal: {}", e),
                    }
                }

                let mode = self.wry_panes.get(&pane_id).map(|p| p.embed_mode());
                let mode_str = match mode {
                    Some(aileron::servo::EmbedMode::ChildWindow) => "X11 child",
                    Some(aileron::servo::EmbedMode::GtkWindow) => "GTK window (Wayland)",
                    None => "unknown",
                };
                info!(
                    "WryPane {} created ({}) -> {}",
                    &pane_id.to_string()[..8],
                    mode_str,
                    url
                );
            }
            Err(e) => {
                warn!("Failed to create WryPane: {}", e);
                if let Some(app_state) = &mut self.app_state {
                    app_state.ui.status_message = format!("Pane creation failed: {e}");
                }
            }
        }
    }

    fn create_offscreen_pane_for(&mut self, pane_id: uuid::Uuid, url: &url::Url) {
        #[cfg(feature = "terminal")]
        let is_terminal = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            app_state.is_terminal_pane(&pane_id)
        };

        let wm_rect = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            let panes = app_state.wm.panes_ref();
            match panes.iter().find(|(id, _)| *id == pane_id) {
                Some((_, rect)) => *rect,
                None => {
                    warn!("BSP rect not found for pane {}", &pane_id.to_string()[..8]);
                    return;
                }
            }
        };

        let wry_rect = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            let tab_layout = app_state.config.tab_layout.as_str();
            let sidebar_width = if tab_layout == "sidebar" {
                app_state.config.tab_sidebar_width as f64
            } else {
                0.0
            };
            let sidebar_on_right = app_state.config.tab_sidebar_right;
            bsp_rect_to_wry_rect(
                &wm_rect,
                STATUS_BAR_HEIGHT,
                URL_BAR_HEIGHT,
                sidebar_width,
                sidebar_on_right,
            )
        };

        let (width, height) = match wry_rect.size {
            winit::dpi::Size::Logical(s) => (s.width as i32, s.height as i32),
            winit::dpi::Size::Physical(s) => (s.width as i32, s.height as i32),
        };

        let blocked_domains: Vec<String> = self.adblocker.blocked_domains_iter();

        let https_safe_list = if self.config.https_upgrade_enabled {
            self.app_state
                .as_mut()
                .map(|s| s.get_cached_https_safe_list())
                .unwrap_or_default()
        } else {
            std::sync::Arc::new(std::collections::HashSet::new())
        };

        let interceptor_registry = self
            .app_state
            .as_ref()
            .map(|s| s.extension_manager.read().interceptor_registry.clone());

        #[cfg(target_os = "linux")]
        match self.offscreen_panes.create_pane_with_privacy(
            pane_id,
            url,
            width,
            height,
            blocked_domains,
            https_safe_list,
            true,
            true,
            self.config.devtools,
            self.config.popup_blocker_enabled,
            interceptor_registry,
        ) {
            Ok(()) => {
                #[cfg(feature = "terminal")]
                if is_terminal {
                    match self.terminal_manager.create_terminal(pane_id, 80, 24) {
                        Ok(_size) => {
                            if let Some(app_state) = &mut self.app_state
                                && let Some(cmd) = app_state.pending_terminal_command.take()
                            {
                                self.terminal_manager.write_input(&pane_id, &cmd);
                            }
                        }
                        Err(e) => warn!("Failed to create terminal: {}", e),
                    }
                }

                info!(
                    "OffscreenWebView {} created -> {}",
                    &pane_id.to_string()[..8],
                    url
                );
            }
            Err(e) => {
                warn!("Failed to create OffscreenWebView: {}", e);
                if let Some(app_state) = &mut self.app_state {
                    app_state.ui.status_message = format!("Pane creation failed: {e}");
                }
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (
                pane_id,
                url,
                width,
                height,
                blocked_domains,
                interceptor_registry,
            );
            warn!("Offscreen webview not supported on this platform");
        }
    }

    fn remove_wry_pane_for(&mut self, pane_id: &uuid::Uuid) {
        #[cfg(feature = "terminal")]
        self.terminal_manager.remove(pane_id);
        self.wry_panes.remove_pane(pane_id);
        self.offscreen_panes.remove_pane(pane_id);
        self.pending_pane_creates.retain(|(id, _)| id != pane_id);
        if let Some(app_state) = &mut self.app_state {
            app_state.cleanup_pane_state(pane_id);
        }
    }

    fn init_popup_window(&mut self, window_id: WindowId, window: Arc<Window>) {
        let url = self
            .app_state
            .as_mut()
            .and_then(|s| s.pending_detach_url.take())
            .unwrap_or_else(|| url::Url::parse("aileron://new").unwrap());
        let blocked_domains: Vec<String> = self.adblocker.blocked_domains_iter();
        let https_safe_list = if self.config.https_upgrade_enabled {
            self.app_state
                .as_mut()
                .map(|s| s.get_cached_https_safe_list())
                .unwrap_or_default()
        } else {
            std::sync::Arc::new(std::collections::HashSet::new())
        };

        self.popup.init_popup_window(
            window_id,
            window,
            url,
            blocked_domains,
            https_safe_list,
            self.config.devtools,
        );
    }

    fn handle_popup_event(&mut self, window_id: WindowId, event: &WindowEvent) {
        self.popup.handle_popup_event(window_id, event);
    }

    /// Initialize the test harness if `--test-harness` was specified.
    pub fn init_test_harness(&mut self, output_dir: &std::path::Path, dump_dom: bool) {
        use aileron::test_harness::default_route;
        let mut harness = TestHarness::new(output_dir, dump_dom);
        harness.define_route(default_route());
        self.test_harness = Some(harness);
        tracing::info!("Test harness activated: {}", output_dir.display());
    }

    fn reposition_all_panes(&mut self) {
        let app_state = match &self.app_state {
            Some(s) => s,
            None => return,
        };

        if let Some(window) = &self.window
            && let Some(ref webview) = self.chrome_webview
        {
            let size = window.inner_size();
            let scale = window.scale_factor();
            let w = size.width as f64 / scale;
            let h = size.height as f64 / scale;
            let _ = webview.set_bounds(wry::Rect {
                position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(0.0, 0.0)),
                size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(w, h)),
            });
        }

        let tab_layout = app_state.config.tab_layout.as_str();
        let sidebar_width = if tab_layout == "sidebar" {
            app_state.config.tab_sidebar_width as f64
        } else {
            0.0
        };
        let sidebar_on_right = app_state.config.tab_sidebar_right;

        let panes = app_state.wm.panes_ref();
        for (pane_id, wm_rect) in panes.iter() {
            if let Some(wry_pane) = self.wry_panes.get(pane_id) {
                let wry_rect = bsp_rect_to_wry_rect(
                    wm_rect,
                    STATUS_BAR_HEIGHT,
                    URL_BAR_HEIGHT,
                    sidebar_width,
                    sidebar_on_right,
                );
                wry_pane.set_bounds(wry_rect);
            }
        }

        if self.config.is_offscreen() {
            for (pane_id, wm_rect) in panes.iter() {
                let wry_rect = bsp_rect_to_wry_rect(
                    wm_rect,
                    STATUS_BAR_HEIGHT,
                    URL_BAR_HEIGHT,
                    sidebar_width,
                    sidebar_on_right,
                );
                let (w, h) = match wry_rect.size {
                    winit::dpi::Size::Logical(s) => (s.width as i32, s.height as i32),
                    winit::dpi::Size::Physical(s) => (s.width as i32, s.height as i32),
                };

                if w > 0 && h > 0 {
                    self.offscreen_panes.resize(pane_id, w, h);
                }
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    bootstrap::run()
}

#[cfg(test)]
mod tests {
    fn drain_one_step(
        pending: &mut std::collections::VecDeque<(uuid::Uuid, url::Url)>,
        active_id: Option<uuid::Uuid>,
        live_pane_ids: &std::collections::HashSet<uuid::Uuid>,
    ) -> (usize, usize) {
        let has_active = pending
            .iter()
            .any(|(pid, _)| Some(*pid) == active_id && live_pane_ids.contains(pid));

        let to_create = if has_active {
            pending
                .iter()
                .position(|(pid, _)| Some(*pid) == active_id && live_pane_ids.contains(pid))
        } else {
            pending
                .iter()
                .position(|(pid, _)| live_pane_ids.contains(pid))
        };

        let mut created = 0usize;
        if let Some(idx) = to_create {
            pending.remove(idx);
            created += 1;
        }

        pending.retain(|(pid, _)| live_pane_ids.contains(pid));
        (created, pending.len())
    }

    #[test]
    fn test_staggered_creation_one_per_step() {
        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();
        let id3 = uuid::Uuid::new_v4();
        let url = url::Url::parse("aileron://new").unwrap();

        let mut pending: std::collections::VecDeque<(uuid::Uuid, url::Url)> =
            std::collections::VecDeque::new();
        pending.push_back((id1, url.clone()));
        pending.push_back((id2, url.clone()));
        pending.push_back((id3, url.clone()));

        let live: std::collections::HashSet<uuid::Uuid> = [id1, id2, id3].into_iter().collect();

        let (created, remaining) = drain_one_step(&mut pending, None, &live);
        assert_eq!(created, 1);
        assert_eq!(remaining, 2);

        let (created, remaining) = drain_one_step(&mut pending, None, &live);
        assert_eq!(created, 1);
        assert_eq!(remaining, 1);

        let (created, remaining) = drain_one_step(&mut pending, None, &live);
        assert_eq!(created, 1);
        assert_eq!(remaining, 0);

        let (created, remaining) = drain_one_step(&mut pending, None, &live);
        assert_eq!(created, 0);
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_staggered_creation_active_pane_created_immediately() {
        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();
        let id3 = uuid::Uuid::new_v4();
        let url = url::Url::parse("aileron://new").unwrap();

        let mut pending: std::collections::VecDeque<(uuid::Uuid, url::Url)> =
            std::collections::VecDeque::new();
        pending.push_back((id1, url.clone()));
        pending.push_back((id2, url.clone()));
        pending.push_back((id3, url.clone()));

        let live: std::collections::HashSet<uuid::Uuid> = [id1, id2, id3].into_iter().collect();

        let (created, remaining) = drain_one_step(&mut pending, Some(id2), &live);
        assert_eq!(created, 1);
        assert_eq!(remaining, 2);
        assert_eq!(pending.front().map(|(id, _)| *id), Some(id1));
    }

    #[test]
    fn test_staggered_creation_closed_pane_discarded() {
        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();
        let url = url::Url::parse("aileron://new").unwrap();

        let mut pending: std::collections::VecDeque<(uuid::Uuid, url::Url)> =
            std::collections::VecDeque::new();
        pending.push_back((id1, url.clone()));
        pending.push_back((id2, url.clone()));

        let live: std::collections::HashSet<uuid::Uuid> = [id1].into_iter().collect();

        let (created, remaining) = drain_one_step(&mut pending, None, &live);
        assert_eq!(created, 1);
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_staggered_creation_empty_queue() {
        let mut pending: std::collections::VecDeque<(uuid::Uuid, url::Url)> =
            std::collections::VecDeque::new();
        let live: std::collections::HashSet<uuid::Uuid> = std::collections::HashSet::new();

        let (created, remaining) = drain_one_step(&mut pending, None, &live);
        assert_eq!(created, 0);
        assert_eq!(remaining, 0);
    }
}
