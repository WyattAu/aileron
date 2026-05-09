use std::sync::Arc;
use tracing::info;
use winit::window::Window;

use crate::app::AppState;
use crate::config::Config;
use crate::gfx::GfxState;
use crate::input::Modifiers;
use crate::mcp::McpBridge;
use crate::net::adblock::AdBlocker;
use crate::offscreen_webview::OffscreenWebViewManager;
use crate::popup::PopupManager;
use crate::profiling::AdaptiveQuality;
use crate::servo::WryPaneManager;
use crate::terminal::NativeTerminalManager;
use crate::wm::Rect;

pub const STATUS_BAR_HEIGHT: f64 = 32.0;
pub const URL_BAR_HEIGHT: f64 = 32.0;

pub struct AileronApp {
    pub(crate) window: Option<Arc<Window>>,
    pub(crate) egui_winit: Option<egui_winit::State>,
    pub(crate) gfx: Option<GfxState>,
    pub(crate) app_state: Option<AppState>,
    pub(crate) modifiers: Modifiers,
    pub(crate) config: Config,

    pub(crate) wry_panes: WryPaneManager,

    pub(crate) adblocker: AdBlocker,

    pub(crate) mcp_bridge: McpBridge,

    pub(crate) terminal_manager: NativeTerminalManager,

    pub(crate) content_scripts: crate::scripts::ContentScriptManager,

    pub(crate) git_status: crate::git::GitStatus,

    pub(crate) git_poller: Option<crate::git::GitPoller>,

    pub(crate) popup: PopupManager,

    pub(crate) first_frame: bool,

    pub(crate) resize_pending: bool,

    pub(crate) frame_count: u64,

    pub(crate) startup_start: std::time::Instant,

    pub(crate) offscreen_panes: OffscreenWebViewManager,

    pub(crate) offscreen_mouse_pressed: bool,

    pub(crate) webview_textures: std::collections::HashMap<uuid::Uuid, egui::TextureId>,

    pub(crate) webview_texture_handles: std::collections::HashMap<uuid::Uuid, egui::TextureHandle>,

    pub(crate) offscreen_last_capture: std::collections::HashMap<uuid::Uuid, std::time::Instant>,

    pub(crate) pending_pane_creates: std::collections::VecDeque<(uuid::Uuid, url::Url)>,

    pub(crate) adblock_reload_pending: std::sync::Arc<std::sync::atomic::AtomicBool>,

    pub(crate) adaptive_quality: AdaptiveQuality,

    pub(crate) last_filter_update: std::time::Instant,

    pub(crate) ime_just_committed: bool,
}

impl AileronApp {
    pub fn new() -> Self {
        let config = Config::load();

        if let Some(ref proxy) = config.proxy {
            // SAFETY: This runs before any threads are spawned (before event loop creation).
            // No other thread can read env vars at this point.
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
            content_scripts: crate::scripts::ContentScriptManager::new(),
            git_status: crate::git::GitStatus::default(),
            git_poller: Some(crate::git::GitPoller::new(
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
            pending_pane_creates: std::collections::VecDeque::new(),
            adblock_reload_pending: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            adaptive_quality,
            resize_pending: false,
            last_filter_update: std::time::Instant::now(),
            ime_just_committed: false,
        }
    }
}

impl Default for AileronApp {
    fn default() -> Self {
        Self::new()
    }
}

impl AileronApp {
    pub(crate) fn init_graphics(&mut self, window: Arc<Window>) {
        info!("── init_graphics(): Starting ──");

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
    }
}
