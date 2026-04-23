use std::sync::Arc;
use tracing::{info, warn};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use aileron::app::AppState;
use aileron::config::Config;
use aileron::gfx::GfxState;
use aileron::input::{KeyEvent as AileronKeyEvent, Modifiers};
use aileron::mcp::McpBridge;
use aileron::net::adblock::AdBlocker;
use aileron::offscreen_webview::OffscreenWebViewManager;
use aileron::popup::PopupManager;
use aileron::profiling::AdaptiveQuality;
use aileron::servo::{bsp_rect_to_wry_rect, init_gtk, WryPaneManager};
use aileron::terminal::NativeTerminalManager;
use aileron::ui::panels;
use aileron::wm::Rect;

mod bootstrap;
mod frame_tasks;

/// Custom X11 error handler that swallows GLXBadWindow errors.
///
/// wry's build_as_child creates X11 child windows that winit doesn't own.
/// When winit's IME tries to unfocus these windows, it triggers GLXBadWindow
/// (error code 170) which winit's event processor .expect()s on, crashing the app.
/// This handler intercepts GLXBadWindow and returns 0 (ignored) to prevent the crash.
#[cfg(target_os = "linux")]
unsafe extern "C" fn x11_error_handler(
    _display: *mut x11_dl::xlib::Display,
    event: *mut x11_dl::xlib::XErrorEvent,
) -> std::os::raw::c_int {
    // SAFETY: This is an X11 error handler callback. We only read the error_code
    // field from the XErrorEvent struct, which is always valid when called by Xlib.
    if !event.is_null() {
        let error = unsafe { &*event };
        if error.error_code == 170 {
            // Swallow GLXBadWindow — this is from wry's child windows
            return 0;
        }
    }
    1 // Return non-zero for unhandled errors (triggers Xlib's default handler)
}

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
    mcp_bridge: McpBridge,

    /// Terminal manager for embedded terminal panes.
    terminal_manager: NativeTerminalManager,

    content_scripts: aileron::scripts::ContentScriptManager,

    /// Current git status for the working directory.
    git_status: aileron::git::GitStatus,

    /// Last time git status was polled (throttled to 1 Hz).
    last_git_poll: std::time::Instant,

    /// Standalone popup browser windows (no egui overlay, no tiling).
    popup: PopupManager,

    /// Tracks whether the first frame has rendered (for startup timing).
    first_frame: bool,

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

    /// Deferred pane creation queue (TASK-K27).
    /// Background panes are queued here and created one-per-frame in
    /// `about_to_wait()` to prevent startup freeze with many tabs.
    pending_pane_creates: std::collections::VecDeque<(uuid::Uuid, url::Url)>,

    /// Adaptive quality renderer (TASK-K24).
    /// Reduces texture capture rate when frames are slow.
    adaptive_quality: AdaptiveQuality,

    /// Last time adblock filter lists were updated (for periodic refresh).
    last_filter_update: std::time::Instant,
}

impl AileronApp {
    fn new() -> Self {
        let config = Config::load();

        // Set proxy environment variable if configured
        if let Some(ref proxy) = config.proxy {
            unsafe { std::env::set_var("all_proxy", proxy) };
            info!("Proxy configured: {}", proxy);
        }

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
            mcp_bridge,
            terminal_manager: NativeTerminalManager::new(),
            content_scripts: aileron::scripts::ContentScriptManager::new(),
            git_status: aileron::git::GitStatus::default(),
            last_git_poll: std::time::Instant::now(),
            popup: PopupManager::new(),
            first_frame: true,
            frame_count: 0,
            startup_start: std::time::Instant::now(),
            offscreen_panes: OffscreenWebViewManager::new(),
            offscreen_mouse_pressed: false,
            webview_textures: std::collections::HashMap::new(),
            webview_texture_handles: std::collections::HashMap::new(),
            offscreen_last_capture: std::collections::HashMap::new(),
            pending_pane_creates: std::collections::VecDeque::new(),
            adaptive_quality,
            last_filter_update: std::time::Instant::now(),
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

        info!("init_graphics(): GPU initialized, max texture: {}px", gfx.device.limits().max_texture_dimension_2d);
        winit_state.set_max_texture_side(gfx.device.limits().max_texture_dimension_2d as usize);

        // Initialize app state with viewport and config
        info!("init_graphics(): Creating AppState...");
        let size = window.inner_size();
        let viewport = Rect::new(0.0, 0.0, size.width as f64, size.height as f64);
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

        let loaded_count = app_state
            .extension_manager
            .lock()
            .map(|mut m| m.load_all().len())
            .unwrap_or(0);
        if loaded_count > 0 {
            info!("Loaded {} extension(s)", loaded_count);
        }

        if let Ok(mgr) = app_state.extension_manager.lock() {
            self.content_scripts.set_extension_registry(
                mgr.content_script_registry().clone()
            );
        }

        self.egui_winit = Some(winit_state);
        self.gfx = Some(gfx);
        self.app_state = Some(app_state);
        self.window = Some(window);
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

        let is_terminal = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            app_state.terminal_pane_ids.contains(&pane_id)
        };

        // Get the BSP rect for this pane
        let wm_rect = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            let panes = app_state.wm.panes();
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

        match self.wry_panes.create_pane(
            &*window,
            pane_id,
            url.clone(),
            wry_rect,
            blocked_domains,
            self.config.devtools,
        ) {
            Ok(()) => {
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
                    app_state.status_message = format!("Pane creation failed: {}", e);
                }
            }
        }
    }

    /// Create an offscreen webview pane for Architecture B rendering.
    fn create_offscreen_pane_for(&mut self, pane_id: uuid::Uuid, url: &url::Url) {
        let is_terminal = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            app_state.terminal_pane_ids.contains(&pane_id)
        };

        let wm_rect = {
            let app_state = match &self.app_state {
                Some(s) => s,
                None => return,
            };
            let panes = app_state.wm.panes();
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

        #[cfg(target_os = "linux")]
        match self.offscreen_panes.create_pane(
            pane_id, url, width, height, blocked_domains, self.config.devtools
        ) {
            Ok(()) => {
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
                    app_state.status_message = format!("Pane creation failed: {}", e);
                }
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (pane_id, url, width, height, blocked_domains);
            warn!("Offscreen webview not supported on this platform");
        }
    }

    /// Remove a wry pane when a BSP leaf is closed.
    fn remove_wry_pane_for(&mut self, pane_id: &uuid::Uuid) {
        self.terminal_manager.remove(pane_id);
        self.wry_panes.remove_pane(pane_id);
        self.offscreen_panes.remove_pane(pane_id);
        self.webview_textures.remove(pane_id);
        self.webview_texture_handles.remove(pane_id);
        self.offscreen_last_capture.remove(pane_id);
        self.pending_pane_creates
            .retain(|(id, _)| id != pane_id);
    }

    /// Create a wry webview for a standalone popup window.
    fn init_popup_window(&mut self, window_id: WindowId, window: Arc<Window>) {
        let url = self
            .app_state
            .as_mut()
            .and_then(|s| s.pending_detach_url.take())
            .unwrap_or_else(|| url::Url::parse("aileron://new").unwrap());
        let blocked_domains: Vec<String> = self.adblocker.blocked_domains_iter();

        self.popup.init_popup_window(
            window_id,
            window,
            url,
            blocked_domains,
            self.config.devtools,
        );
    }

    /// Create at most one deferred offscreen pane per frame (TASK-K27).
    /// When the active pane switches to a deferred pane, creates it immediately.
    fn drain_pending_pane_creates(&mut self) {
        if self.pending_pane_creates.is_empty() {
            return;
        }

        let active_id = self
            .app_state
            .as_ref()
            .map(|s| s.wm.active_pane_id());

        let current_pane_ids: std::collections::HashSet<uuid::Uuid> = self
            .app_state
            .as_ref()
            .map(|s| s.wm.panes().iter().map(|(id, _)| *id).collect())
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
            let (pid, url) = self.pending_pane_creates.remove(idx).unwrap();
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

        let panes = app_state.wm.panes();
        for (pane_id, wm_rect) in &panes {
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
            // Import CellMetrics for terminal auto-resize
            use aileron::terminal::grid::CellMetrics;

            for (pane_id, wm_rect) in &panes {
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
                    self.offscreen_panes.resize(pane_id, w, h);
                }
            }
        }
    }

    /// Run one frame of egui UI + wgpu rendering.
    fn render(&mut self) {
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
                &self.terminal_manager,
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
        let skip_non_active = self.adaptive_quality.should_skip_non_active();
        let active_id = self
            .app_state
            .as_ref()
            .map(|s| s.wm.active_pane_id());

        let mut dirty_data: Vec<(uuid::Uuid, Vec<u8>, u32, u32)> = Vec::new();

        for (id, pane) in self.offscreen_panes.iter_mut() {
            if skip_non_active && active_id.is_some_and(|aid| aid != *id) {
                continue;
            }

            let last = self
                .offscreen_last_capture
                .get(id)
                .copied()
                .unwrap_or_else(|| std::time::Instant::now() - std::time::Duration::from_secs(10));
            let dirty = pane.is_dirty();
            let elapsed = last.elapsed();
            if dirty && elapsed >= std::time::Duration::from_millis(capture_interval as u64) {
                tracing::debug!(
                    "capture: pane {} dirty={} elapsed={:?}",
                    &id.to_string()[..8], dirty, elapsed,
                );
                if pane.capture_frame().is_some()
                    && let Some(rgba) = pane.frame_rgba()
                {
                    let (w, h) = pane.dimensions();
                    dirty_data.push((*id, rgba, w as u32, h as u32));
                }
                self.offscreen_last_capture.insert(*id, std::time::Instant::now());
            }
        }

        let mut updated = false;
        for (pane_id, rgba, width, height) in dirty_data {
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba);

            if let Some(ws) = self.egui_winit.as_ref() {
                let ctx = ws.egui_ctx();

                if let Some(handle) = self.webview_texture_handles.get_mut(&pane_id) {
                    if handle.size() == [width as usize, height as usize] {
                        handle.set(color_image, egui::TextureOptions::LINEAR);
                    } else {
                        let new_handle = ctx.load_texture(
                            format!("webview-{}", pane_id),
                            color_image,
                            egui::TextureOptions::LINEAR,
                        );
                        self.webview_textures.insert(pane_id, new_handle.id());
                        self.webview_texture_handles.insert(pane_id, new_handle);
                    }
                } else {
                    let handle = ctx.load_texture(
                        format!("webview-{}", pane_id),
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

impl ApplicationHandler for AileronApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        info!("── resumed(): Creating window ──");
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("Aileron")
                        .with_inner_size(winit::dpi::LogicalSize::new(
                            self.config.window_width,
                            self.config.window_height,
                        )),
                )
                .expect("Failed to create window"),
        );

        info!(
            "Window created: {}x{}",
            window.inner_size().width,
            window.inner_size().height
        );
        self.init_graphics(window);

        // Create initial wry pane for the root BSP leaf
        if let Some(app_state) = &self.app_state {
            let root_pane_id = app_state.wm.active_pane_id();
            let root_url = app_state
                .engines
                .get(&root_pane_id)
                .and_then(|e| e.current_url().cloned())
                .unwrap_or_else(|| url::Url::parse("aileron://welcome").unwrap());
            self.create_wry_pane_for(root_pane_id, &root_url);
        }

        // Auto-restore workspace based on session state.
        // Prefer _autosave (crash recovery) if previous session was unclean.
        if let Some(app_state) = &mut self.app_state {
            let was_unclean = Config::was_previous_session_unclean();

            if (!self.config.restore_session || !was_unclean)
                && let Some(db) = app_state.db.as_ref()
            {
                let _ = aileron::db::workspaces::delete_workspace(db, "_autosave");
            }

            if self.config.restore_session {
                let all_workspaces = app_state
                    .db
                    .as_ref()
                    .and_then(|conn| aileron::db::workspaces::list_workspaces(conn).ok())
                    .unwrap_or_default();

                let to_restore = if was_unclean {
                    all_workspaces
                        .iter()
                        .find(|ws| ws.name == "_autosave")
                        .cloned()
                } else {
                    all_workspaces
                        .iter()
                        .find(|ws| ws.name != "_autosave")
                        .cloned()
                };

                if let Some(workspace) = to_restore {
                    info!("Auto-restoring workspace: {}", workspace.name);
                    app_state.pending_workspace_restore = Some(workspace.name);
                    app_state.session_dirty = true;
                }
            }
        }

        frame_tasks::load_default_adblock_rules(&mut self.adblocker);

        frame_tasks::spawn_mcp_server(&self.mcp_bridge);

        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Route popup window events
        if self.popup.contains_key(&window_id) {
            self.handle_popup_event(window_id, &event);
            return;
        }

        // Handle initialization event for a newly created popup window
        if let Some((popup_id, popup_window)) = self.popup.pending_popup_window.take() {
            if window_id == popup_id {
                self.init_popup_window(popup_id, popup_window);
                return;
            }
            self.popup.pending_popup_window = Some((popup_id, popup_window));
        }

        let window = match &self.window {
            Some(w) => Arc::clone(w),
            None => return,
        };
        let winit_state = match &mut self.egui_winit {
            Some(s) => s,
            None => return,
        };

        // Feed event to egui first
        let egui_response = winit_state.on_window_event(&window, &event);

        // Track modifiers
        if let WindowEvent::ModifiersChanged(state) = &event {
            let ms = state.state();
            self.modifiers = Modifiers {
                ctrl: ms.control_key(),
                alt: ms.alt_key(),
                shift: ms.shift_key(),
                super_key: ms.super_key(),
            };
        }

        // Handle resize
        if let Some(app_state) = &mut self.app_state
            && let WindowEvent::Resized(physical_size) = &event
            && physical_size.width > 0 && physical_size.height > 0
        {
            app_state.wm.resize(Rect::new(
                0.0,
                0.0,
                physical_size.width as f64,
                physical_size.height as f64,
            ));
            self.reposition_all_panes();
        }

        // Handle events
        match &event {
            WindowEvent::CloseRequested => {
                info!("Close requested — quitting");
                event_loop.exit();
            }

            WindowEvent::RedrawRequested => {
                let frame_start = std::time::Instant::now();
                self.frame_count += 1;
                if self.frame_count <= 3 || self.frame_count.is_multiple_of(300) {
                    info!("Render frame #{}", self.frame_count);
                }
                self.render();
                let frame_time = frame_start.elapsed();
                let frame_time_ms = frame_time.as_secs_f64() * 1000.0;
                if frame_time.as_millis() > 17 {
                    tracing::debug!("Frame over budget: {:.1}ms", frame_time_ms);
                }
                self.adaptive_quality.update(frame_time_ms);
            }

            WindowEvent::Resized(physical_size) => {
                if let Some(gfx) = &self.gfx {
                    gfx.resize(physical_size.width, physical_size.height);
                }
            }

            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key,
                        logical_key,
                        state: winit::event::ElementState::Pressed,
                        repeat,
                        ..
                    },
                ..
            } => {
                if *repeat
                    && let Some(app_state) = &self.app_state
                {
                    let active_id = app_state.wm.active_pane_id();
                    if !self.terminal_manager.is_terminal(&active_id) {
                        return;
                    }
                }

                // Let egui consume the event first
                if egui_response.consumed {
                    return;
                }

                let key = aileron::input::map_key(*physical_key, logical_key);

                // Route key event through app state (mode machine + keybindings)
                let mods = self.modifiers;
                let aileron_event = AileronKeyEvent {
                    key: key.clone(),
                    modifiers: mods,
                    physical_key: None,
                };

                if let Some(app_state) = &mut self.app_state {
                    // ─── Hint mode: intercept letter keys to follow hinted links ───
                    if app_state.hint_mode {
                        match &key {
                            aileron::input::Key::Character(c) if c.is_ascii_lowercase() => {
                                app_state.hint_buffer.push(*c);
                                let hint_buf = app_state.hint_buffer.clone();
                                // Use IPC to get click feedback for auto-exit
                                let js = format!(
                                    "(function() {{ \
                                        var el = document.querySelector('[data-aileron-hint=\"{}\"]'); \
                                        if (el) {{ el.click(); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                                        var all = document.querySelectorAll('[data-aileron-hint]'); \
                                        var matches = []; \
                                        all.forEach(function(e) {{ \
                                            if (e.getAttribute('data-aileron-hint').startsWith('{}')) matches.push(e); \
                                        }}); \
                                        if (matches.length === 1) {{ matches[0].click(); window.ipc.postMessage(JSON.stringify({{t:'hint-clicked'}})); return; }} \
                                    }})()",
                                    hint_buf, hint_buf
                                );
                                let active_id = app_state.wm.active_pane_id();
                                if let Some(wry_pane) = self.wry_panes.get(&active_id) {
                                    wry_pane.execute_js(&js);
                                } else if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                                    pane.execute_js(&js);
                                    pane.mark_dirty();
                                }
                                return;
                            }
                            _ => {
                                // Any non-letter key exits hint mode
                                let active_id = app_state.wm.active_pane_id();
                                app_state.hint_mode = false;
                                app_state.hint_buffer.clear();
                                app_state.status_message.clear();
                                let clear_js = r#"
                                    (function() {
                                        var style = document.getElementById('__aileron_hints');
                                        if (style) style.remove();
                                        document.querySelectorAll('[data-aileron-hint]').forEach(el => {
                                            el.removeAttribute('data-aileron-hint');
                                        });
                                    })();
                                "#;
                                if let Some(wry_pane) = self.wry_panes.get(&active_id) {
                                    wry_pane.execute_js(clear_js);
                                } else if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                                    pane.execute_js(clear_js);
                                    pane.mark_dirty();
                                }
                                return;
                            }
                        }
                    }

                    // Escape closes find bar first, then URL bar, then normal key processing
                    if app_state.find_bar_open && key == aileron::input::Key::Escape {
                        app_state.find_bar_open = false;
                        app_state.find_query.clear();
                        let active_id = app_state.wm.active_pane_id();
                        if let Some(wry_pane) = self.wry_panes.get(&active_id) {
                            wry_pane.execute_js("window.getSelection().removeAllRanges()");
                        }
                        return;
                    }
                    if app_state.url_bar_focused && key == aileron::input::Key::Escape {
                        app_state.url_bar_focused = false;
                        app_state.url_bar_input.clear();
                        return;
                    }
                    // Track pane count before processing key
                    let pane_ids_before: std::collections::HashSet<uuid::Uuid> = app_state
                        .wm
                        .panes()
                        .iter()
                        .map(|(id, _)| *id)
                        .collect();

                    app_state.process_key_event(aileron_event);

                    let pane_ids_after: std::collections::HashSet<uuid::Uuid> = app_state
                        .wm
                        .panes()
                        .iter()
                        .map(|(id, _)| *id)
                        .collect();

                    let closed_pane_ids: Vec<uuid::Uuid> = pane_ids_before
                        .difference(&pane_ids_after)
                        .copied()
                        .collect();

                    let new_pane_ids: Vec<uuid::Uuid> = pane_ids_after
                        .difference(&pane_ids_before)
                        .copied()
                        .collect();

                    let need_reposition = pane_ids_before.len() != pane_ids_after.len();
                    let active_pane_id = app_state.wm.active_pane_id();
                    let is_insert_mode = app_state.mode == aileron::input::Mode::Insert;

                    // Now sync wry panes (drop borrow on app_state first)
                    for pid in &new_pane_ids {
                        let new_url = url::Url::parse("aileron://new").unwrap();
                        if *pid == active_pane_id || !self.config.is_offscreen() {
                            self.create_wry_pane_for(*pid, &new_url);
                        } else {
                            self.pending_pane_creates.push_back((*pid, new_url));
                        }
                    }

                    for pid in &closed_pane_ids {
                        self.remove_wry_pane_for(pid);
                    }

                    if need_reposition {
                        self.reposition_all_panes();
                    }

                    // Handle Insert mode: focus the wry webview (native mode only)
                    if is_insert_mode
                        && !self.config.is_offscreen()
                        && let Some(wry_pane) = self.wry_panes.get(&active_pane_id)
                    {
                        wry_pane.focus();
                    }

                    // Offscreen mode: forward keyboard to webview via JS or native terminal
                    if is_insert_mode && self.config.is_offscreen() {
                        let is_terminal = self.terminal_manager.is_terminal(&active_pane_id);

                        if is_terminal {
                            // Native terminal: write directly to PTY
                            if let aileron::input::Key::Character(c) = &key {
                                self.terminal_manager.write_input(&active_pane_id, &c.to_string());
                            } else {
                                // Convert special keys to escape sequences
                                let escape_seq = key_to_escape_sequence(&key, mods);
                                if !escape_seq.is_empty() {
                                    self.terminal_manager.write_input(&active_pane_id, &escape_seq);
                                }
                            }
                        } else if let Some(pane) = self.offscreen_panes.get_mut(&active_pane_id) {
                            // Web content: forward via JS
                            if let aileron::input::Key::Character(c) = &key {
                                pane.insert_text(&c.to_string());
                            } else {
                                let (js_key, js_code) = key_to_js(&key);
                                let mods = aileron::offscreen_webview::modifiers_js(
                                    mods.ctrl, mods.alt, mods.shift, mods.super_key,
                                );
                                pane.forward_key_event("keydown", &js_key, &js_code, &mods);
                            }
                        }
                    }
                }
            }

            WindowEvent::DroppedFile(path) => {
                info!("File dropped: {:?}", path);
            }

            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(app_state) = &self.app_state
                    && app_state.mode == aileron::input::Mode::Insert
                {
                    let active_id = app_state.wm.active_pane_id();
                    let (dx, dy) = match delta {
                        winit::event::MouseScrollDelta::LineDelta(x, y) => {
                            (*x as f64 * 40.0, *y as f64 * 40.0)
                        }
                        winit::event::MouseScrollDelta::PixelDelta(pos) => {
                            (pos.x, pos.y)
                        }
                    };
                    if dx.abs() > 0.1 || dy.abs() > 0.1 {
                        if self.terminal_manager.is_terminal(&active_id) {
                            // Native terminal: scroll scrollback buffer
                            // Positive dy = scroll down (toward bottom), negative = scroll up
                            let lines = (dy / 40.0).round() as i32;
                            if lines != 0 {
                                self.terminal_manager.scroll(&active_id, -lines);
                            }
                        } else if !self.config.is_offscreen() {
                            if let Some(wry_pane) = self.wry_panes.get(&active_id) {
                                let js = format!("window.scrollBy({}, {})", dx, dy);
                                wry_pane.execute_js(&js);
                            }
                        } else if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                            pane.scroll_by(dx, dy);
                        }
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                if !egui_response.consumed
                    && self.config.is_offscreen()
                    && let Some(app_state) = &self.app_state
                    && app_state.mode == aileron::input::Mode::Insert
                {
                    self.offscreen_mouse_pressed = *state == winit::event::ElementState::Pressed;

                    let active_id = app_state.wm.active_pane_id();

                    if self.terminal_manager.is_terminal(&active_id) {
                        let terminal_info = (|| {
                            let ws = self.egui_winit.as_ref()?;
                            let ctx = ws.egui_ctx();
                            let pos = ctx.pointer_latest_pos()?;
                            let panes = app_state.wm.panes();
                            let (_, rect) = panes.iter().find(|(id, _)| *id == active_id)?;
                            let top_offset = URL_BAR_HEIGHT as f32;
                            let sidebar_offset =
                                if app_state.config.tab_layout == "sidebar"
                                    && !app_state.config.tab_sidebar_right
                                {
                                    app_state.config.tab_sidebar_width
                                } else {
                                    0.0
                                };
                            let local_x = pos.x - rect.x as f32 - sidebar_offset;
                            let local_y = pos.y - rect.y as f32 - top_offset;
                            if local_x >= 0.0 && local_y >= 0.0 {
                                Some((local_x, local_y))
                            } else {
                                None
                            }
                        })();

                        if let Some((local_x, local_y)) = terminal_info {
                            use aileron::terminal::grid::CellMetrics;
                            if let Some(ws) = self.egui_winit.as_ref() {
                                let metrics = CellMetrics::from_egui(ws.egui_ctx(), 14.0);
                                if let Some(pane) = self.terminal_manager.get_mut(&active_id) {
                                    let (line, col) = pane.pixel_to_grid(
                                        local_x,
                                        local_y,
                                        metrics.cell_width,
                                        metrics.cell_height,
                                    );
                                    match (state, button) {
                                        (winit::event::ElementState::Pressed, winit::event::MouseButton::Left) => {
                                            pane.start_selection(line, col);
                                        }
                                        (winit::event::ElementState::Released, winit::event::MouseButton::Left) => {
                                            pane.end_selection();
                                            if let Some(text) = pane.selection_text() {
                                                ws.egui_ctx()
                                            .copy_text(text);
                                            }
                                        }
                                        (winit::event::ElementState::Pressed, winit::event::MouseButton::Right) => {
                                            pane.clear_selection();
                                        }
                                        (winit::event::ElementState::Pressed, winit::event::MouseButton::Middle) => {
                                            if let Some(pane_ref) = self.terminal_manager.get(&active_id)
                                                && let Some(text) = pane_ref.selection_text()
                                            {
                                                self.terminal_manager.write_input(&active_id, &text);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    } else {
                        let forward_info = (|| {
                            let ws = self.egui_winit.as_ref()?;
                            let ctx = ws.egui_ctx();
                            let pos = ctx.pointer_latest_pos()?;
                            let panes = app_state.wm.panes();
                            let (_, rect) = panes.iter().find(|(id, _)| *id == active_id)?;
                            let (pw, ph) = self.offscreen_panes.get(&active_id)?.dimensions();
                            let top_offset = URL_BAR_HEIGHT as f32;
                            let sidebar_offset =
                                if app_state.config.tab_layout == "sidebar"
                                    && !app_state.config.tab_sidebar_right
                                {
                                    app_state.config.tab_sidebar_width
                                } else {
                                    0.0
                                };
                            let local_x = pos.x - rect.x as f32 - sidebar_offset;
                            let local_y = pos.y - rect.y as f32 - top_offset;
                            if local_x >= 0.0 && local_y >= 0.0 && local_x < pw as f32 && local_y < ph as f32 {
                                let event_type = match state {
                                    winit::event::ElementState::Pressed => "mousedown",
                                    winit::event::ElementState::Released => "mouseup",
                                };
                                let btn = match button {
                                    winit::event::MouseButton::Left => "0",
                                    winit::event::MouseButton::Middle => "1",
                                    winit::event::MouseButton::Right => "2",
                                    winit::event::MouseButton::Back => "3",
                                    winit::event::MouseButton::Forward => "4",
                                    _ => "0",
                                };
                                Some((event_type, local_x as f64, local_y as f64, btn))
                            } else {
                                None
                            }
                        })();

                        if let Some((event_type, local_x, local_y, btn)) = forward_info {
                            if *button == winit::event::MouseButton::Middle
                                && *state == winit::event::ElementState::Pressed
                            {
                                let js = format!(
                                    r#"(function() {{
                                        var el = document.elementFromPoint({}, {});
                                        while (el && el.tagName !== 'A') {{ el = el.parentElement; }}
                                        if (el && el.href) {{
                                            window.open(el.href, '_blank');
                                        }}
                                    }})();"#,
                                    local_x, local_y
                                );
                                if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                                    pane.execute_js(&js);
                                }
                            } else {
                                let mods = aileron::offscreen_webview::modifiers_js(
                                    self.modifiers.ctrl,
                                    self.modifiers.alt,
                                    self.modifiers.shift,
                                    self.modifiers.super_key,
                                );
                                if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                                    pane.forward_mouse_event(event_type, local_x, local_y, btn, &mods);
                                }
                            }
                        }
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                if !egui_response.consumed
                    && self.config.is_offscreen()
                {
                    let scale = window.scale_factor() as f32;
                    let logical_pos = egui::pos2(position.x as f32 / scale, position.y as f32 / scale);

                    if let Some(app_state) = &self.app_state
                        && app_state.mode == aileron::input::Mode::Insert
                    {
                        let active_id = app_state.wm.active_pane_id();

                        if self.terminal_manager.is_terminal(&active_id) {
                            let terminal_info = (|| {
                                let panes = app_state.wm.panes();
                                let (_, rect) = panes.iter().find(|(id, _)| *id == active_id)?;
                                let top_offset = URL_BAR_HEIGHT as f32;
                                let sidebar_offset =
                                    if app_state.config.tab_layout == "sidebar"
                                        && !app_state.config.tab_sidebar_right
                                    {
                                        app_state.config.tab_sidebar_width
                                    } else {
                                        0.0
                                    };
                                let local_x = logical_pos.x - rect.x as f32 - sidebar_offset;
                                let local_y = logical_pos.y - rect.y as f32 - top_offset;
                                if local_x >= 0.0 && local_y >= 0.0 {
                                    Some((local_x, local_y))
                                } else {
                                    None
                                }
                            })();

                            if let Some((local_x, local_y)) = terminal_info {
                                use aileron::terminal::grid::CellMetrics;
                                if let Some(ws) = self.egui_winit.as_ref()
                                    && let Some(pane) = self.terminal_manager.get_mut(&active_id)
                                    && pane.is_selecting()
                                {
                                    let metrics = CellMetrics::from_egui(ws.egui_ctx(), 14.0);
                                    let (line, col) = pane.pixel_to_grid(
                                        local_x,
                                        local_y,
                                        metrics.cell_width,
                                        metrics.cell_height,
                                    );
                                    pane.extend_selection(line, col);
                                }
                            }
                        } else {
                            let forward_info = (|| {
                                let panes = app_state.wm.panes();
                                let (_, rect) = panes.iter().find(|(id, _)| *id == active_id)?;
                                let (pw, ph) = self.offscreen_panes.get(&active_id)?.dimensions();
                                let top_offset = URL_BAR_HEIGHT as f32;
                                let sidebar_offset =
                                    if app_state.config.tab_layout == "sidebar"
                                        && !app_state.config.tab_sidebar_right
                                    {
                                        app_state.config.tab_sidebar_width
                                    } else {
                                        0.0
                                    };
                                let local_x = logical_pos.x - rect.x as f32 - sidebar_offset;
                                let local_y = logical_pos.y - rect.y as f32 - top_offset;
                                if local_x >= 0.0 && local_y >= 0.0 && local_x < pw as f32 && local_y < ph as f32 {
                                    Some((local_x as f64, local_y as f64))
                                } else {
                                    None
                                }
                            })();

                            if let Some((local_x, local_y)) = forward_info
                                && self.offscreen_mouse_pressed
                            {
                                let mods = aileron::offscreen_webview::modifiers_js(
                                    self.modifiers.ctrl,
                                    self.modifiers.alt,
                                    self.modifiers.shift,
                                    self.modifiers.super_key,
                                );
                                if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                                    pane.forward_mouse_event("mousemove", local_x, local_y, "0", &mods);
                                }
                            }
                        }
                    }
                }
            }

            WindowEvent::Ime(ime) => {
                if self.config.is_offscreen()
                    && let Some(app_state) = &self.app_state
                    && app_state.mode == aileron::input::Mode::Insert
                {
                    match ime {
                        winit::event::Ime::Commit(text) => {
                            let active_id = app_state.wm.active_pane_id();
                            let text_owned = text.clone();

                            // Route IME commit to native terminal or webview
                            if self.terminal_manager.is_terminal(&active_id) {
                                self.terminal_manager.write_input(&active_id, &text_owned);
                            } else if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                                pane.insert_text(&text_owned);
                            }
                        }
                        winit::event::Ime::Preedit(text, _cursor) => {
                            if text.is_empty() {
                                if let Some(app_state) = &mut self.app_state
                                    && app_state.status_message.starts_with("composing: ")
                                {
                                    app_state.status_message.clear();
                                }
                            } else if let Some(app_state) = &mut self.app_state {
                                app_state.status_message =
                                    format!("composing: {}", text);
                            }
                        }
                        _ => {}
                    }
                }
            }

            _ => {}
        }

        // Check if app wants to quit
        if let Some(app_state) = &self.app_state
            && app_state.should_quit
        {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.first_frame {
            self.first_frame = false;
            info!("Startup completed in {:?}", self.startup_start.elapsed());
        }

        if let Some(app_state) = &mut self.app_state
            && app_state.pending_new_window
        {
            app_state.pending_new_window = false;
            self.popup.pending_new_window = true;
        }

        frame_tasks::poll_git_status(&mut self.git_status, &mut self.last_git_poll);
        if let Some(app_state) = &mut self.app_state {
            app_state.adblock_blocked_count = self.adblocker.blocked_count();
            frame_tasks::auto_save_workspace(app_state, &self.wry_panes);
        }

        {
            let interval = std::time::Duration::from_secs(self.config.adblock_update_interval_hours * 3600);
            if self.last_filter_update.elapsed() >= interval {
                self.last_filter_update = std::time::Instant::now();
                let updated = aileron::net::filter_list::update_all_filter_lists();
                if updated > 0 {
                    frame_tasks::load_default_adblock_rules(&mut self.adblocker);
                    if let Some(app_state) = &mut self.app_state {
                        app_state.status_message = format!("Updated {} filter list(s)", updated);
                    }
                    info!("Periodic filter list update: {} list(s) refreshed", updated);
                }
            }
        }

        {
            let app_state = match &mut self.app_state {
                Some(s) => s,
                None => return,
            };
            frame_tasks::process_wry_events(
                app_state,
                &mut self.wry_panes,
                &self.content_scripts,
                &mut self.mcp_bridge,
                &self.adblocker,
            );
        }

        frame_tasks::process_pending_wry_actions(
            &mut self.app_state,
            &mut self.wry_panes,
            &mut self.offscreen_panes,
            &self.content_scripts,
        );

        if self.config.is_offscreen()
            && let Some(app_state) = &mut self.app_state
        {
            frame_tasks::process_offscreen_events(
                app_state,
                &mut self.offscreen_panes,
                &self.content_scripts,
                &mut self.mcp_bridge,
                &self.adblocker,
            );
        }

        let ws_name = self
            .app_state
            .as_mut()
            .and_then(|s| s.pending_workspace_restore.take());

        if let Some(ws_name) = ws_name {
            info!("Restoring workspace: {}", ws_name);

            let viewport = match &self.window {
                Some(w) => {
                    let size = w.inner_size();
                    Rect::new(0.0, 0.0, size.width as f64, size.height as f64)
                }
                None => {
                    if let Some(app_state) = &mut self.app_state {
                        app_state.status_message = "Restore failed: no window".into();
                    }
                    return;
                }
            };

            self.wry_panes.remove_all();
            self.offscreen_panes = OffscreenWebViewManager::new();
            self.webview_textures.clear();
            self.webview_texture_handles.clear();
            self.offscreen_last_capture.clear();
            self.pending_pane_creates.clear();

            let app_state = match &mut self.app_state {
                Some(s) => s,
                None => return,
            };

            let outcome = aileron::workspace_restore::restore_workspace(
                &ws_name,
                viewport,
                app_state.db.as_ref(),
                &mut app_state.terminal_pane_ids,
                &mut app_state.engines,
                &mut app_state.wm,
                &mut self.terminal_manager,
            );

            match outcome {
                aileron::workspace_restore::RestoreOutcome::Restored(result) => {
                    let active_id = app_state.wm.active_pane_id();
                    for (pid, url) in result.panes_to_create {
                        if pid == active_id {
                            self.create_wry_pane_for(pid, &url);
                        } else if self.config.is_offscreen() {
                            self.pending_pane_creates.push_back((pid, url));
                        } else {
                            self.create_wry_pane_for(pid, &url);
                        }
                    }
                    if let Some(s) = self.app_state.as_mut() {
                        s.status_message = format!(
                            "Workspace restored: {} ({} panes)",
                            ws_name, result.pane_count
                        );
                    }
                }
                aileron::workspace_restore::RestoreOutcome::NotFound => {
                    if let Some(s) = self.app_state.as_mut() {
                        s.status_message = format!("Workspace '{}' not found", ws_name);
                    }
                }
                aileron::workspace_restore::RestoreOutcome::NoDatabase => {
                    if let Some(s) = self.app_state.as_mut() {
                        s.status_message = "Restore failed: no database".into();
                    }
                }
                aileron::workspace_restore::RestoreOutcome::TreeError(e) => {
                    if let Some(s) = self.app_state.as_mut() {
                        s.status_message = format!("Restore failed (tree): {}", e);
                    }
                }
            }
        }

        let active_id = self
            .app_state
            .as_ref()
            .map(|s| s.wm.active_pane_id())
            .unwrap_or_default();
        frame_tasks::process_mcp_commands(&self.mcp_bridge, &mut self.wry_panes, active_id);

        if let Some(close_id) = self
            .app_state
            .as_mut()
            .and_then(|s| s.pending_tab_close.take())
        {
            if let Some(app_state) = &mut self.app_state {
                frame_tasks::handle_pending_tab_close(app_state, close_id);
            }
            self.remove_wry_pane_for(&close_id);
            self.reposition_all_panes();
        }

        frame_tasks::poll_terminal_output(&mut self.terminal_manager);

        // Handle pending mark jumps (scroll to stored position).
        if let Some(app_state) = &mut self.app_state
            && let Some(frac) = app_state.pending_mark_jump.take()
        {
            let active_id = app_state.wm.active_pane_id();
            if let Some(pane) = self.offscreen_panes.get_mut(&active_id) {
                let js = format!(
                    "window.scrollTo(0, document.documentElement.scrollHeight * {})",
                    frac
                );
                pane.execute_js(&js);
                pane.mark_dirty();
            }
        }

        self.reposition_all_panes();
        frame_tasks::pump_gtk_loop();

        // TASK-K27: create at most one deferred offscreen pane per frame.
        self.drain_pending_pane_creates();

        // Architecture B: capture dirty offscreen frames and update egui textures.
        let textures_updated = self.update_webview_textures();

        // Request redraw if:
        // 1. egui explicitly requested a repaint (UI interaction), OR
        // 2. A webview texture was updated (new frame from offscreen webview), OR
        // 3. We have offscreen panes (continuous repaint for async web content)
        if let Some(winit_state) = &self.egui_winit {
            let egui_ctx = winit_state.egui_ctx();
            let needs_repaint = egui_ctx.has_requested_repaint()
                || textures_updated
                || !self.offscreen_panes.is_empty();
            if needs_repaint
                && let Some(window) = &self.window
            {
                window.request_redraw();
            }
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, _cause: winit::event::StartCause) {
        if self.popup.pending_new_window {
            self.popup.pending_new_window = false;
            let window = Arc::new(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Aileron")
                            .with_inner_size(winit::dpi::LogicalSize::new(
                                self.config.window_width,
                                self.config.window_height,
                            )),
                    )
                    .expect("Failed to create popup window"),
            );
            let window_id = window.id();
            self.popup.pending_popup_window = Some((window_id, window));
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        info!("Clean shutdown — clearing session-active flag");
        Config::clear_session_active();
    }
}

fn key_to_js(key: &aileron::input::Key) -> (String, String) {
    match key {
        aileron::input::Key::Enter => ("Enter".into(), "Enter".into()),
        aileron::input::Key::Backspace => ("Backspace".into(), "Backspace".into()),
        aileron::input::Key::Tab => ("Tab".into(), "Tab".into()),
        aileron::input::Key::Escape => ("Escape".into(), "Escape".into()),
        aileron::input::Key::Up => ("ArrowUp".into(), "ArrowUp".into()),
        aileron::input::Key::Down => ("ArrowDown".into(), "ArrowDown".into()),
        aileron::input::Key::Left => ("ArrowLeft".into(), "ArrowLeft".into()),
        aileron::input::Key::Right => ("ArrowRight".into(), "ArrowRight".into()),
        aileron::input::Key::Home => ("Home".into(), "Home".into()),
        aileron::input::Key::End => ("End".into(), "End".into()),
        aileron::input::Key::PageUp => ("PageUp".into(), "PageUp".into()),
        aileron::input::Key::PageDown => ("PageDown".into(), "PageDown".into()),
        aileron::input::Key::F(n) => (format!("F{}", n), format!("F{}", n)),
        _ => ("".into(), "".into()),
    }
}

/// Convert an aileron Key + modifiers to a terminal escape sequence.
/// This is the native terminal equivalent of key_to_js — it sends
/// the appropriate VT100/xterm escape sequence to the PTY.
fn key_to_escape_sequence(key: &aileron::input::Key, mods: aileron::input::Modifiers) -> String {
    use aileron::input::Key;

    let ctrl = mods.ctrl;
    let shift = mods.shift;
    let alt = mods.alt;

    // Control letter: Ctrl+A through Ctrl+Z → \x01 through \x1A
    if ctrl
        && let Key::Character(c) = key
    {
        let lower = c.to_ascii_lowercase();
        let byte = lower as u32;
        if (0x61..=0x7a).contains(&byte) {
            // a=0x61 → Ctrl+A = 0x01
            return String::from_utf8_lossy(&[(byte - 0x60) as u8]).to_string();
        }
    }

    // Alt+letter: ESC followed by the character
    if alt
        && let Key::Character(c) = key
    {
        return format!("\x1b{}", c);
    }

    match key {
        Key::Enter => "\r".into(),
        Key::Backspace => "\x7f".into(), // DEL
        Key::Tab => "\t".into(),
        Key::Escape => "\x1b".into(),
        Key::Up => {
            if shift { "\x1b[1;2A".into() } else { "\x1b[A".into() }
        }
        Key::Down => {
            if shift { "\x1b[1;2B".into() } else { "\x1b[B".into() }
        }
        Key::Right => {
            if shift { "\x1b[1;2C".into() } else { "\x1b[C".into() }
        }
        Key::Left => {
            if shift { "\x1b[1;2D".into() } else { "\x1b[D".into() }
        }
        Key::Home => "\x1b[H".into(),
        Key::End => "\x1b[F".into(),
        Key::PageUp => "\x1b[5~".into(),
        Key::PageDown => "\x1b[6~".into(),
        Key::F(1) => "\x1bOP".into(),
        Key::F(2) => "\x1bOQ".into(),
        Key::F(3) => "\x1bOR".into(),
        Key::F(4) => "\x1bOS".into(),
        Key::F(5) => "\x1b[15~".into(),
        Key::F(6) => "\x1b[17~".into(),
        Key::F(7) => "\x1b[18~".into(),
        Key::F(8) => "\x1b[19~".into(),
        Key::F(9) => "\x1b[20~".into(),
        Key::F(10) => "\x1b[21~".into(),
        Key::F(11) => "\x1b[23~".into(),
        Key::F(12) => "\x1b[24~".into(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashSet, VecDeque};

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

fn main() -> anyhow::Result<()> {
    // Install panic hook BEFORE anything else — writes crash report to file
    bootstrap::install_panic_hook();

    // Initialize tracing to both stderr AND a log file
    let log_dir = directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|d| d.data_dir().join("logs"))
        .unwrap_or_else(|| std::path::PathBuf::from("./logs"));
    let _ = std::fs::create_dir_all(&log_dir);
    let log_file_path = log_dir.join(format!("aileron_{}.log", chrono::Local::now().format("%Y%m%d_%H%M%S")));
    let log_file = std::fs::File::create(&log_file_path).ok();
    if log_file.is_some() {
        eprintln!("[aileron] Logging to: {}", log_file_path.display());
    }

    // Build subscriber with optional file layer
    let subscriber = tracing_subscriber::fmt::Subscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "aileron=debug,wgpu=warn,wry=debug,webkit2gtk=debug,gdk=debug,gtk=debug,egui=info".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .finish();

    // We can't easily add a file layer with type-compatible subscriber,
    // so just use stderr + direct file writes via the crash hook
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Aileron v0.12.0");
    info!("Keyboard-Driven Web Environment");
    info!("OS: {} {}", std::env::consts::OS, std::env::consts::ARCH);
    info!("PID: {}", std::process::id());

    // Log environment info
    bootstrap::log_environment();

    // Phase 1: Load config
    info!("── Phase 1: Loading config ──");
    let config = Config::load();
    info!("Config loaded: render_mode={}, tab_layout={}, theme={}", config.render_mode, config.tab_layout, config.theme);

    // Force GDK_BACKEND=x11 on Wayland so GTK/WebKitGTK can create a GL context.
    // NVIDIA's Wayland EGL doesn't provide GL through the GDK Wayland backend,
    // causing "GDK is not able to create a GL context" → SIGTRAP.
    // We do NOT remove WAYLAND_DISPLAY so winit continues using Wayland directly.
    #[cfg(target_os = "linux")]
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        info!("Wayland detected — forcing GDK_BACKEND=x11 for GTK/WebKitGTK GL context");
        unsafe {
            std::env::set_var("GDK_BACKEND", "x11");
        }
    }

    // Disable WebKitGTK's DMA-BUF renderer on NVIDIA to prevent
    // "Failed to create GBM buffer" which causes the offscreen pixbuf
    // to remain empty (no visual content rendered).
    // Falls back to the shared GL texture path which works on all GPUs.
    #[cfg(target_os = "linux")]
    unsafe {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    // Disable WebKitGTK's threaded compositor so it renders through cairo
    // (software path) instead of OpenGL. This is essential for the
    // OffscreenWindow pixbuf capture to contain actual web content rather
    // than just a blank GL proxy surface. Without this, GPU-accelerated
    // WebKitGTK renders web content into an OpenGL texture that pixbuf()
    // cannot see — only the GTK widget background is captured.
    #[cfg(target_os = "linux")]
    unsafe {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }
    info!("Set WEBKIT_DISABLE_COMPOSITING_MODE=1 (forces cairo software rendering for pixbuf capture)");

    // Phase 2: Initialize GTK
    info!("── Phase 2: Initializing GTK ──");
    init_gtk();
    info!("GTK initialized successfully");

    // Phase 3: Create event loop
    info!("── Phase 3: Creating event loop ──");
    let event_loop = EventLoop::builder().build()?;
    info!("Event loop created successfully");

    // Workaround: X11 error handler (GTK uses XWayland on Wayland systems)
    #[cfg(target_os = "linux")]
    {
        unsafe {
            if let Ok(xlib) = x11_dl::xlib::Xlib::open() {
                (xlib.XSetErrorHandler)(Some(x11_error_handler));
                info!("X11 error handler installed");
            }
        }
    }

    // Phase 4: Create app and run
    info!("── Phase 4: Creating application ──");
    Config::set_session_active();
    let mut app = AileronApp::new();
    info!("Application created successfully");

    info!("── Phase 5: Entering event loop ──");
    event_loop.run_app(&mut app)?;

    info!("Aileron shutting down.");
    Ok(())
}
