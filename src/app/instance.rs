use std::sync::Arc;
use tracing::info;
use winit::window::Window;

use crate::app::AppState;
use crate::config::Config;
use crate::input::Modifiers;
#[cfg(feature = "mcp")]
use crate::mcp::McpBridge;
use crate::net::adblock::AdBlocker;
use crate::popup::PopupManager;
use crate::profiling::AdaptiveQuality;
use crate::servo::WryPaneManager;
#[cfg(feature = "terminal")]
use crate::terminal::NativeTerminalManager;

pub const STATUS_BAR_HEIGHT: f64 = 32.0;
pub const URL_BAR_HEIGHT: f64 = 32.0;

pub struct AileronApp {
    pub(crate) window: Option<Arc<Window>>,
    pub(crate) app_state: Option<AppState>,
    pub(crate) modifiers: Modifiers,
    pub(crate) config: Config,

    pub(crate) wry_panes: WryPaneManager,

    pub(crate) adblocker: AdBlocker,

    #[cfg(feature = "mcp")]
    pub(crate) mcp_bridge: McpBridge,

    #[cfg(feature = "terminal")]
    pub(crate) terminal_manager: NativeTerminalManager,

    pub(crate) content_scripts: crate::scripts::ContentScriptManager,

    pub(crate) git_status: crate::git::GitStatus,

    pub(crate) git_poller: Option<crate::git::GitPoller>,

    pub(crate) popup: PopupManager,

    pub(crate) first_frame: bool,

    pub(crate) resize_pending: bool,

    pub(crate) frame_count: u64,

    pub(crate) startup_start: std::time::Instant,

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

        #[cfg(feature = "mcp")]
        let mcp_bridge = McpBridge::new();
        let mut adaptive_quality = AdaptiveQuality::new();
        adaptive_quality.set_enabled(config.adaptive_quality);
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

impl AileronApp {}
