//! Agent-native runtime (ROADMAP-2027-2031 Era-1, S1–S3).
//!
//! A headless, MCP-only browser daemon: offscreen WebKitGTK panes, storage,
//! and the MCP relay on the GTK main loop. Compiles out *at runtime* the
//! subsystems the 2026-10 campaign identified as cost/failure centers: the
//! winit window, the wgpu compositor, the chrome webview, the terminal, and
//! input routing. Pixels exist only on explicit `screenshot` requests.
//!
//! Launch: `aileron --agent` (designed for Xvfb / a headless display; the
//! `aileron-agent.service` unit runs it under Xvfb).

use aileron::app::AppState;
use aileron::config::Config;
use aileron::frame_tasks;
use aileron::mcp::McpBridge;
use aileron::net::adblock::AdBlocker;
use aileron::offscreen_webview::OffscreenWebViewManager;
use aileron::scripts::ContentScriptManager;
use aileron::servo::WryPaneManager;
use aileron::wm::Rect;

/// Ticks per second for the agent main loop (MCP command pump + event drain).
/// 125 Hz mirrors the frame loop's command latency without any compositing.
const TICK_MS: u64 = 8;

/// Virtual viewport for the BSP tree; panes are offscreen, so this only sizes
/// the layout rectangles handed to WebKitGTK.
const VIEWPORT_W: f64 = 1280.0;
const VIEWPORT_H: f64 = 800.0;

/// Run the agent daemon. Called from `bootstrap::run` after the panic hook and
/// logging are installed (`aileron --agent`).
pub fn run() -> anyhow::Result<()> {
    // Same NVIDIA constraint as the headed runtime: DMA-BUF renderer must be
    // disabled before WebKitGTK initializes (shared-GL fallback), or GDK
    // cannot create a GL context and the webview aborts.
    #[cfg(target_os = "linux")]
    // SAFETY: single-threaded at this point; WebKitGTK reads the env var
    // during its later initialization on this same thread.
    unsafe {
        if crate::event_handlers::is_nvidia_gpu() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            tracing::info!("NVIDIA GPU detected -- disabled DMA-BUF renderer");
        }
    }

    aileron::servo::init_gtk();

    let config = Config::load();
    tracing::info!(
        "agent runtime starting: render_mode={} (agent profile ignores it)",
        config.render_mode
    );
    Config::set_session_active();

    let mut app_state = AppState::new(Rect::new(0.0, 0.0, VIEWPORT_W, VIEWPORT_H), config.clone())
        .map_err(|e| anyhow::anyhow!("agent: AppState init failed: {e}"))?;
    {
        let mut ext = app_state.extension_manager.write();
        let loaded = ext.load_all().len();
        ext.register_builtin_adblock();
        if loaded > 0 {
            tracing::info!("agent: loaded {loaded} user extension(s)");
        }
    }

    let mut adblocker = AdBlocker::new();
    let mcp_bridge = McpBridge::new();
    let mut offscreen_panes = OffscreenWebViewManager::new();
    // X11-native wry panes never exist in the agent profile; the manager stays
    // empty and is passed only to satisfy the shared MCP dispatch signature.
    let wry_panes = WryPaneManager::new();
    let content_scripts = ContentScriptManager::new();

    // MCP surface: TCP 127.0.0.1:9543 (the `aileron-mcp` relay target) plus the
    // in-process stdio transport, identical tools on both.
    frame_tasks::spawn_mcp_server(
        &mcp_bridge,
        app_state.session_registry.clone(),
        &config.mcp_policy,
    );

    // Initial pane: the BSP root AppState::new created from the homepage.
    let root_pane = app_state.wm.active_pane_id();
    create_agent_pane(
        &mut app_state,
        &mut offscreen_panes,
        &mut adblocker,
        root_pane,
    );
    tracing::info!(
        "agent: initial pane {} -> {}",
        &root_pane.to_string()[..8],
        config.homepage
    );

    let tick = std::cell::RefCell::new(AgentTick {
        app_state,
        offscreen_panes,
        wry_panes,
        adblocker,
        content_scripts,
        bridge: mcp_bridge,
    });
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(TICK_MS), move || {
        tick.borrow_mut().run();
        // This glib generation: timeout closures return `glib::ControlFlow`.
        gtk::glib::ControlFlow::Continue
    });

    tracing::info!(
        "agent: entering GTK main loop (MCP on 127.0.0.1:{} )",
        aileron::mcp::MCP_TCP_PORT
    );
    gtk::main();
    Ok(())
}

struct AgentTick {
    app_state: AppState,
    offscreen_panes: OffscreenWebViewManager,
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
            .app_state
            .extension_manager
            .read()
            .interceptor_registry
            .clone();
        frame_tasks::process_offscreen_events(
            &mut self.app_state,
            &mut self.offscreen_panes,
            &self.content_scripts,
            &self.bridge,
            &self.adblocker,
            &registry,
        );

        // MCP-requested panes: build the offscreen webview with full wiring.
        let mut creations = Vec::new();
        while let Some(c) = self.app_state.pending_mcp_pane_creations.pop_front() {
            creations.push(c);
        }
        for (pane_id, url) in creations {
            tracing::info!("agent: MCP pane {} -> {}", &pane_id.to_string()[..8], url);
            create_agent_pane(
                &mut self.app_state,
                &mut self.offscreen_panes,
                &mut self.adblocker,
                pane_id,
            );
            let _ = url;
        }

        // Workspace restore deferral (YP-WORKSPACE-002 parity with headed mode).
        if self.app_state.pending_workspace_restore.is_none() {
            let active_id = self.app_state.wm.active_pane_id();
            frame_tasks::process_mcp_commands(
                &self.bridge,
                &mut self.wry_panes,
                active_id,
                &mut self.app_state,
                &mut self.offscreen_panes,
            );
        }
    }
}

/// Create an offscreen pane with the headed runtime's full wiring (adblock
/// domains, HTTPS-upgrade safe list, interceptor registry) at the BSP rect.
fn create_agent_pane(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
    adblocker: &mut AdBlocker,
    pane_id: uuid::Uuid,
) {
    let rect = app_state
        .wm
        .panes_ref()
        .iter()
        .find(|(id, _)| *id == pane_id)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| Rect::new(0.0, 0.0, VIEWPORT_W, VIEWPORT_H));
    let (width, height) = (rect.w.round() as i32, rect.h.round() as i32);
    let url = {
        let homepage = app_state.config.homepage.clone();
        url::Url::parse(&homepage).unwrap_or_else(|_| url::Url::parse("aileron://welcome").unwrap())
    };
    let blocked_domains: Vec<String> = adblocker.blocked_domains_iter();
    let https_safe_list = if app_state.config.https_upgrade_enabled {
        app_state.get_cached_https_safe_list()
    } else {
        std::sync::Arc::new(std::collections::HashSet::new())
    };
    let registry = app_state
        .extension_manager
        .read()
        .interceptor_registry
        .clone();
    if let Err(e) = offscreen_panes.create_pane_with_privacy(
        pane_id,
        &url,
        width,
        height,
        blocked_domains,
        https_safe_list,
        app_state.config.https_upgrade_enabled,
        app_state.config.tracking_protection_enabled,
        false, // devtools
        app_state.config.popup_blocker_enabled,
        Some(registry),
    ) {
        tracing::error!(
            "agent: pane {} creation failed: {e}",
            &pane_id.to_string()[..8]
        );
    }
}
