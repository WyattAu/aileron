//! TCP bridge for MCP commands between the standalone server and the running browser.
//!
//! The browser listens on TCP port 9543 and accepts line-delimited JSON-RPC MCP
//! requests. Every request is delegated to a fully-registered [`McpServer`], so
//! the standalone `aileron-mcp` binary and the in-process stdio transport share a
//! single source of truth for tool dispatch (the [`McpTool`] implementations in
//! [`crate::mcp::tools`]).
//!
//! Concurrency model: connections are served sequentially. The browser process
//! has exactly one local MCP client (the standalone binary), which itself emits
//! requests serially over stdio. A tool `execute()` may block on the main thread
//! via the command channel; serving connections sequentially therefore mirrors
//! the existing stdio semantics and avoids reordering.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use tracing::{info, warn};

use super::bridge::McpCommand;
use super::server::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpServer};
use super::tools::McpTool;

/// Default TCP port for the MCP bridge.
///
/// Chosen to be outside the ephemeral range and unlikely to collide with
/// common local services.
pub const MCP_TCP_PORT: u16 = 9543;

/// Bind loopback address (IPv4) for the MCP bridge. Exposed so the standalone
/// relay binary can construct the same address as the in-browser listener.
pub const MCP_TCP_BIND_ADDR: &str = "127.0.0.1";

/// Compile-time invariant: the bridge port is non-privileged (>1024) and
/// non-ephemeral (<49152). A bad edit here fails the build.
const _: () = {
    assert!(MCP_TCP_PORT > 1024);
    assert!(MCP_TCP_PORT < 49152);
};

/// Start the TCP MCP server that accepts JSON-RPC requests and dispatches them
/// through `tool_list` (already constructed with a clone of `command_tx`).
///
/// This runs inside the browser process on a dedicated thread. The returned
/// [`std::thread::JoinHandle`] is intentionally dropped by callers; the server
/// runs for the lifetime of the process.
///
/// `command_tx` is retained as a parameter so the bridge can issue fire-and-forget
/// lifecycle commands (currently none are required, but it keeps the signature
/// stable for future shutdown signalling).
#[allow(clippy::needless_pass_by_value)]
pub fn start_tcp_mcp_server(
    tool_list: Vec<Box<dyn McpTool + Send + Sync>>,
    _command_tx: mpsc::Sender<McpCommand>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind((MCP_TCP_BIND_ADDR, MCP_TCP_PORT)) {
            Ok(l) => l,
            Err(e) => {
                warn!(
                    "[tcp-mcp] Failed to bind to {}:{}: {e}",
                    MCP_TCP_BIND_ADDR, MCP_TCP_PORT
                );
                return;
            }
        };
        info!(
            "[tcp-mcp] Listening on {}:{}",
            MCP_TCP_BIND_ADDR, MCP_TCP_PORT
        );

        // Single registered server: the canonical dispatcher for all MCP methods,
        // including `tools/call`. This deliberately avoids re-implementing tool
        // dispatch here (see commit history / Phase 7 deduplication).
        let mut server = McpServer::new();
        for tool in tool_list {
            server.register_tool(tool);
        }

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => handle_tcp_connection(stream, &server),
                Err(e) => warn!("[tcp-mcp] Connection accept error: {e}"),
            }
        }
    })
}

/// Handle a single TCP connection from the standalone MCP server.
///
/// Each line on the stream is one JSON-RPC request; each response is written as a
/// single line. Delegates every request to `server.handle_request`, which is the
/// same path used by the stdio transport.
fn handle_tcp_connection(mut stream: TcpStream, server: &McpServer) {
    // A read half is needed for line iteration; the original `stream` is retained
    // as the write half. Cloning a `TcpStream` duplicates the file descriptor,
    // which is the documented way to obtain independent read/write halves for
    // blocking I/O without `shutdown`.
    let read_half = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            warn!("[tcp-mcp] Failed to clone stream for reading: {e}");
            return;
        }
    };
    let reader = BufReader::new(read_half);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                warn!("[tcp-mcp] Read error: {e}");
                break;
            }
        };

        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => server.handle_request(&request),
            Err(e) => {
                JsonRpcResponse::error(None, JsonRpcError::new(-32700, format!("Parse error: {e}")))
            }
        };

        let resp_str = serde_json::to_string(&response).unwrap_or_default();
        if let Err(e) = writeln!(stream, "{resp_str}") {
            warn!("[tcp-mcp] Write error: {e}");
            break;
        }
        if let Err(e) = stream.flush() {
            warn!("[tcp-mcp] Flush error: {e}");
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::Arc;
    use std::time::Duration;

    /// Safety net for connection reads/writes so a regression cannot hang the
    /// whole test suite indefinitely.
    const TEST_IO_TIMEOUT: Duration = Duration::from_secs(3);

    /// A deterministic tool used to exercise the bridge end-to-end over a real
    /// loopback TCP socket without depending on the full browser state.
    struct EchoTool;

    impl McpTool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes the `msg` argument back as text."
        }
        fn input_schema(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": { "msg": { "type": "string" } },
                "required": ["msg"]
            })
        }
        fn execute(&self, args: &Value) -> anyhow::Result<String> {
            let msg = args
                .get("msg")
                .and_then(|v| v.as_str())
                .unwrap_or("(empty)");
            Ok(format!("echo:{msg}"))
        }
    }

    fn build_server() -> McpServer {
        let mut server = McpServer::new();
        server.register_tool(Box::new(EchoTool));
        server
    }

    /// Round-trip one raw request line through the bridge and return the
    /// response line. Spins up an ephemeral loopback listener owned by a thread
    /// that holds the server behind an `Arc` (joined before return).
    ///
    /// The client shuts down its write half after sending so the server's
    /// connection loop observes EOF and terminates; otherwise `join()` would
    /// deadlock waiting for a connection the client still holds open.
    fn round_trip_raw(server: McpServer, raw_line: &str) -> String {
        let server = Arc::new(server);
        let listener = TcpListener::bind((MCP_TCP_BIND_ADDR, 0)).expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let server_clone = Arc::clone(&server);
        let handle = std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let _ = stream.set_read_timeout(Some(TEST_IO_TIMEOUT));
                let _ = stream.set_write_timeout(Some(TEST_IO_TIMEOUT));
                handle_tcp_connection(stream, &server_clone);
            }
        });

        let mut client = TcpStream::connect(addr).expect("connect");
        client.set_read_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
        client.set_write_timeout(Some(TEST_IO_TIMEOUT)).unwrap();
        writeln!(client, "{raw_line}").unwrap();
        client.flush().unwrap();
        // Signal EOF to the server so its read loop terminates after it replies;
        // the read half stays usable for the response below.
        client
            .shutdown(std::net::Shutdown::Write)
            .expect("shutdown write");

        let reader = BufReader::new(client);
        let resp_line = reader
            .lines()
            .next()
            .expect("one response line")
            .expect("read ok");
        handle.join().unwrap();
        drop(server);
        resp_line
    }

    fn round_trip(server: McpServer, request: &JsonRpcRequest) -> JsonRpcResponse {
        let req_str = serde_json::to_string(request).unwrap();
        let line = round_trip_raw(server, &req_str);
        serde_json::from_str(&line).unwrap()
    }

    #[test]
    fn tools_list_is_dispatched_to_server() {
        let server = build_server();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "tools/list".into(),
            params: None,
        };
        let resp = round_trip(server, &request);
        assert!(resp.error.is_none());
        let tools = resp.result.unwrap()["tools"].as_array().unwrap().clone();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "echo");
    }

    #[test]
    fn tools_call_is_dispatched_to_registered_tool() {
        let server = build_server();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(2.into())),
            method: "tools/call".into(),
            params: Some(serde_json::json!({
                "name": "echo",
                "arguments": { "msg": "hello" }
            })),
        };
        let resp = round_trip(server, &request);
        assert!(resp.error.is_none());
        let text = &resp.result.unwrap()["content"][0]["text"];
        assert_eq!(text, "echo:hello");
    }

    #[test]
    fn initialize_is_dispatched_to_server() {
        let server = build_server();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(3.into())),
            method: "initialize".into(),
            params: Some(serde_json::json!({})),
        };
        let resp = round_trip(server, &request);
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let server = build_server();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(4.into())),
            method: "no/such/method".into(),
            params: None,
        };
        let resp = round_trip(server, &request);
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601, "JSON-RPC method-not-found code");
    }

    #[test]
    fn unknown_tool_returns_invalid_params() {
        let server = build_server();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(5.into())),
            method: "tools/call".into(),
            params: Some(serde_json::json!({ "name": "does_not_exist", "arguments": {} })),
        };
        let resp = round_trip(server, &request);
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32602, "JSON-RPC invalid-params code");
    }

    #[test]
    fn malformed_json_returns_parse_error() {
        let line = round_trip_raw(build_server(), "{ this is not json");
        let resp: JsonRpcResponse = serde_json::from_str(&line).unwrap();
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32700, "expected JSON-RPC parse error code");
    }
}
