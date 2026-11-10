//! The aileron agent runtime (S4 stage 2).
//!
//! Currently hosts the MCP server bootstrap (stdio + TCP bridge). The
//! agent event loop moves here in stage 2c, once `process_mcp_commands`
//! crosses the crate boundary via the `PaneOps` callback trait — the
//! target layout gives `aileron-agent` = thin `aileron_core::agent::run()`.

use crate::mcp::bridge::McpBridge;
use tracing::{info, warn};

/// Pane operations the MCP dispatch layer needs, abstracted so
/// `process_mcp_commands` can live in core while the pane mechanics
/// (GTK pumping, settle waits, frame capture) stay in the engine side.
///
/// Semantics are behavior-locked by the GATE-3 soak — implementations
/// must preserve the settle budgets (wry URL-property pump 15s,
/// offscreen is_loading settle 60s) and the two-path screenshot
/// capture exactly.
pub trait PaneOps {
    /// First non-nil content pane id (offscreen iteration order) — the
    /// fallback when the wm reports the chrome pane as active.
    fn first_content_pane(&self) -> Option<uuid::Uuid>;
    /// True when either manager owns a pane with this id.
    fn pane_exists(&self, id: &uuid::Uuid) -> bool;
    /// (url, title) for whichever manager owns the pane.
    fn pane_url_title(&self, id: &uuid::Uuid) -> Option<(String, String)>;
    /// url for whichever manager owns the pane.
    fn pane_url(&self, id: &uuid::Uuid) -> Option<String>;
    /// (id, url, title) for every offscreen pane.
    fn all_panes(&self) -> Vec<(uuid::Uuid, String, String)>;
    /// Navigate the pane owned by either manager, with that engine's
    /// deterministic settle semantics. Returns Some(is_offscreen) when
    /// handled, None when no manager owns the pane (phantom).
    fn navigate_settled(&mut self, id: &uuid::Uuid, url: &url::Url) -> Option<bool>;
    /// Set the user agent (offscreen panes only). Returns handled.
    fn set_user_agent(&mut self, id: &uuid::Uuid, ua: &str) -> bool;
    /// Set the viewport size (offscreen panes only). Returns handled.
    fn set_viewport(&mut self, id: &uuid::Uuid, w: i32, h: i32) -> bool;
    /// Capture the pane as a PNG data-URI. Offscreen: synchronous frame
    /// grab + encode. Wry: Canvas capture with the GTK pump, 10s budget.
    /// Error strings follow the established MCP response phrasing.
    fn screenshot_data_uri(&mut self, id: &uuid::Uuid) -> String;
    /// Dispatch JS against the active pane; the callback receives the
    /// decoded result (wry's JSON-encoded wrapper stripped). Returns
    /// dispatched (false = no pane owned).
    fn execute_js(
        &mut self,
        id: &uuid::Uuid,
        js: &str,
        on_result: Box<dyn Fn(String) + Send>,
    ) -> bool;
    /// Reset the offscreen manager (the workspace-restore arm's
    /// fresh-managers pattern).
    fn reset_offscreen(&mut self);
    /// Live WebKitGTK load state for the pane (the settle poll).
    fn webview_is_loading(&self, id: &uuid::Uuid) -> bool;
    /// Pump the event loop so pending load/frame events land.
    fn pump(&mut self);
}

/// Bootstrap both MCP transports (stdio + TCP). All-core types: the
/// headed/agent caller provides the shared bridge and session registry.
pub fn spawn_mcp_server(
    mcp_bridge: &McpBridge,
    registry: crate::mcp::bridge::SessionRegistry,
    policy: &str,
) {
    let mcp_state = mcp_bridge.state.clone();
    let mcp_command_tx = mcp_bridge.command_tx.clone();

    // Policy profile (Era-3 S38): the default ceiling for NEW sessions.
    let initial_caps = match policy {
        "strict" => Some(crate::mcp::bridge::SessionCaps {
            panes: None,
            tool_classes: Some(std::collections::HashSet::from(["read".to_string()])),
        }),
        "balanced" => Some(crate::mcp::bridge::SessionCaps {
            panes: None,
            tool_classes: Some(std::collections::HashSet::from([
                "read".to_string(),
                "mutate".to_string(),
            ])),
        }),
        _ => None, // permissive (default)
    };

    // TCP bridge (Era-2 S27): per-connection session tool sets are built
    // inside the accept loop from this shared state; the registry carries
    // each session's capability set.
    crate::mcp::tcp_bridge::start_tcp_mcp_server(
        mcp_state.clone(),
        mcp_command_tx.clone(),
        registry,
        initial_caps,
    );

    // A separate tool set backs the in-process stdio transport; both transports
    // therefore behave identically without sharing mutable state.
    let stdio_tools = crate::mcp::tools::create_tools(mcp_state, mcp_bridge.command_tx.clone());
    let mut mcp_server = crate::mcp::McpServer::new();
    for tool in stdio_tools {
        mcp_server.register_tool(tool);
    }

    let transport = crate::mcp::McpTransport::new(mcp_server);
    info!(
        "MCP server starting (stdio + TCP bridge on port {})",
        crate::mcp::MCP_TCP_PORT
    );
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create MCP tokio runtime");
        rt.block_on(async {
            if let Err(e) = tokio::task::spawn_blocking(move || transport.run_stdio()).await {
                warn!("MCP server error: {}", e);
            }
        });
    });
}
