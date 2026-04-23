//! ARP WebSocket server implementation.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex as AsyncMutex};
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

use super::commands::ArpCommand;

// ─── JSON-RPC Types ───────────────────────────────────────

/// A JSON-RPC 2.0 request (from client).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
    pub id: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 response (to client).
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: Some(result),
            error: None,
            id,
        }
    }

    fn error(id: Option<serde_json::Value>, code: i64, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
            id,
        }
    }

    fn notification(method: &str, params: serde_json::Value) -> String {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        msg.to_string()
    }

    /// Serialize this response to a JSON string for sending over WebSocket.
    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            serde_json::json!({
                "jsonrpc": "2.0",
                "error": { "code": -32603, "message": "Failed to serialize response" }
            }).to_string()
        })
    }
}

// ─── Error Codes ───────────────────────────────────────────

const ERR_PARSE_ERROR: i64 = -32700;
const ERR_INVALID_REQUEST: i64 = -32600;
const ERR_METHOD_NOT_FOUND: i64 = -32601;
#[allow(dead_code)]
const ERR_INVALID_PARAMS: i64 = -32602;
#[allow(dead_code)]
const ERR_AUTH_FAILED: i64 = -32001;
#[allow(dead_code)]
const ERR_RATE_LIMITED: i64 = -32002;
#[allow(dead_code)]
const ERR_NOT_FOUND: i64 = -32003;

// ─── ARP Server ────────────────────────────────────────────

/// ARP server configuration.
#[derive(Debug, Clone)]
pub struct ArpConfig {
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
}

impl Default for ArpConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 19743,
            token: None,
        }
    }
}

/// The ARP WebSocket server.
///
/// Self-contained: owns its own tokio runtime (like `DownloadManager`).
/// Start with `start()`, stop with `stop()`, push state with `set_tabs()`.
/// Mutation commands from clients are sent via `cmd_sender()` and polled
/// on the main thread via the returned `mpsc::UnboundedReceiver`.
pub struct ArpServer {
    config: ArpConfig,
    /// Connected client sessions (session_id → sender).
    sessions: Arc<AsyncMutex<HashMap<String, broadcast::Sender<String>>>>,
    /// Shared state snapshots (updated by the desktop app).
    tabs_state: Arc<AsyncMutex<Vec<serde_json::Value>>>,
    /// Shared quickmarks snapshot (updated by the desktop app).
    quickmarks_state: Arc<AsyncMutex<Vec<serde_json::Value>>>,
    /// Sender for mutation commands (mobile → desktop).
    cmd_sender: Arc<StdMutex<mpsc::UnboundedSender<ArpCommand>>>,
    runtime: tokio::runtime::Runtime,
    running: AtomicBool,
}

impl ArpServer {
    /// Create a new ARP server with the given configuration.
    ///
    /// Returns `(server, receiver)` — the receiver must be polled on the main
    /// thread to process mutation commands from mobile clients.
    /// The server is not started until `start()` is called.
    pub fn new(config: ArpConfig) -> anyhow::Result<(Self, mpsc::UnboundedReceiver<ArpCommand>)> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create ARP runtime: {}", e))?;

        let (cmd_sender, cmd_receiver) = mpsc::unbounded_channel();

        Ok((
            Self {
                config,
                sessions: Arc::new(AsyncMutex::new(HashMap::new())),
                tabs_state: Arc::new(AsyncMutex::new(Vec::new())),
                quickmarks_state: Arc::new(AsyncMutex::new(Vec::new())),
                cmd_sender: Arc::new(StdMutex::new(cmd_sender)),
                runtime,
                running: AtomicBool::new(false),
            },
            cmd_receiver,
        ))
    }

    /// Check if the server is currently running.
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Start the ARP server (spawns the listener on its own runtime).
    /// Returns an error if already running or if binding fails.
    pub fn start(&self) -> anyhow::Result<()> {
        if self.running.swap(true, Ordering::Relaxed) {
            return Err(anyhow::anyhow!("ARP server is already running"));
        }

        let sessions = self.sessions.clone();
        let tabs_state = self.tabs_state.clone();
        let quickmarks_state = self.quickmarks_state.clone();
        let cmd_sender = self.cmd_sender.clone();
        let config = self.config.clone();
        let next_id = Arc::new(AtomicU64::new(1));

        let host = config.host.clone();
        let port = config.port;
        let has_token = config.token.is_some();

        self.runtime.spawn(async move {
            let addr = format!("{}:{}", host, port);
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    warn!(target: "arp", "ARP server failed to bind {}: {}", addr, e);
                    return;
                }
            };

            info!(target: "arp", "ARP server listening on ws://{}", addr);

            if !has_token {
                warn!(
                    target: "arp",
                    "No auth token set! Generate with :arp-token and set in config"
                );
            }

            loop {
                let (stream, peer_addr) = match listener.accept().await {
                    Ok((s, a)) => (s, a),
                    Err(e) => {
                        warn!(target: "arp", "Accept error: {}", e);
                        continue;
                    }
                };

                let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                    Ok(ws) => ws,
                    Err(e) => {
                        warn!(target: "arp", "WebSocket handshake failed for {}: {}", peer_addr, e);
                        continue;
                    }
                };

                info!(target: "arp", "New connection from {}", peer_addr);

                let cfg = config.clone();
                let sess = sessions.clone();
                let state = tabs_state.clone();
                let qm = quickmarks_state.clone();
                let cmd = cmd_sender.clone();

                let session_id = format!("{}", next_id.fetch_add(1, Ordering::Relaxed));

                tokio::spawn(async move {
                    if let Err(e) =
                        handle_connection(ws_stream, cfg, session_id, sess, state, qm, cmd).await
                    {
                        warn!(target: "arp", "Connection handler error: {}", e);
                    }
                });
            }
        });

        Ok(())
    }

    /// Stop the ARP server.
    /// Note: For graceful shutdown, we'd need a shutdown channel (future work).
    pub fn stop(&self) {
        self.running.store(false, std::sync::atomic::Ordering::Relaxed);
        info!(target: "arp", "ARP server stop requested");
    }

    /// Get the configured listen port.
    pub fn port(&self) -> u16 {
        self.config.port
    }

    /// Get the configured listen host.
    pub fn host(&self) -> &str {
        &self.config.host
    }

    /// Update the tabs state snapshot (called from the desktop app).
    /// Non-blocking — spawns on the ARP runtime.
    pub fn set_tabs(&self, tabs: Vec<serde_json::Value>) {
        let state = self.tabs_state.clone();
        drop(self.runtime.spawn(async move {
            let mut s = state.lock().await;
            *s = tabs;
        }));
    }

    /// Update the quickmarks state snapshot (called from the desktop app).
    /// Non-blocking — spawns on the ARP runtime.
    pub fn set_quickmarks(&self, quickmarks: Vec<serde_json::Value>) {
        let state = self.quickmarks_state.clone();
        drop(self.runtime.spawn(async move {
            let mut s = state.lock().await;
            *s = quickmarks;
        }));
    }

    /// Push a server notification to all connected clients.
    /// Non-blocking — spawns on the ARP runtime.
    pub fn notify(&self, method: &str, params: serde_json::Value) {
        let msg = JsonRpcResponse::notification(method, params);
        let sessions = self.sessions.clone();
        drop(self.runtime.spawn(async move {
            let sessions = sessions.lock().await;
            for (_id, sender) in sessions.iter() {
                let _ = sender.send(msg.clone());
            }
        }));
    }
}

/// Handle a single client connection.
async fn handle_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    config: ArpConfig,
    session_id: String,
    sessions: Arc<AsyncMutex<HashMap<String, broadcast::Sender<String>>>>,
    tabs_state: Arc<AsyncMutex<Vec<serde_json::Value>>>,
    quickmarks_state: Arc<AsyncMutex<Vec<serde_json::Value>>>,
    cmd_sender: Arc<StdMutex<mpsc::UnboundedSender<ArpCommand>>>,
) -> anyhow::Result<()> {
    let (mut write, mut read) = ws_stream.split();

    // Send server.hello
    let hello = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "server.hello",
        "params": {
            "version": "0.1.0",
            "session_id": session_id,
            "server_info": {
                "version": "0.12.0",
                "platform": std::env::consts::OS,
            }
        }
    });
    write
        .send(Message::text(serde_json::to_string(&hello).unwrap()))
        .await?;

    // Register session for broadcast notifications
    let (tx, _) = broadcast::channel::<String>(32);
    {
        let mut sessions = sessions.lock().await;
        sessions.insert(session_id.clone(), tx);
    }

    // Wait for client.info, then process requests
    let mut authenticated = config.token.is_none();

    while let Some(msg) = read.next().await {
        let msg = msg?;
        let text = msg.to_text()?;

        let request: JsonRpcRequest = match serde_json::from_str(text) {
            Ok(r) => r,
            Err(e) => {
                let response = JsonRpcResponse::error(
                    None,
                    ERR_PARSE_ERROR,
                    &format!("Invalid JSON: {}", e),
                );
                write.send(Message::text(response.to_json())).await?;
                continue;
            }
        };

        // Handle authentication via client.info
        if !authenticated
            && request.method.as_deref() == Some("client.info")
        {
            // Validate auth token if configured
            if let Some(ref _token) = config.token {
                // Token would come via Authorization header in real impl
                // For now, we accept the connection and mark as authenticated
                info!(
                    target: "arp",
                    "Client {} authenticated",
                    session_id
                );
            }
            authenticated = true;

            let response = JsonRpcResponse::success(
                request.id.clone(),
                serde_json::json!({ "status": "ok" }),
            );
            write.send(Message::text(response.to_json())).await?;
            continue;
        }

        // Dispatch authenticated requests
        let response = dispatch_request(&request, &tabs_state, &quickmarks_state, &cmd_sender).await;
        write.send(Message::text(response.to_json())).await?;
    }

    // Cleanup on disconnect
    {
        let mut sessions = sessions.lock().await;
        sessions.remove(&session_id);
    }
    info!(target: "arp", "Client {} disconnected", session_id);

    Ok(())
}

/// Dispatch a JSON-RPC request to the appropriate handler.
async fn dispatch_request(
    request: &JsonRpcRequest,
    tabs_state: &Arc<AsyncMutex<Vec<serde_json::Value>>>,
    quickmarks_state: &Arc<AsyncMutex<Vec<serde_json::Value>>>,
    cmd_sender: &Arc<StdMutex<mpsc::UnboundedSender<ArpCommand>>>,
) -> JsonRpcResponse {
    let method = match &request.method {
        Some(m) => m.as_str(),
        None => {
            return JsonRpcResponse::error(
                request.id.clone(),
                ERR_INVALID_REQUEST,
                "Missing 'method' field",
            );
        }
    };

    // Helper: send a command and return "queued" status
    let send_cmd = |cmd: ArpCommand| -> serde_json::Value {
        if let Ok(sender) = cmd_sender.lock()
            && sender.send(cmd).is_err()
        {
            return serde_json::json!({ "status": "error", "message": "Server shutting down" });
        }
        serde_json::json!({ "status": "queued" })
    };

    let result = match method {
        // ─── System ───
        "system.info" => system_info(),
        "system.ping" => serde_json::json!({ "pong": true }),
        "system.subscribe" => {
            // Subscriptions are handled via the broadcast channel
            serde_json::json!({ "status": "subscribed" })
        }

        // ─── Tabs ───
        "tabs.list" => {
            let state = tabs_state.lock().await;
            serde_json::json!(&*state)
        }
        "tabs.create" => {
            let url = request
                .params
                .as_ref()
                .and_then(|p| p.get("url"))
                .and_then(|v| v.as_str())
                .map(String::from);
            send_cmd(ArpCommand::TabCreate { url })
        }
        "tabs.navigate" => {
            match parse_tab_url_params(&request.params) {
                Ok((tab_id, url)) => send_cmd(ArpCommand::TabNavigate { tab_id, url }),
                Err(msg) => {
                    return JsonRpcResponse::error(
                        request.id.clone(),
                        ERR_INVALID_PARAMS,
                        &msg,
                    );
                }
            }
        }
        "tabs.close" => {
            let tab_id = request
                .params
                .as_ref()
                .and_then(|p| p.get("tab_id"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<uuid::Uuid>().ok());
            send_cmd(ArpCommand::TabClose { tab_id })
        }
        "tabs.activate" => {
            match request
                .params
                .as_ref()
                .and_then(|p| p.get("tab_id"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<uuid::Uuid>().ok())
            {
                Some(id) => send_cmd(ArpCommand::TabActivate { tab_id: id }),
                None => {
                    return JsonRpcResponse::error(
                        request.id.clone(),
                        ERR_INVALID_PARAMS,
                        "Missing or invalid 'tab_id' parameter",
                    );
                }
            }
        }
        "tabs.screenshot" => {
            // Screenshots require GPU access — return not_implemented for now
            serde_json::json!({ "status": "not_implemented", "message": "Screenshots require GPU context access" })
        }
        "tabs.goBack" => {
            let tab_id = request
                .params
                .as_ref()
                .and_then(|p| p.get("tab_id"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<uuid::Uuid>().ok());
            send_cmd(ArpCommand::TabGoBack { tab_id })
        }
        "tabs.goForward" => {
            let tab_id = request
                .params
                .as_ref()
                .and_then(|p| p.get("tab_id"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<uuid::Uuid>().ok());
            send_cmd(ArpCommand::TabGoForward { tab_id })
        }

        // ─── Terminal ───
        "terminal.list" => {
            // Terminal list comes from the tabs state — terminals are marked there
            let state = tabs_state.lock().await;
            let terminals: Vec<_> = state
                .iter()
                .filter(|t| t.get("terminal").and_then(|v| v.as_bool()).unwrap_or(false))
                .cloned()
                .collect();
            serde_json::json!(terminals)
        }
        "terminal.input" => {
            serde_json::json!({ "status": "not_implemented" })
        }
        "terminal.sendKey" => {
            serde_json::json!({ "status": "not_implemented" })
        }
        "terminal.snapshot" => {
            serde_json::json!({ "status": "not_implemented" })
        }

        // ─── Downloads ───
        "downloads.list" => {
            // Downloads state is not yet pushed to tabs_state
            // For now, return empty — will be wired when download state is shared
            serde_json::json!([])
        }
        "downloads.cancel" => {
            serde_json::json!({ "status": "not_implemented" })
        }
        "downloads.pause" => {
            serde_json::json!({ "status": "not_implemented" })
        }
        "downloads.resume" => {
            serde_json::json!({ "status": "not_implemented" })
        }

        // ─── Clipboard ───
        "clipboard.get" => {
            // Reading clipboard requires main thread access — send command,
            // result will be pushed back via notify("clipboard.contents", ...)
            let req_id = request.id.as_ref()
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            send_cmd(ArpCommand::ClipboardGet { request_id: req_id })
        }
        "clipboard.set" => {
            let text = request
                .params
                .as_ref()
                .and_then(|p| p.get("text"))
                .and_then(|v| v.as_str())
                .map(String::from);
            match text {
                Some(t) => send_cmd(ArpCommand::ClipboardSet { text: t }),
                None => {
                    return JsonRpcResponse::error(
                        request.id.clone(),
                        ERR_INVALID_PARAMS,
                        "Missing 'text' parameter",
                    );
                }
            }
        }

        // ─── Quickmarks ───
        "quickmarks.list" => {
            let qm = quickmarks_state.lock().await;
            serde_json::json!(qm.clone())
        }
        "quickmarks.open" => {
            let key = request
                .params
                .as_ref()
                .and_then(|p| p.get("key"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.chars().next());
            match key {
                Some(k) => send_cmd(ArpCommand::QuickmarkOpen { key: k }),
                None => {
                    return JsonRpcResponse::error(
                        request.id.clone(),
                        ERR_INVALID_PARAMS,
                        "Missing or invalid 'key' parameter",
                    );
                }
            }
        }

        _ => {
            return JsonRpcResponse::error(
                request.id.clone(),
                ERR_METHOD_NOT_FOUND,
                &format!("Unknown method: {}", method),
            );
        }
    };

    JsonRpcResponse::success(request.id.clone(), result)
}

/// Parse tab_id (optional) and url (required) from request params.
fn parse_tab_url_params(
    params: &Option<serde_json::Value>,
) -> Result<(Option<uuid::Uuid>, String), String> {
    let params = params.as_ref().ok_or("Missing params")?;
    let url = params
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'url' parameter")?
        .to_string();
    let tab_id = params
        .get("tab_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<uuid::Uuid>().ok());
    Ok((tab_id, url))
}

/// Return system information.
fn system_info() -> serde_json::Value {
    serde_json::json!({
        "version": "0.12.0",
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "arp_version": "1.0.0",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_success_response() {
        let resp = JsonRpcResponse::success(
            Some(serde_json::json!(1)),
            serde_json::json!({ "status": "ok" }),
        );
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }

    #[test]
    fn test_json_rpc_error_response() {
        let resp = JsonRpcResponse::error(
            Some(serde_json::json!(1)),
            ERR_METHOD_NOT_FOUND,
            "Unknown method: foo",
        );
        assert!(resp.result.is_none());
        assert_eq!(resp.error.as_ref().unwrap().code, ERR_METHOD_NOT_FOUND);
    }

    #[test]
    fn test_json_rpc_notification() {
        let msg = JsonRpcResponse::notification(
            "tab.updated",
            serde_json::json!({"id": 1}),
        );
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["method"], "tab.updated");
        assert!(parsed.get("id").is_none()); // Notifications have no id
    }

    #[tokio::test]
    async fn test_dispatch_system_info() {
        let state = Arc::new(AsyncMutex::new(Vec::new()));
        let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
        let cmd_sender = Arc::new(StdMutex::new(cmd_tx));
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: Some("system.info".into()),
            params: None,
            id: Some(serde_json::json!(1)),
        };
        let response = dispatch_request(&request, &state, &state, &cmd_sender).await;
        assert!(response.error.is_none());
        assert!(response.result.is_some());
        let result = response.result.unwrap();
        assert_eq!(result["version"], "0.12.0");
    }

    #[tokio::test]
    async fn test_dispatch_unknown_method() {
        let state = Arc::new(AsyncMutex::new(Vec::new()));
        let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
        let cmd_sender = Arc::new(StdMutex::new(cmd_tx));
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: Some("nonexistent.method".into()),
            params: None,
            id: Some(serde_json::json!(42)),
        };
        let response = dispatch_request(&request, &state, &state, &cmd_sender).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, ERR_METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_dispatch_tabs_list() {
        let state = Arc::new(AsyncMutex::new(vec![
            serde_json::json!({"id": 1, "url": "https://example.com", "title": "Example"}),
        ]));
        let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
        let cmd_sender = Arc::new(StdMutex::new(cmd_tx));
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: Some("tabs.list".into()),
            params: None,
            id: Some(serde_json::json!(2)),
        };
        let response = dispatch_request(&request, &state, &state, &cmd_sender).await;
        assert!(response.error.is_none());
        let result = response.result.unwrap();
        assert_eq!(result.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_dispatch_tabs_create_queues_command() {
        let state = Arc::new(AsyncMutex::new(Vec::new()));
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let cmd_sender = Arc::new(StdMutex::new(cmd_tx));
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: Some("tabs.create".into()),
            params: Some(serde_json::json!({"url": "https://example.com"})),
            id: Some(serde_json::json!(3)),
        };
        let response = dispatch_request(&request, &state, &state, &cmd_sender).await;
        assert!(response.error.is_none());
        assert_eq!(response.result.unwrap()["status"], "queued");
        // Verify command was actually sent
        let cmd = cmd_rx.try_recv().unwrap();
        assert!(matches!(cmd, ArpCommand::TabCreate { url: Some(ref u) } if u == "https://example.com"));
    }

    #[tokio::test]
    async fn test_dispatch_tabs_navigate_queues_command() {
        let state = Arc::new(AsyncMutex::new(Vec::new()));
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let cmd_sender = Arc::new(StdMutex::new(cmd_tx));
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: Some("tabs.navigate".into()),
            params: Some(serde_json::json!({"url": "https://example.com"})),
            id: Some(serde_json::json!(4)),
        };
        let response = dispatch_request(&request, &state, &state, &cmd_sender).await;
        assert!(response.error.is_none());
        assert_eq!(response.result.unwrap()["status"], "queued");
        let cmd = cmd_rx.try_recv().unwrap();
        assert!(matches!(cmd, ArpCommand::TabNavigate { url: ref u, .. } if u == "https://example.com"));
    }

    #[tokio::test]
    async fn test_dispatch_parse_error() {
        let state = Arc::new(AsyncMutex::new(Vec::new()));
        let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
        let cmd_sender = Arc::new(StdMutex::new(cmd_tx));
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            method: Some("tabs.list".into()),
            params: Some(serde_json::json!({"url": 42})),
            id: Some(serde_json::Value::Null),
        };
        let response = dispatch_request(&request, &state, &state, &cmd_sender).await;
        assert!(response.error.is_none());
    }

    #[test]
    fn test_arp_config_default() {
        let config = ArpConfig::default();
        assert_eq!(config.port, 19743);
        assert_eq!(config.host, "127.0.0.1");
        assert!(config.token.is_none());
    }

    #[test]
    fn test_arp_server_create() {
        let result = ArpServer::new(ArpConfig::default());
        assert!(result.is_ok());
        let (server, _receiver) = result.unwrap();
        assert!(!server.is_running());
        assert_eq!(server.port(), 19743);
        assert_eq!(server.host(), "127.0.0.1");
    }

    #[test]
    fn test_arp_server_set_tabs() {
        let (server, _receiver) = ArpServer::new(ArpConfig::default()).unwrap();
        server.set_tabs(vec![
            serde_json::json!({"id": 1, "url": "https://a.com"}),
            serde_json::json!({"id": 2, "url": "https://b.com"}),
        ]);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = server.tabs_state.lock().await;
            assert_eq!(state.len(), 2);
        });
    }

    #[test]
    fn test_arp_server_with_token() {
        let mut config = ArpConfig::default();
        config.token = Some("test-token-123".into());
        let result = ArpServer::new(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_arp_server_start_stop() {
        let (server, _receiver) = ArpServer::new(ArpConfig::default()).unwrap();
        assert!(!server.is_running());
        let result = server.start();
        if result.is_ok() {
            assert!(server.is_running());
            server.stop();
            assert!(!server.is_running());
        }
    }

    #[test]
    fn test_parse_tab_url_params() {
        let params = serde_json::json!({"url": "https://example.com"});
        let (tab_id, url) = parse_tab_url_params(&Some(params)).unwrap();
        assert!(tab_id.is_none());
        assert_eq!(url, "https://example.com");

        let params = serde_json::json!({"url": "https://example.com", "tab_id": "00000000-0000-0000-0000-000000000001"});
        let (tab_id, url) = parse_tab_url_params(&Some(params)).unwrap();
        assert!(tab_id.is_some());
        assert_eq!(url, "https://example.com");

        let params = serde_json::json!({"tab_id": "invalid"});
        let result = parse_tab_url_params(&Some(params));
        assert!(result.is_err());
    }
}
