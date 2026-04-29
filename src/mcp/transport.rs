use std::io::{BufRead, Write};
use tracing::{info, warn};

use super::server::{JsonRpcRequest, JsonRpcResponse, McpServer};

/// Transport layer for the MCP server.
/// Reads JSON-RPC requests from stdin, writes responses to stdout.
/// Designed to run on a background thread via tokio.
pub struct McpTransport {
    server: McpServer,
}

impl McpTransport {
    pub fn new(server: McpServer) -> Self {
        Self { server }
    }

    /// Process a single JSON-RPC message and return the response.
    pub fn handle_message(&self, message: &str) -> Option<String> {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return None;
        }
        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                warn!(target: "mcp", "Failed to parse JSON-RPC request: {}", e);
                let response = JsonRpcResponse::error(
                    None,
                    super::server::JsonRpcError::new(-32700, format!("Parse error: {}", e)),
                );
                return Some(serde_json::to_string(&response).unwrap_or_else(|e| {
                    tracing::error!("Failed to serialize MCP response: {}", e);
                    r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"},"id":null}"#.to_string()
                }));
            }
        };

        let response = self.server.handle_request(&request);
        Some(serde_json::to_string(&response).unwrap_or_else(|e| {
            tracing::error!("Failed to serialize MCP response: {}", e);
            r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"},"id":null}"#
                .to_string()
        }))
    }

    /// Run the MCP server on stdin/stdout (blocking).
    /// Call this from a background thread.
    pub fn run_stdio(&self) -> anyhow::Result<()> {
        info!(target: "mcp", "MCP server starting on stdio");

        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();

        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    warn!(target: "mcp", "Failed to read stdin: {}", e);
                    break;
                }
            };

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(response) = self.handle_message(line) {
                if let Err(e) = writeln!(stdout, "{}", response) {
                    warn!(target: "mcp", "Failed to write response: {}", e);
                    break;
                }
                if let Err(e) = stdout.flush() {
                    warn!(target: "mcp", "Failed to flush stdout: {}", e);
                    break;
                }
            }
        }

        info!(target: "mcp", "MCP server shutting down");
        Ok(())
    }

    /// Spawn the MCP server on a tokio background thread.
    pub fn spawn_background(server: McpServer) -> tokio::task::JoinHandle<anyhow::Result<()>> {
        tokio::spawn(async move {
            let transport = McpTransport::new(server);
            // Run on a blocking thread since it reads from stdin
            tokio::task::spawn_blocking(move || transport.run_stdio()).await?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tools::McpTool;
    use serde_json::Value;

    struct EchoTool;
    impl McpTool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echo back the input"
        }
        fn input_schema(&self) -> Value {
            serde_json::json!({"type": "object"})
        }
        fn execute(&self, args: &Value) -> anyhow::Result<String> {
            Ok(format!("echo: {}", args))
        }
    }

    fn make_server() -> McpServer {
        let mut server = McpServer::new();
        server.register_tool(Box::new(EchoTool));
        server
    }

    #[test]
    fn test_handle_initialize() {
        let transport = McpTransport::new(make_server());
        let msg = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
        let response = transport.handle_message(msg).unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&response).unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_handle_tools_list() {
        let transport = McpTransport::new(make_server());
        let msg = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let response = transport.handle_message(msg).unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&response).unwrap();
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "echo");
    }

    #[test]
    fn test_handle_tools_call() {
        let transport = McpTransport::new(make_server());
        let msg = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"msg":"hello"}}}"#;
        let response = transport.handle_message(msg).unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&response).unwrap();
        let text = &resp.result.unwrap()["content"][0]["text"];
        assert!(text.as_str().unwrap().contains("hello"));
    }

    #[test]
    fn test_invalid_json() {
        let transport = McpTransport::new(make_server());
        let response = transport.handle_message("not json").unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&response).unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32700);
    }

    #[test]
    fn test_empty_line_ignored() {
        let transport = McpTransport::new(make_server());
        assert!(transport.handle_message("").is_none());
        assert!(transport.handle_message("   ").is_none());
    }
}
