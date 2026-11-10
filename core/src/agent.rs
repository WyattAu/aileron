//! The aileron agent runtime (S4 stage 2).
//!
//! Currently hosts the MCP server bootstrap (stdio + TCP bridge). The
//! agent event loop moves here in stage 2c, once `process_mcp_commands`
//! crosses the crate boundary via the `PaneOps` callback trait — the
//! target layout gives `aileron-agent` = thin `aileron_core::agent::run()`.

use crate::mcp::bridge::McpBridge;
use tracing::{info, warn};

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
