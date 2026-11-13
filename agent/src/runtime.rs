//! The aileron agent runtime (S4 stage 2d-3b): headless MCP-only daemon.
//!
//! Holds `CoreState` directly — no `AppState`, no headed shell. The GTK
//! main loop stays in this binary because WebKitGTK delivers its events
//! on it; everything else (state, MCP stack, dispatch) is core, and the
//! pane mechanics are engine (`CombinedPaneOps` implements `PaneOps`).

use std::sync::Arc;

use aileron_core::agent::{process_mcp_commands, spawn_mcp_server};
use aileron_core::config::Config;
use aileron_core::mcp::McpBridge;
use aileron_core::net::adblock::AdBlocker;
use aileron_core::scripts::ContentScriptManager;
use aileron_core::state::CoreState;
use aileron_engine::offscreen_webview::OffscreenWebViewManager;
use aileron_engine::pane_ops::CombinedPaneOps;
use aileron_engine::servo::WryPaneManager;
use aileron_engine::servo::wry_engine::WryEvent;

const VIEWPORT_W: f64 = 1280.0;
const VIEWPORT_H: f64 = 800.0;
const TICK_MS: u64 = 60;

/// NVIDIA GPUs: DMA-BUF renderer must be disabled before WebKitGTK
/// initializes (shared-GL fallback), or GDK cannot create a GL context
/// and the webview aborts.
#[cfg(target_os = "linux")]
fn is_nvidia_gpu() -> bool {
    (0..=9).any(|i| {
        let path = format!("/sys/class/drm/card{i}/device/vendor");
        std::fs::read_to_string(&path)
            .map(|v| v.trim() == "0x10de")
            .unwrap_or(false)
    })
}

pub fn run() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    // SAFETY: single-threaded at this point; WebKitGTK reads the env var
    // during its later initialization on this same thread.
    unsafe {
        if is_nvidia_gpu() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            tracing::info!("NVIDIA GPU detected -- disabled DMA-BUF renderer");
        }
    }

    aileron_engine::servo::init_gtk();

    let config = Config::load();
    tracing::info!(
        "agent runtime starting: render_mode={} (agent profile ignores it)",
        config.render_mode
    );
    Config::set_session_active();

    let mut core = CoreState::new(
        aileron_core::wm::Rect::new(0.0, 0.0, VIEWPORT_W, VIEWPORT_H),
        config.clone(),
    )
    .map_err(|e| anyhow::anyhow!("agent: CoreState init failed: {e}"))?;
    {
        let mut ext = core.extension_manager.write();
        let loaded = ext.load_all().len();
        ext.register_builtin_adblock();
        if loaded > 0 {
            tracing::info!("agent: loaded {loaded} user extension(s)");
        }
    }

    let mut adblocker = AdBlocker::new();
    let mcp_bridge = McpBridge::new();
    let mut offscreen_panes = OffscreenWebViewManager::new();
    // X11-native wry panes never exist in the agent profile; the manager
    // stays empty and exists only as the CombinedPaneOps half.
    let wry_panes = WryPaneManager::new();
    let content_scripts = ContentScriptManager::new();

    // MCP surface: TCP 127.0.0.1:9543 (the `aileron-mcp` relay target) plus
    // the in-process stdio transport, identical tools on both.
    spawn_mcp_server(
        &mcp_bridge,
        core.session_registry.clone(),
        &config.mcp_policy,
    );

    // Initial pane: the BSP root CoreState::new created from the homepage.
    let root_pane = core.wm.active_pane_id();
    create_agent_pane(&mut core, &mut offscreen_panes, &mut adblocker, root_pane);
    tracing::info!(
        "agent: initial pane {} -> {}",
        &root_pane.to_string()[..8],
        config.homepage
    );

    let tick = std::cell::RefCell::new(AgentTick {
        core,
        offscreen_panes,
        wry_panes,
        adblocker,
        content_scripts,
        bridge: mcp_bridge,
    });
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(TICK_MS), move || {
        tick.borrow_mut().run();
        gtk::glib::ControlFlow::Continue
    });

    tracing::info!(
        "agent: entering GTK main loop (MCP on 127.0.0.1:{})",
        aileron_core::mcp::MCP_TCP_PORT
    );
    gtk::main();
    Ok(())
}

struct AgentTick {
    core: CoreState,
    offscreen_panes: OffscreenWebViewManager,
    #[allow(dead_code)] // CombinedPaneOps half; agent has no native panes
    wry_panes: WryPaneManager,
    adblocker: AdBlocker,
    content_scripts: ContentScriptManager,
    bridge: McpBridge,
}

impl AgentTick {
    fn run(&mut self) {
        // Event parity with the headed runtime: visit recording, tab
        // title/url sync, downloads, extensions, IPC shims.
        let registry = self
            .core
            .extension_manager
            .read()
            .interceptor_registry
            .clone();
        aileron_engine::offscreen_events::process_offscreen_events(
            &mut self.core,
            &mut self.offscreen_panes,
            &self.content_scripts,
            &self.bridge,
            &self.adblocker,
            &registry,
        );

        // MCP-requested panes: build the offscreen webview with full wiring.
        let mut creations = Vec::new();
        while let Some(c) = self.core.pending_mcp_pane_creations.pop_front() {
            creations.push(c);
        }
        for (pane_id, url) in creations {
            tracing::info!("agent: MCP pane {} -> {}", &pane_id.to_string()[..8], url);
            create_agent_pane_at(
                &mut self.core,
                &mut self.offscreen_panes,
                &mut self.adblocker,
                pane_id,
                Some(&url),
            );
        }

        // Workspace restore deferral (YP-WORKSPACE-002 parity).
        if self.core.pending_workspace_restore.is_none() {
            let active_id = self.core.wm.active_pane_id();
            let mut panes = CombinedPaneOps {
                wry: &mut self.wry_panes,
                off: &mut self.offscreen_panes,
            };
            process_mcp_commands(&self.bridge, active_id, &mut self.core, &mut panes);
        }
    }
}

fn create_agent_pane(
    core: &mut CoreState,
    offscreen_panes: &mut OffscreenWebViewManager,
    adblocker: &mut AdBlocker,
    pane_id: uuid::Uuid,
) {
    create_agent_pane_at(core, offscreen_panes, adblocker, pane_id, None)
}

/// Create the pane's webview at an explicit URL (queued MCP creations);
/// `None` resolves to the configured homepage (the initial BSP root).
fn create_agent_pane_at(
    core: &mut CoreState,
    offscreen_panes: &mut OffscreenWebViewManager,
    adblocker: &mut AdBlocker,
    pane_id: uuid::Uuid,
    target: Option<&url::Url>,
) {
    use aileron_core::wm::Rect;
    let rect = core
        .wm
        .panes_ref()
        .iter()
        .find(|(id, _)| *id == pane_id)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| Rect::new(0.0, 0.0, VIEWPORT_W, VIEWPORT_H));
    let (width, height) = (rect.w.round() as i32, rect.h.round() as i32);
    let url = target.cloned().unwrap_or_else(|| {
        let homepage = core.config.homepage.clone();
        url::Url::parse(&homepage).unwrap_or_else(|_| url::Url::parse("aileron://welcome").unwrap())
    });
    let blocked_domains: Vec<String> = adblocker.blocked_domains_iter();
    let https_safe_list = if core.config.https_upgrade_enabled {
        core.get_cached_https_safe_list()
    } else {
        Arc::new(std::collections::HashSet::new())
    };
    let registry = core.extension_manager.read().interceptor_registry.clone();
    if let Err(e) = offscreen_panes.create_pane_with_privacy(
        pane_id,
        &url,
        width,
        height,
        blocked_domains,
        https_safe_list,
        core.config.https_upgrade_enabled,
        core.config.tracking_protection_enabled,
        false, // devtools
        core.config.popup_blocker_enabled,
        Some(registry),
    ) {
        tracing::error!(
            "agent: pane {} creation failed: {e}",
            &pane_id.to_string()[..8]
        );
    }
}

// Suppress unused warnings for types only referenced via re-exported
// engine paths in non-linux builds.
#[allow(unused)]
fn _wry_event_marker(_e: WryEvent) {}
