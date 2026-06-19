//! Standalone MCP server binary for external client (e.g. OpenCode) integration.
//!
//! Architecture: this binary is a thin TCP relay. It reads line-delimited
//! JSON-RPC 2.0 requests from stdin, forwards each one to the running Aileron
//! browser over the loopback MCP bridge (see [`aileron::mcp::tcp_bridge`]), and
//! writes the line-delimited response to stdout. The browser process owns the
//! canonical [`aileron::mcp::server::McpServer`] with all tool implementations
//! registered, so dispatch behaviour is identical to the in-process stdio
//! transport.
//!
//! Each request opens a fresh TCP connection. This is deliberate: it lets the
//! standalone server tolerate browser restarts (e.g. crashes and `:crash-reload`)
//! without the MCP client having to reconnect, at negligible cost given MCP's
//! low request frequency.

use aileron::mcp::server::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use aileron::mcp::tcp_bridge::{MCP_TCP_BIND_ADDR, MCP_TCP_PORT};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("mcp=info,warn")
        .with_writer(std::io::stderr)
        .try_init();

    let tcp_addr = format!("{MCP_TCP_BIND_ADDR}:{MCP_TCP_PORT}");
    eprintln!("[mcp-server] Starting standalone Aileron MCP relay -> {tcp_addr}");
    eprintln!("[mcp-server] Browser must be running (cargo run --bin aileron)");
    eprintln!("[mcp-server] Listening on stdin");

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[mcp-server] stdin read error: {e}");
                break;
            }
        };

        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(request) => forward_to_browser(&tcp_addr, &request),
            Err(e) => {
                JsonRpcResponse::error(None, JsonRpcError::new(-32700, format!("Parse error: {e}")))
            }
        };

        let resp_str = serde_json::to_string(&response).unwrap_or_default();
        if let Err(e) = writeln!(stdout, "{resp_str}") {
            eprintln!("[mcp-server] stdout write error: {e}");
            break;
        }
        if let Err(e) = stdout.flush() {
            eprintln!("[mcp-server] stdout flush error: {e}");
            break;
        }
    }
}

/// Forward one JSON-RPC request to the browser bridge and read one response line.
///
/// Any I/O or transport error is mapped to a JSON-RPC error response so the
/// client always receives a well-formed reply on its single-line channel.
fn forward_to_browser(tcp_addr: &str, request: &JsonRpcRequest) -> JsonRpcResponse {
    let mut stream = match TcpStream::connect(tcp_addr) {
        Ok(s) => s,
        Err(e) => {
            return JsonRpcResponse::error(
                request.id.clone(),
                JsonRpcError::internal_error(format!(
                    "Cannot connect to browser at {tcp_addr}: {e}. Is the browser running?"
                )),
            );
        }
    };

    let req_str = match serde_json::to_string(request) {
        Ok(s) => s,
        Err(e) => {
            return JsonRpcResponse::error(
                request.id.clone(),
                JsonRpcError::internal_error(format!("Failed to serialize request: {e}")),
            );
        }
    };

    if let Err(e) = writeln!(stream, "{req_str}") {
        return JsonRpcResponse::error(
            request.id.clone(),
            JsonRpcError::internal_error(format!("TCP write error: {e}")),
        );
    }
    if let Err(e) = stream.flush() {
        return JsonRpcResponse::error(
            request.id.clone(),
            JsonRpcError::internal_error(format!("TCP flush error: {e}")),
        );
    }

    // Clone the stream to obtain an independent read half for line reading.
    let read_half = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            return JsonRpcResponse::error(
                request.id.clone(),
                JsonRpcError::internal_error(format!("TCP clone error: {e}")),
            );
        }
    };
    let mut reader = BufReader::new(read_half);
    let mut response_line = String::new();

    match reader.read_line(&mut response_line) {
        Ok(0) => JsonRpcResponse::error(
            request.id.clone(),
            JsonRpcError::internal_error("Browser closed the MCP bridge connection"),
        ),
        Ok(_) => match serde_json::from_str::<JsonRpcResponse>(response_line.trim()) {
            Ok(resp) => resp,
            Err(e) => JsonRpcResponse::error(
                request.id.clone(),
                JsonRpcError::internal_error(format!("Malformed browser response: {e}")),
            ),
        },
        Err(e) => JsonRpcResponse::error(
            request.id.clone(),
            JsonRpcError::internal_error(format!("TCP read error: {e}")),
        ),
    }
}
