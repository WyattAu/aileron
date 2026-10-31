//! Standalone Aileron MCP relay: bridges MCP clients speaking JSON-RPC over
//! stdio to the browser's loopback MCP TCP bridge (`aileron::mcp::tcp_bridge`).
//!
//! The relay holds ONE TCP connection to the browser for the lifetime of the
//! stdio session (Era-2, YP-MCP-SESSIONS-002): the connection IS the session,
//! and its capability set (`session_scope`) applies to every request on it.
//! On I/O failure the relay reconnects once per request and retries - browser
//! restarts (crash-reload, watchdog) are transparent, at the documented cost
//! that a retried mutation may have executed before the failure.

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

    // Persistent session connection: created lazily, reused across requests,
    // re-established (once per request) after any I/O failure.
    let mut conn: Option<(TcpStream, BufReader<TcpStream>)> = None;

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
            Ok(request) => forward_session(&tcp_addr, &mut conn, &request),
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

/// Ensure the session connection exists (or reconnect), then forward the
/// request over it. One reconnect+retry per request on I/O failure.
fn forward_session(
    tcp_addr: &str,
    conn: &mut Option<(TcpStream, BufReader<TcpStream>)>,
    request: &JsonRpcRequest,
) -> JsonRpcResponse {
    if conn.is_none() {
        match connect(tcp_addr) {
            Ok(c) => *conn = Some(c),
            Err(e) => {
                return JsonRpcResponse::error(
                    request.id.clone(),
                    JsonRpcError::internal_error(format!(
                        "Cannot connect to browser at {tcp_addr}: {e}. Is the browser running?"
                    )),
                );
            }
        }
    }

    // First attempt on the existing connection; on I/O failure, reconnect
    // once and retry (session re-attaches fresh - caps are per-connection
    // by design and must be re-applied with session_scope after a reconnect).
    let attempt = |conn: &mut Option<(TcpStream, BufReader<TcpStream>)>,
                   request: &JsonRpcRequest|
     -> Result<JsonRpcResponse, String> {
        let Some((stream, reader)) = conn.as_mut() else {
            return Err("no connection".into());
        };
        let req_str = serde_json::to_string(request)
            .map_err(|e| format!("Failed to serialize request: {e}"))?;
        writeln!(stream, "{req_str}").map_err(|e| format!("TCP write error: {e}"))?;
        stream
            .flush()
            .map_err(|e| format!("TCP flush error: {e}"))?;
        let mut response_line = String::new();
        let n = reader
            .read_line(&mut response_line)
            .map_err(|e| format!("TCP read error: {e}"))?;
        if n == 0 {
            return Err("Browser closed the MCP bridge connection".into());
        }
        serde_json::from_str::<JsonRpcResponse>(response_line.trim())
            .map_err(|e| format!("Malformed browser response: {e}"))
    };

    match attempt(conn, request) {
        Ok(resp) => resp,
        Err(first_err) => {
            *conn = None;
            if let Ok(c) = connect(tcp_addr) {
                *conn = Some(c);
            }
            match attempt(conn, request) {
                Ok(resp) => resp,
                Err(_) => JsonRpcResponse::error(
                    request.id.clone(),
                    JsonRpcError::internal_error(format!(
                        "Browser unreachable at {tcp_addr} ({first_err}); session re-attach: re-apply session_scope if scoped"
                    )),
                ),
            }
        }
    }
}

/// Open a session connection to the browser bridge.
fn connect(tcp_addr: &str) -> Result<(TcpStream, BufReader<TcpStream>), String> {
    let stream = TcpStream::connect(tcp_addr).map_err(|e| e.to_string())?;
    let read_half = stream.try_clone().map_err(|e| e.to_string())?;
    Ok((stream, BufReader::new(read_half)))
}
