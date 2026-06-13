//! Standalone MCP server binary for OpenCode integration.
//!
//! Registers all Aileron MCP tools and runs on stdio.
//! Tools that need the main thread (navigate, execute_js, etc.)
//! return mock responses in standalone mode.

use aileron::mcp::bridge::McpState;
use aileron::mcp::server::McpServer;
use aileron::mcp::tools::create_tools;
use aileron::mcp::transport::McpTransport;

fn main() {
    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_env_filter("mcp=info,warn")
        .with_writer(std::io::stderr)
        .try_init();

    eprintln!("[mcp-server] Starting standalone Aileron MCP server");

    let state = McpState::default();
    let (command_tx, command_rx) = std::sync::mpsc::channel();

    // Drop the receiver immediately so tool send() calls fail fast
    // instead of hanging on blocking_recv() when no main thread processes commands.
    drop(command_rx);

    let tools = create_tools(state, command_tx);
    let tool_count = tools.len();

    let mut server = McpServer::new();
    for tool in tools {
        server.register_tool(tool);
    }

    eprintln!("[mcp-server] Registered {tool_count} tools, listening on stdin");

    let transport = McpTransport::new(server);
    if let Err(e) = transport.run_stdio() {
        eprintln!("[mcp-server] Error: {e}");
        std::process::exit(1);
    }
}
