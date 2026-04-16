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
use aileron::popup::PopupManager;
use aileron::servo::{bsp_rect_to_wry_rect, init_gtk, WryPaneManager};
use aileron::terminal::TerminalManager;
use aileron::ui::panels;
use aileron::wm::Rect;

mod frame_tasks;

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
    terminal_manager: TerminalManager,

    /// Per-terminal-pane input senders. JS→Rust IPC uses these.
    terminal_input_tx: Arc<
        std::sync::Mutex<std::collections::HashMap<uuid::Uuid, std::sync::mpsc::Sender<String>>>,
    >,

    /// Channel for terminal resize events from JS→Rust IPC.
    /// JS sends {t:'r', rows, cols} → IPC handler sends (pane_id, rows, cols) → drained in about_to_wait().
    terminal_resize_tx: std::sync::mpsc::Sender<(uuid::Uuid, u16, u16)>,
    terminal_resize_rx: std::sync::mpsc::Receiver<(uuid::Uuid, u16, u16)>,

    content_scripts: aileron::scripts::ContentScriptManager,

    /// Current git status for the working directory.
    git_status: aileron::git::GitStatus,

    /// Last time git status was polled (throttled to 1 Hz).
    last_git_poll: std::time::Instant,

    /// Standalone popup browser windows (no egui overlay, no tiling).
    popup: PopupManager,

    /// Tracks whether the first frame has rendered (for startup timing).
    first_frame: bool,

    /// Instant when the app was created (for startup timing).
    startup_start: std::time::Instant,
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
        let (terminal_resize_tx, terminal_resize_rx) = std::sync::mpsc::channel();
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
            terminal_manager: TerminalManager::new(),
            terminal_input_tx: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            terminal_resize_tx,
            terminal_resize_rx,
            content_scripts: aileron::scripts::ContentScriptManager::new(),
            git_status: aileron::git::GitStatus::default(),
            last_git_poll: std::time::Instant::now(),
            popup: PopupManager::new(),
            first_frame: true,
            startup_start: std::time::Instant::now(),
        }
    }

    fn init_graphics(&mut self, window: Arc<Window>) {
        // Create egui context and winit state
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

        // Create GPU state
        let gfx = match GfxState::new(Arc::clone(&window)) {
            Ok(g) => g,
            Err(e) => {
                tracing::error!("Failed to initialize GPU: {}", e);
                return;
            }
        };

        winit_state.set_max_texture_side(gfx.device.limits().max_texture_dimension_2d as usize);

        // Initialize app state with viewport and config
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

        self.egui_winit = Some(winit_state);
        self.gfx = Some(gfx);
        self.app_state = Some(app_state);
        self.window = Some(window);
    }

    /// Create a wry webview for a BSP pane.
    /// Called when a new pane is created (initial + splits).
    fn create_wry_pane_for(&mut self, pane_id: uuid::Uuid, url: &url::Url) {
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
                Some((_, rect)) => rect.clone(),
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

        let terminal_input_tx = self.terminal_input_tx.clone();
        let terminal_resize_tx = self.terminal_resize_tx.clone();

        match self.wry_panes.create_pane(
            &*window,
            pane_id,
            url.clone(),
            wry_rect,
            blocked_domains,
            terminal_input_tx,
            terminal_resize_tx,
        ) {
            Ok(()) => {
                if is_terminal {
                    match self.terminal_manager.create_terminal(pane_id, 80, 24) {
                        Ok((tx, _size)) => {
                            self.terminal_input_tx.lock().unwrap().insert(pane_id, tx);

                            if let Some(app_state) = &mut self.app_state {
                                if let Some(cmd) = app_state.pending_terminal_command.take() {
                                    self.terminal_manager.write_input(&pane_id, &cmd);
                                }
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

    /// Remove a wry pane when a BSP leaf is closed.
    fn remove_wry_pane_for(&mut self, pane_id: &uuid::Uuid) {
        self.terminal_manager.remove(pane_id);
        self.terminal_input_tx.lock().unwrap().remove(pane_id);
        self.wry_panes.remove_pane(pane_id);
    }

    /// Create a wry webview for a standalone popup window.
    fn init_popup_window(&mut self, window_id: WindowId, window: Arc<Window>) {
        let url = self
            .app_state
            .as_mut()
            .and_then(|s| s.pending_detach_url.take())
            .unwrap_or_else(|| url::Url::parse("aileron://new").unwrap());
        let blocked_domains: Vec<String> = self.adblocker.blocked_domains_iter();
        let terminal_input_tx = self.terminal_input_tx.clone();
        let terminal_resize_tx = self.terminal_resize_tx.clone();

        self.popup.init_popup_window(
            window_id,
            window,
            url,
            blocked_domains,
            terminal_input_tx,
            terminal_resize_tx,
        );
    }

    /// Handle a window event for a popup window.
    fn handle_popup_event(&mut self, window_id: WindowId, event: &WindowEvent) {
        self.popup.handle_popup_event(window_id, event);
    }

    /// Reposition all wry panes to match current BSP layout.
    /// Called on window resize and after splits/closes.
    fn reposition_all_panes(&self) {
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
                if app_state.terminal_pane_ids.contains(pane_id) {
                    wry_pane.execute_js("_terminalFit()");
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
}

impl ApplicationHandler for AileronApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

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

        // Auto-restore the most recent workspace if configured.
        // Prefer the _autosave workspace (crash recovery) over user-named ones.
        if self.config.restore_session {
            if let Some(app_state) = &mut self.app_state {
                let all_workspaces = app_state
                    .db
                    .as_ref()
                    .and_then(|conn| aileron::db::workspaces::list_workspaces(conn).ok())
                    .unwrap_or_default();

                let to_restore = all_workspaces
                    .iter()
                    .find(|ws| ws.name == "_autosave")
                    .or_else(|| all_workspaces.first())
                    .cloned();

                if let Some(workspace) = to_restore {
                    info!("Auto-restoring workspace: {}", workspace.name);
                    app_state.pending_workspace_restore = Some(workspace.name);
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
        if let Some(app_state) = &mut self.app_state {
            if let WindowEvent::Resized(physical_size) = &event {
                if physical_size.width > 0 && physical_size.height > 0 {
                    app_state.wm.resize(Rect::new(
                        0.0,
                        0.0,
                        physical_size.width as f64,
                        physical_size.height as f64,
                    ));
                    // Reposition wry panes to match new BSP layout
                    self.reposition_all_panes();
                }
            }
        }

        // Handle events
        match &event {
            WindowEvent::CloseRequested => {
                info!("Close requested — quitting");
                event_loop.exit();
            }

            WindowEvent::RedrawRequested => {
                self.render();
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
                if *repeat {
                    return;
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
                    // ─── Hint mode: intercept digit keys to follow hinted links ───
                    if app_state.hint_mode {
                        match &key {
                            aileron::input::Key::Character(c) if c.is_ascii_digit() => {
                                app_state.hint_buffer.push(*c);
                                // Try to click the hinted element
                                let hint_buf = app_state.hint_buffer.clone();
                                let js = format!(
                                    "(function() {{ \
                                        var el = document.querySelector('[data-aileron-hint=\"{}\"]'); \
                                        if (el) {{ el.click(); return 'clicked'; }} \
                                        var all = document.querySelectorAll('[data-aileron-hint]'); \
                                        var matches = []; \
                                        all.forEach(function(e) {{ \
                                            if (e.getAttribute('data-aileron-hint').startsWith('{}')) matches.push(e); \
                                        }}); \
                                        if (matches.length === 1) {{ matches[0].click(); return 'clicked'; }} \
                                        return 'typing'; \
                                    }})()",
                                    hint_buf, hint_buf
                                );
                                let active_id = app_state.wm.active_pane_id();
                                if let Some(wry_pane) = self.wry_panes.get(&active_id) {
                                    wry_pane.execute_js(&js);
                                    // We can't get the return value from execute_js (it's fire-and-forget),
                                    // so we clear hints when the buffer length would be unambiguous enough
                                    // The JS handles click logic internally; we exit hint mode after a short delay
                                    // or on Escape/non-digit key.
                                }
                                return;
                            }
                            _ => {
                                // Any non-digit key exits hint mode
                                let active_id = app_state.wm.active_pane_id();
                                app_state.hint_mode = false;
                                app_state.hint_buffer.clear();
                                // Inline clear_hints to avoid self borrow conflict
                                // (self.app_state mut + self.wry_panes immut can't coexist through &mut ref)
                                if let Some(wry_pane) = self.wry_panes.get(&active_id) {
                                    wry_pane.execute_js(
                                        r#"
                                        (function() {
                                            var style = document.getElementById('__aileron_hints');
                                            if (style) style.remove();
                                            document.querySelectorAll('[data-aileron-hint]').forEach(el => {
                                                el.removeAttribute('data-aileron-hint');
                                            });
                                        })();
                                        "#,
                                    );
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
                    let pane_count_before = app_state.wm.leaf_count();
                    let active_id_before = app_state.wm.active_pane_id();

                    app_state.process_key_event(aileron_event);

                    let pane_count_after = app_state.wm.leaf_count();
                    let active_id_after = app_state.wm.active_pane_id();

                    // Collect info needed for wry sync before borrowing self.wry_panes
                    let mut new_pane_ids: Vec<uuid::Uuid> = Vec::new();
                    let mut closed_pane_id: Option<uuid::Uuid> = None;

                    if pane_count_after > pane_count_before {
                        // A new pane was created (split) — find the new pane ID
                        let all_pane_ids: Vec<_> =
                            app_state.wm.panes().iter().map(|(id, _)| *id).collect();
                        for pid in all_pane_ids {
                            if !self.wry_panes.contains(&pid) {
                                new_pane_ids.push(pid);
                            }
                        }
                    } else if pane_count_after < pane_count_before {
                        if active_id_before != active_id_after {
                            if !app_state
                                .wm
                                .panes()
                                .iter()
                                .any(|(id, _)| *id == active_id_before)
                            {
                                closed_pane_id = Some(active_id_before);
                            }
                        }
                    }

                    let need_reposition = pane_count_after != pane_count_before;
                    let active_pane_id = app_state.wm.active_pane_id();
                    let is_insert_mode = app_state.mode == aileron::input::Mode::Insert;

                    // Now sync wry panes (drop borrow on app_state first)
                    for pid in &new_pane_ids {
                        let new_url = url::Url::parse("aileron://new").unwrap();
                        self.create_wry_pane_for(*pid, &new_url);
                    }

                    if let Some(pid) = closed_pane_id {
                        self.remove_wry_pane_for(&pid);
                    }

                    if need_reposition {
                        self.reposition_all_panes();
                    }

                    // Handle Insert mode: focus the wry webview
                    if is_insert_mode {
                        if let Some(wry_pane) = self.wry_panes.get(&active_pane_id) {
                            wry_pane.focus();
                        }
                    }
                }
            }

            WindowEvent::DroppedFile(path) => {
                info!("File dropped: {:?}", path);
            }

            WindowEvent::MouseWheel { delta, .. } => {
                // Forward mouse wheel to wry pane when in Insert mode
                // (egui handles scrolling in its own widgets, so we only forward
                // when the user is interacting with the web content)
                if let Some(app_state) = &self.app_state {
                    if app_state.mode == aileron::input::Mode::Insert {
                        let active_id = app_state.wm.active_pane_id();
                        if let Some(wry_pane) = self.wry_panes.get(&active_id) {
                            // winit uses logical pixels, convert to scroll delta
                            let (dx, dy) = match delta {
                                winit::event::MouseScrollDelta::LineDelta(x, y) => {
                                    (*x as f64 * 40.0, *y as f64 * 40.0)
                                }
                                winit::event::MouseScrollDelta::PixelDelta(pos) => {
                                    (pos.x as f64, pos.y as f64)
                                }
                            };
                            if dx.abs() > 0.1 || dy.abs() > 0.1 {
                                let js = format!("window.scrollBy({}, {})", dx, dy);
                                wry_pane.execute_js(&js);
                            }
                        }
                    }
                }
                // Also let egui handle it (for its own scrollable areas)
                // via the earlier winit_state.on_window_event() call at line 393
            }

            _ => {}
        }

        // Check if app wants to quit
        if let Some(app_state) = &self.app_state {
            if app_state.should_quit {
                event_loop.exit();
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.first_frame {
            self.first_frame = false;
            info!("Startup completed in {:?}", self.startup_start.elapsed());
        }

        if let Some(app_state) = &mut self.app_state {
            if app_state.pending_new_window {
                app_state.pending_new_window = false;
                self.popup.pending_new_window = true;
            }
        }

        frame_tasks::poll_git_status(&mut self.git_status, &mut self.last_git_poll);
        if let Some(app_state) = &mut self.app_state {
            frame_tasks::auto_save_workspace(app_state, &self.wry_panes);
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
            );
        }

        frame_tasks::process_pending_wry_actions(
            &mut self.app_state,
            &mut self.wry_panes,
            &self.content_scripts,
        );

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
                &self.terminal_input_tx,
                &mut self.terminal_manager,
            );

            match outcome {
                aileron::workspace_restore::RestoreOutcome::Restored(result) => {
                    for (pid, url) in result.panes_to_create {
                        self.create_wry_pane_for(pid, &url);
                    }
                    self.app_state.as_mut().map(|s| {
                        s.status_message = format!(
                            "Workspace restored: {} ({} panes)",
                            ws_name, result.pane_count
                        );
                    });
                }
                aileron::workspace_restore::RestoreOutcome::NotFound => {
                    self.app_state.as_mut().map(|s| {
                        s.status_message = format!("Workspace '{}' not found", ws_name);
                    });
                }
                aileron::workspace_restore::RestoreOutcome::NoDatabase => {
                    self.app_state.as_mut().map(|s| {
                        s.status_message = "Restore failed: no database".into();
                    });
                }
                aileron::workspace_restore::RestoreOutcome::TreeError(e) => {
                    self.app_state.as_mut().map(|s| {
                        s.status_message = format!("Restore failed (tree): {}", e);
                    });
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

        frame_tasks::poll_terminal_output(&mut self.terminal_manager, &self.wry_panes);
        frame_tasks::process_terminal_resizes(
            &mut self.terminal_manager,
            &mut self.terminal_resize_rx,
        );

        self.reposition_all_panes();
        frame_tasks::pump_gtk_loop();

        if let Some(winit_state) = &self.egui_winit {
            let egui_ctx = winit_state.egui_ctx();
            if egui_ctx.has_requested_repaint() {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
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
}

fn main() -> anyhow::Result<()> {
    // Initialize tracing
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "aileron=info,wgpu=warn,wry=info".parse().unwrap()),
        )
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Aileron v0.1.0-pre-alpha");
    info!("Keyboard-Driven Web Environment");

    // On Wayland, wry's GTK fallback creates a standalone gtk::Window which
    // conflicts with winit's Wayland surface (Error 71: Protocol error).
    // wry needs an X11 window handle for build_as_child, so we force winit
    // to use XWayland by unsetting WAYLAND_DISPLAY (winit 0.29+ removed
    // WINIT_UNIX_BACKEND and uses WAYLAND_DISPLAY/DISPLAY to pick backend).
    // We also force GDK to use X11 so gtk::init() creates an X11 display.
    // TODO: Remove when wry supports embedding into a winit Wayland surface.
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        info!("Wayland detected — using XWayland for wry build_as_child compatibility");
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");
            std::env::set_var("GDK_BACKEND", "x11");
        }
    }

    // Initialize GTK BEFORE creating the event loop (required by wry on Linux)
    init_gtk();

    let event_loop = EventLoop::builder().build()?;
    info!("Entering event loop...");
    let mut app = AileronApp::new();
    event_loop.run_app(&mut app)?;

    info!("Aileron shutting down.");
    Ok(())
}
