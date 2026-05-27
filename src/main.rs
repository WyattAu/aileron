use std::sync::Arc;
use tracing::{info, warn};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

use aileron::app::AppState;
use aileron::config::Config;
use aileron::frame_tasks;
use aileron::gfx::GfxState;
use aileron::input::{KeyEvent as AileronKeyEvent, Modifiers};
#[cfg(feature = "mcp")]
use aileron::mcp::McpBridge;
use aileron::net::adblock::AdBlocker;
use aileron::offscreen_webview::OffscreenWebViewManager;
use aileron::popup::PopupManager;
use aileron::profiling::AdaptiveQuality;
use aileron::servo::{WryPaneManager, bsp_rect_to_wry_rect};
#[cfg(feature = "terminal")]
use aileron::terminal::NativeTerminalManager;
use aileron::ui::panels;
use aileron::wm::Rect;

mod app_handler;
mod bootstrap;
mod event_handlers;

#[cfg(feature = "terminal")]
use event_handlers::key_to_escape_sequence;
use event_handlers::key_to_js;

/// Heights (in logical pixels) for the egui panels.
const STATUS_BAR_HEIGHT: f64 = 32.0;
const URL_BAR_HEIGHT: f64 = 32.0;

/// The top-level application holding window, GPU state, and app logic.
struct AileronApp {
    window: Option<Arc<Window>>,
    egui_winit: Option<egui_winit::State>,
    gfx: Option<GfxState>,
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

    /// Standalone popup browser windows (no egui overlay, no tiling).
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
    /// Webviews render into gtk::OffscreenWindow buffers; pixel data is
    /// captured and uploaded as egui textures each frame.
    offscreen_panes: OffscreenWebViewManager,

    /// Whether the left mouse button is currently pressed (for drag detection in offscreen mode).
    offscreen_mouse_pressed: bool,

    /// Maps pane IDs to their current egui texture ID.
    /// Updated each frame by `update_webview_textures()`.
    webview_textures: std::collections::HashMap<uuid::Uuid, egui::TextureId>,

    /// Cached texture handles for offscreen panes (TASK-K28).
    /// Reuses GPU textures across frames when dimensions are unchanged.
    webview_texture_handles: std::collections::HashMap<uuid::Uuid, egui::TextureHandle>,

    /// Last time each offscreen pane was captured (for frame rate limiting).
    offscreen_last_capture: std::collections::HashMap<uuid::Uuid, std::time::Instant>,

    /// Reusable capture buffers keyed by pane ID. Avoids per-frame heap allocation
    /// for RGBA pixel data during active scrolling.
    capture_buffers: std::collections::HashMap<uuid::Uuid, Vec<u8>>,

    /// Deferred pane creation queue (TASK-K27).
    /// Background panes are queued here and created one-per-frame in
    /// `about_to_wait()` to prevent startup freeze with many tabs.
    pending_pane_creates: std::collections::VecDeque<(uuid::Uuid, url::Url)>,

    /// Atomic flag set by background thread when filter lists are updated.
    adblock_reload_pending: std::sync::Arc<std::sync::atomic::AtomicBool>,

    /// Adaptive quality renderer (TASK-K24).
    /// Reduces texture capture rate when frames are slow.
    adaptive_quality: AdaptiveQuality,

    /// Last time adblock filter lists were updated (for periodic refresh).
    last_filter_update: std::time::Instant,

    /// Guards against double-processing: on Wayland, a single keystroke
    /// can produce both an Ime::Commit and a KeyboardInput event.
    /// Set to true after handling an Ime::Commit in Normal/Command mode,
    /// then cleared after the corresponding KeyboardInput is skipped.
    ime_just_committed: bool,
}

impl AileronApp {
    fn new() -> Self {
        let config = Config::load();

        // Set proxy environment variable if configured
        if let Some(ref proxy) = config.proxy {
            // SAFETY: This runs before any threads are spawned (before event loop creation).
            // No other thread can read env vars at this point.
            unsafe { std::env::set_var("all_proxy", proxy) };
            info!("Proxy configured: {}", proxy);
        }

        #[cfg(feature = "mcp")]
        let mcp_bridge = McpBridge::new();
        let mut adaptive_quality = AdaptiveQuality::new();
        adaptive_quality.set_enabled(config.adaptive_quality);
        Self {
            window: None,
            egui_winit: None,
            gfx: None,
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
            offscreen_mouse_pressed: false,
            webview_textures: std::collections::HashMap::new(),
            webview_texture_handles: std::collections::HashMap::new(),
            offscreen_last_capture: std::collections::HashMap::new(),
            capture_buffers: std::collections::HashMap::new(),
            pending_pane_creates: std::collections::VecDeque::new(),
            adblock_reload_pending: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            adaptive_quality,
            resize_pending: false,
            last_filter_update: std::time::Instant::now(),
            ime_just_committed: false,
        }
    }

    fn init_graphics(&mut self, window: Arc<Window>) {
        info!("── init_graphics(): Starting ──");

        // Create egui context and winit state
        info!("init_graphics(): Creating egui context...");
        let egui_ctx = egui::Context::default();
        egui_ctx.set_visuals(egui::Visuals::dark());

        let mut winit_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            None,
            None,
            None,
        );

        info!("init_graphics(): Creating GPU state (wgpu + Vulkan)...");
        // Create GPU state
        let gfx = match GfxState::new(Arc::clone(&window)) {
            Ok(g) => g,
            Err(e) => {
                tracing::error!("GPU INIT FAILED: {}", e);
                tracing::error!("This is likely a Vulkan/driver issue. Check:");
                tracing::error!("  1. Vulkan ICDs installed: ls /usr/share/vulkan/icd.d/");
                tracing::error!("  2. NVIDIA driver loaded: lsmod | grep nvidia");
                tracing::error!("  3. Try: LD_LIBRARY_PATH=/usr/lib:$LD_LIBRARY_PATH aileron");
                return;
            }
        };

        info!(
            "init_graphics(): GPU initialized, max texture: {}px",
            gfx.device.limits().max_texture_dimension_2d
        );
        winit_state.set_max_texture_side(gfx.device.limits().max_texture_dimension_2d as usize);

        // Initialize app state with viewport and config.
        // The BSP tree and egui operate in logical (CSS) pixels, but
        // window.inner_size() returns physical pixels. Convert by dividing
        // by the scale factor so all downstream sizing is correct.
        info!("init_graphics(): Creating AppState...");
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

        self.egui_winit = Some(winit_state);
        self.gfx = Some(gfx);
        self.app_state = Some(app_state);
        self.window = Some(window);
        info!(
            "init_graphics() completed in {:?}",
            self.startup_start.elapsed()
        );
    }

    /// Create a wry webview for a BSP pane.
    /// Called when a new pane is created (initial + splits).
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

        // Get the BSP rect for this pane
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

        // Collect blocked domains for the ad-block closure
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
                            // Native terminal: direct PTY write, no IPC sender needed

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

    /// Create an offscreen webview pane for Architecture B rendering.
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
                            // Native terminal: direct PTY write, no IPC sender needed

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

    /// Remove a wry pane when a BSP leaf is closed.
    fn remove_wry_pane_for(&mut self, pane_id: &uuid::Uuid) {
        #[cfg(feature = "terminal")]
        self.terminal_manager.remove(pane_id);
        self.wry_panes.remove_pane(pane_id);
        self.offscreen_panes.remove_pane(pane_id);
        self.webview_textures.remove(pane_id);
        self.webview_texture_handles.remove(pane_id);
        self.offscreen_last_capture.remove(pane_id);
        self.capture_buffers.remove(pane_id);
        self.pending_pane_creates.retain(|(id, _)| id != pane_id);
        // Clean up per-pane state to prevent memory leaks
        if let Some(app_state) = &mut self.app_state {
            app_state.cleanup_pane_state(pane_id);
        }
    }

    /// Create a wry webview for a standalone popup window.
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

    /// Create at most one deferred offscreen pane per frame (TASK-K27).
    /// When the active pane switches to a deferred pane, creates it immediately.
    fn drain_pending_pane_creates(&mut self) {
        if self.pending_pane_creates.is_empty() {
            return;
        }

        let active_id = self.app_state.as_ref().map(|s| s.wm.active_pane_id());

        let current_pane_ids: std::collections::HashSet<uuid::Uuid> = self
            .app_state
            .as_ref()
            .map(|s| s.wm.panes_ref().iter().map(|(id, _)| *id).collect())
            .unwrap_or_default();

        let has_active = self
            .pending_pane_creates
            .iter()
            .any(|(pid, _)| Some(*pid) == active_id && current_pane_ids.contains(pid));

        let to_create = if has_active {
            self.pending_pane_creates
                .iter()
                .position(|(pid, _)| Some(*pid) == active_id && current_pane_ids.contains(pid))
        } else {
            self.pending_pane_creates
                .iter()
                .position(|(pid, _)| current_pane_ids.contains(pid))
        };

        if let Some(idx) = to_create {
            let (pid, url) = self
                .pending_pane_creates
                .remove(idx)
                .expect("pending pane must exist at index from position()");
            self.create_wry_pane_for(pid, &url);
        }

        self.pending_pane_creates
            .retain(|(pid, _)| current_pane_ids.contains(pid));
    }

    /// Handle a window event for a popup window.
    fn handle_popup_event(&mut self, window_id: WindowId, event: &WindowEvent) {
        self.popup.handle_popup_event(window_id, event);
    }

    /// Reposition all wry panes to match current BSP layout.
    /// Called on window resize and after splits/closes.
    fn reposition_all_panes(&mut self) {
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
            #[cfg(feature = "terminal")]
            {
                use aileron::terminal::grid::CellMetrics;

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

                    // Auto-resize native terminals to fit the pane
                    if self.terminal_manager.is_terminal(pane_id) {
                        if let Some(ws) = self.egui_winit.as_ref() {
                            let ctx = ws.egui_ctx();
                            let metrics = CellMetrics::from_egui(ctx, 14.0);
                            let cols = (w as f32 / metrics.cell_width).max(2.0) as u16;
                            let rows = (h as f32 / metrics.cell_height).max(1.0) as u16;
                            self.terminal_manager.resize(pane_id, cols, rows);
                        }
                    } else {
                        if w > 0 && h > 0 {
                            self.offscreen_panes.resize(pane_id, w, h);
                        }
                    }
                }
            }
            #[cfg(not(feature = "terminal"))]
            {
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

    /// Run one frame of egui UI + wgpu rendering.
    fn render(&mut self) {
        if let Some(app_state) = &mut self.app_state {
            app_state.profiler.start_frame();
        }

        let window = match &self.window {
            Some(w) => w,
            None => return,
        };
        let winit_state = match &mut self.egui_winit {
            Some(s) => s,
            None => return,
        };
        let gfx = match &mut self.gfx {
            Some(g) => g,
            None => return,
        };
        let app_state = match &mut self.app_state {
            Some(s) => s,
            None => return,
        };

        // 1. Take accumulated input from egui_winit
        let raw_input = winit_state.take_egui_input(window);

        // 2. Run egui logic — build the UI
        let full_output = winit_state.egui_ctx().run(raw_input, |egui_ctx| {
            panels::build_ui(
                egui_ctx,
                app_state,
                &self.wry_panes,
                &self.git_status,
                STATUS_BAR_HEIGHT,
                &self.webview_textures,
                #[cfg(feature = "terminal")]
                &mut self.terminal_manager,
                &self.offscreen_panes,
            );
        });

        // 3. Handle platform output
        winit_state.handle_platform_output(window, full_output.platform_output);

        // 4. Get tessellated paint jobs
        let egui_ctx = winit_state.egui_ctx();
        let paint_jobs = egui_ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        let textures_delta = &full_output.textures_delta;

        // 5. Build screen descriptor
        let screen_descriptor = gfx.screen_descriptor(window);

        // 6. Update egui textures and buffers
        for (id, image_delta) in &textures_delta.set {
            gfx.egui_renderer
                .update_texture(&gfx.device, &gfx.queue, *id, image_delta);
        }

        let mut encoder = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("egui-encoder"),
            });

        let user_cmd_bufs = gfx.egui_renderer.update_buffers(
            &gfx.device,
            &gfx.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        // 7. Get the surface texture
        let output = match gfx.surface.get_current_texture() {
            Ok(tex) => tex,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                let size = window.inner_size();
                gfx.resize(size.width, size.height);
                return;
            }
            Err(e) => {
                warn!("Surface error (skipping frame): {:?}", e);
                return;
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // 8. Begin render pass
        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui-main-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            let mut render_pass = render_pass.forget_lifetime();
            gfx.egui_renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
            // render_pass is dropped here, ending the pass
        }

        // 9. Submit
        gfx.queue.submit(
            user_cmd_bufs
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );

        // 10. Free old textures
        for id in &textures_delta.free {
            gfx.egui_renderer.free_texture(id);
        }

        // 11. Present
        output.present();

        if let Some(app_state) = &mut self.app_state {
            app_state.profiler.end_frame("render");
        }
    }

    /// Capture dirty offscreen frames and update egui textures.
    ///
    /// For each offscreen pane that has changed since last capture:
    /// 1. Call capture_frame() to read pixels from the offscreen GTK buffer
    /// 2. Convert BGRA→RGBA
    /// 3. Create or update an egui TextureId
    ///
    /// Returns true if any texture was updated (caller should request repaint).
    #[cfg(target_os = "linux")]
    fn update_webview_textures(&mut self) -> bool {
        if self.offscreen_panes.is_empty() {
            return false;
        }

        let capture_interval = self.adaptive_quality.capture_interval_ms();
        let bg_capture_interval = self.adaptive_quality.background_capture_interval_ms();
        let skip_non_active = self.adaptive_quality.should_skip_non_active();
        let active_id = self.app_state.as_ref().map(|s| s.wm.active_pane_id());

        // Collect IDs of panes that need capture (avoid holding mutable borrows across texture updates).
        let mut captured: Vec<(uuid::Uuid, u32, u32)> = Vec::new();

        for (id, pane) in self.offscreen_panes.iter_mut() {
            // When quality is very low, skip non-active panes entirely
            if skip_non_active && active_id.is_some_and(|aid| aid != *id) {
                continue;
            }

            // Background panes use a much lower capture rate
            let is_active = active_id.is_some_and(|aid| aid == *id);
            let interval_ms = if is_active {
                capture_interval
            } else {
                bg_capture_interval
            };

            let last = self
                .offscreen_last_capture
                .get(id)
                .copied()
                .unwrap_or_else(|| std::time::Instant::now() - std::time::Duration::from_secs(10));
            let dirty = pane.is_dirty();
            let elapsed = last.elapsed();
            if dirty && elapsed >= std::time::Duration::from_millis(interval_ms as u64) {
                tracing::debug!(
                    "capture: pane {} dirty={} elapsed={:?} active={} interval={}ms",
                    &id.to_string()[..8],
                    dirty,
                    elapsed,
                    is_active,
                    interval_ms,
                );
                if pane.capture_frame().is_some()
                    && let Some(frame) = pane.frame()
                {
                    let fw = frame.width;
                    let fh = frame.height;
                    let needed = (fw as usize) * (fh as usize) * 4;
                    // Reuse existing buffer; only reallocate when pane size grows.
                    let buf = self
                        .capture_buffers
                        .entry(*id)
                        .or_insert_with(|| Vec::with_capacity(needed));
                    if buf.len() < needed {
                        buf.resize(needed, 0);
                    } else {
                        buf[..needed].fill(0);
                    }
                    if let Some(rgba) = pane.frame_rgba() {
                        let copy_len = rgba.len().min(needed);
                        buf[..copy_len].copy_from_slice(&rgba[..copy_len]);
                    }
                    captured.push((*id, fw, fh));
                    // Only reset the capture timer when a frame was actually produced.
                    self.offscreen_last_capture
                        .insert(*id, std::time::Instant::now());
                }
            }
        }

        let mut updated = false;
        for (pane_id, width, height) in captured {
            let rgba = self.capture_buffers.get(&pane_id);
            let Some(rgba) = rgba else {
                continue;
            };
            let expected = (width as usize) * (height as usize) * 4;
            if rgba.len() != expected {
                // Buffer size mismatch can occur during window resize when
                // the offscreen webview is resized between capture and upload.
                // Skip this frame rather than panicking in epaint.
                tracing::warn!(
                    "Texture upload skipped for pane {}: buffer {} != expected {} ({}x{})",
                    &pane_id.to_string()[..8],
                    rgba.len(),
                    expected,
                    width,
                    height,
                );
                continue;
            }
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], rgba);

            if let Some(ws) = self.egui_winit.as_ref() {
                let ctx = ws.egui_ctx();

                if let Some(handle) = self.webview_texture_handles.get_mut(&pane_id) {
                    if handle.size() == [width as usize, height as usize] {
                        handle.set(color_image, egui::TextureOptions::LINEAR);
                    } else {
                        let new_handle = ctx.load_texture(
                            format!("webview-{pane_id}"),
                            color_image,
                            egui::TextureOptions::LINEAR,
                        );
                        self.webview_textures.insert(pane_id, new_handle.id());
                        self.webview_texture_handles.insert(pane_id, new_handle);
                    }
                } else {
                    let handle = ctx.load_texture(
                        format!("webview-{pane_id}"),
                        color_image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.webview_textures.insert(pane_id, handle.id());
                    self.webview_texture_handles.insert(pane_id, handle);
                }
            }
            updated = true;
        }
        updated
    }
}

fn main() -> anyhow::Result<()> {
    bootstrap::run()
}

#[cfg(test)]
mod tests {
    /// Simulate one step of drain_pending_pane_creates logic.
    /// Returns (created_count, remaining_count) and modifies the queue in-place.
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
