//! MCP tool implementations for the Aileron Model Context Protocol server.
//!
//! Tools access pane state via McpBridge (shared state + command channel).

use crate::mcp::bridge::{CommandTx, McpCommand, McpState};
use serde_json::{Value, json};

#[allow(unused_imports)]
use {attohttpc, urlencoding};

/// Trait for MCP tools that can be called from an MCP client.
pub trait McpTool: Send + Sync {
    /// The tool name (used in tools/call requests).
    fn name(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's input parameters.
    fn input_schema(&self) -> Value;

    /// Execute the tool with the given arguments.
    fn execute(&self, args: &Value) -> anyhow::Result<String>;
}

/// Tool: Read the active pane's URL and title.
/// Uses shared state from McpBridge (no main thread blocking needed).
pub struct ReadActivePaneTool {
    state: McpState,
}

impl ReadActivePaneTool {
    pub fn new(state: McpState) -> Self {
        Self { state }
    }
}

impl McpTool for ReadActivePaneTool {
    fn name(&self) -> &str {
        "read_active_pane"
    }
    fn description(&self) -> &str {
        "Read the URL and title of the currently active browser pane"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    fn execute(&self, _args: &Value) -> anyhow::Result<String> {
        let url = self
            .state
            .active_url
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();
        let title = self
            .state
            .active_title
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();

        if url.is_empty() {
            Ok("No active pane.".into())
        } else {
            Ok(format!(
                "## Active Pane\n\n**URL:** {url}\n**Title:** {title}\n"
            ))
        }
    }
}

/// Tool: Structured observation of the active pane (YP-AGENT-ACI-001 tiers).
///
/// T0: url/title/epoch/loading — the staleness token agents bind actions to.
/// T3: readable text (adblock-filtered) + the T0 header.
/// T1/T2 (AX snapshots) arrive with the S8-S10 walker.
struct BrowserObserveTool {
    state: McpState,
    command_tx: CommandTx,
}

impl BrowserObserveTool {
    fn new(state: McpState, command_tx: CommandTx) -> Self {
        Self { state, command_tx }
    }

    fn t0(&self) -> String {
        use std::sync::atomic::Ordering;
        let url = self
            .state
            .active_url
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();
        let title = self
            .state
            .active_title
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();
        let epoch = self.state.active_epoch.load(Ordering::Relaxed);
        let loading = self.state.active_loading.load(Ordering::Relaxed);
        serde_json::json!({
            "url": url,
            "title": title,
            "epoch": epoch,
            "loading": loading,
        })
        .to_string()
    }
}

impl McpTool for BrowserObserveTool {
    fn name(&self) -> &str {
        "browser_observe"
    }
    fn description(&self) -> &str {
        "Structured page observation by tier: T0 = url/title/epoch/loading metadata (JSON, the staleness token for refs), T3 = readable text of the page (adblock-filtered) with the T0 header"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tier": {
                    "type": "string",
                    "enum": ["T0", "T3"],
                    "description": "Observation tier (default T0)"
                },
                "max_length": {
                    "type": "integer",
                    "description": "T3 only: maximum characters of text (default 10000)"
                }
            },
            "required": []
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let tier = args
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or("T0")
            .to_ascii_uppercase();
        match tier.as_str() {
            "T0" => Ok(self.t0()),
            "T3" => {
                let max_length = args
                    .get("max_length")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10_000);
                let code = format!(
                    "(function() {{ \
                        var text = document.body ? document.body.innerText : ''; \
                        if (text.length > {max_length}) {{ text = text.substring(0, {max_length}) + '... [truncated]'; }} \
                        return text; \
                    }})()"
                );
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                self.command_tx
                    .send(McpCommand::ExecuteJs { code, response_tx })
                    .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;
                let text = response_rx
                    .blocking_recv()
                    .map_err(|_| anyhow::anyhow!("Observation cancelled"))?;
                Ok(format!("{} (text follows)\n\n{}", self.t0(), text))
            }
            other => Err(anyhow::anyhow!(
                "Unknown tier {other}; supported: T0, T3 (T1/T2 ship with the S8 walker)"
            )),
        }
    }
}

/// Tool: Accessibility snapshot with epoch-stamped refs (YP-AGENT-ACI-001).
struct BrowserSnapshotTool {
    state: McpState,
    command_tx: CommandTx,
}

impl BrowserSnapshotTool {
    fn new(state: McpState, command_tx: CommandTx) -> Self {
        Self { state, command_tx }
    }
}

impl McpTool for BrowserSnapshotTool {
    fn name(&self) -> &str {
        "browser_snapshot"
    }
    fn description(&self) -> &str {
        "Accessibility snapshot of the active pane: interactive elements as a compact JSON list with epoch-stamped integer refs. Pass (epoch, ref) to browser_click/browser_type/etc; on STALE_EPOCH take a fresh snapshot. tier T1 = interactive+named, T2 = all interactive"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tier": {"type": "string", "enum": ["T1", "T2"], "description": "T1 (default): interactive+named only. T2: all interactive"},
                "max_refs": {"type": "integer", "description": "Cap on refs returned (default 150)"}
            },
            "required": []
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        use std::sync::atomic::Ordering;
        let tier = args.get("tier").and_then(|v| v.as_str()).unwrap_or("T1");
        let max_refs = args.get("max_refs").and_then(|v| v.as_u64()).unwrap_or(150) as usize;
        let epoch = self.state.active_epoch.load(Ordering::Relaxed);
        let code = crate::mcp::aci::walker_js(epoch, tier, max_refs);
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ExecuteJs { code, response_tx })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;
        let result = response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Snapshot cancelled"))?;
        if result.is_empty() {
            return Err(anyhow::anyhow!(
                "Snapshot returned empty (page still loading? retry)"
            ));
        }
        Ok(result)
    }
}

/// Shared dispatch for the ref-actuation tools: build the JS, send ExecuteJs,
/// decode the {ok, err} envelope, surface STALE_EPOCH with guidance.
fn dispatch_act(state: &McpState, command_tx: &CommandTx, code: String) -> anyhow::Result<String> {
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    command_tx
        .send(McpCommand::ExecuteJs { code, response_tx })
        .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;
    let result = response_rx
        .blocking_recv()
        .map_err(|_| anyhow::anyhow!("Actuation cancelled"))?;
    if result.contains("STALE_EPOCH") {
        use std::sync::atomic::Ordering;
        return Err(anyhow::anyhow!(
            "STALE_EPOCH: the page navigated since this snapshot (current epoch {}). Re-snapshot and retry with the new refs.",
            state.active_epoch.load(Ordering::Relaxed)
        ));
    }
    if result.contains("DETACHED") {
        return Err(anyhow::anyhow!(
            "DETACHED: the referenced element left the DOM (page re-rendered). Re-snapshot and retry."
        ));
    }
    Ok(result)
}

fn ref_args_schema() -> Value {
    json!({
        "epoch": {"type": "integer", "description": "Snapshot epoch the ref was taken at (from browser_snapshot / browser_observe T0)"},
        "ref": {"type": "integer", "description": "Element ref from browser_snapshot"}
    })
}

fn with_refs(mut schema: Value) -> Value {
    if let Some(props) = schema.get_mut("properties").and_then(|p| p.as_object_mut())
        && let Some(r) = ref_args_schema().as_object()
    {
        for (k, v) in r {
            props.insert(k.clone(), v.clone());
        }
    }
    if let Some(req) = schema.get_mut("required").and_then(|r| r.as_array_mut()) {
        req.push(json!("epoch"));
        req.push(json!("ref"));
    }
    schema
}

macro_rules! act_tool {
    ($tool:ident, $name:literal, $desc:literal, $schema:expr, |$self_id:ident, $args:ident| $code:expr) => {
        struct $tool {
            #[allow(dead_code)] // available to bodies; some tools are pure dispatch
            state: McpState,
            command_tx: CommandTx,
        }
        impl $tool {
            fn new(state: McpState, command_tx: CommandTx) -> Self {
                Self { state, command_tx }
            }
        }
        impl McpTool for $tool {
            fn name(&self) -> &str {
                $name
            }
            fn description(&self) -> &str {
                $desc
            }
            fn input_schema(&self) -> Value {
                $schema
            }
            fn execute(&self, $args: &Value) -> anyhow::Result<String> {
                let $self_id = self;
                $code
            }
        }
    };
}

act_tool!(
    BrowserClickTool,
    "browser_click",
    "Click the epoch-stamped element; fails closed on STALE_EPOCH/DETACHED",
    with_refs(json!({"type":"object","properties":{},"required":[]})),
    |self_, args| {
        let epoch = args.get("epoch").and_then(|v| v.as_u64()).unwrap_or(0);
        let ref_idx = args.get("ref").and_then(|v| v.as_u64()).unwrap_or(0);
        dispatch_act(
            &self_.state,
            &self_.command_tx,
            crate::mcp::aci::actuator_js("click", epoch, ref_idx),
        )
    }
);

act_tool!(
    BrowserHoverTool,
    "browser_hover",
    "Hover the epoch-stamped element (reveals menus/tooltips)",
    with_refs(json!({"type":"object","properties":{},"required":[]})),
    |self_, args| {
        let epoch = args.get("epoch").and_then(|v| v.as_u64()).unwrap_or(0);
        let ref_idx = args.get("ref").and_then(|v| v.as_u64()).unwrap_or(0);
        dispatch_act(
            &self_.state,
            &self_.command_tx,
            crate::mcp::aci::actuator_js("hover", epoch, ref_idx),
        )
    }
);

act_tool!(
    BrowserFocusTool,
    "browser_focus",
    "Focus the epoch-stamped element and scroll it into view",
    with_refs(json!({"type":"object","properties":{},"required":[]})),
    |self_, args| {
        let epoch = args.get("epoch").and_then(|v| v.as_u64()).unwrap_or(0);
        let ref_idx = args.get("ref").and_then(|v| v.as_u64()).unwrap_or(0);
        dispatch_act(
            &self_.state,
            &self_.command_tx,
            crate::mcp::aci::actuator_js("focus", epoch, ref_idx),
        )
    }
);

act_tool!(
    BrowserTypeTool,
    "browser_type",
    "Type text into the epoch-stamped input (native value setter + input/change events; frameworks see it as real typing)",
    with_refs(json!({"type":"object","properties":{
        "text": {"type":"string","description":"Text to enter"},
        "clear": {"type":"boolean","description":"Clear existing content first (default false)"},
        "submit": {"type":"boolean","description":"Press Enter after typing (default false)"}
    },"required":["text"]})),
    |self_, args| {
        let epoch = args.get("epoch").and_then(|v| v.as_u64()).unwrap_or(0);
        let ref_idx = args.get("ref").and_then(|v| v.as_u64()).unwrap_or(0);
        let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let clear = args.get("clear").and_then(|v| v.as_bool()).unwrap_or(false);
        let result = dispatch_act(
            &self_.state,
            &self_.command_tx,
            crate::mcp::aci::type_js(epoch, ref_idx, text, clear),
        )?;
        if args
            .get("submit")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            // Synthetic Enter events don't trigger native form submission
            // (untrusted events); submit explicitly via requestSubmit.
            let submit = dispatch_act(
                &self_.state,
                &self_.command_tx,
                crate::mcp::aci::submit_js(),
            )?;
            return Ok(format!(
                "{{\"type_result\":{result},\"submit_result\":{submit}}}"
            ));
        }
        Ok(result)
    }
);

act_tool!(
    BrowserPressKeyTool,
    "browser_press_key",
    "Press a key on the focused element (ENTER, TAB, ESCAPE, ARROW keys, single characters)",
    json!({"type":"object","properties":{
        "key": {"type":"string","description":"Key name, e.g. ENTER, TAB, ESCAPE, ArrowDown, a"}
    },"required":["key"]}),
    |self_, args| {
        let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("");
        dispatch_act(
            &self_.state,
            &self_.command_tx,
            crate::mcp::aci::key_js(key),
        )
    }
);

act_tool!(
    BrowserDragTool,
    "browser_drag",
    "Drag from one epoch-stamped element to another (mouse-event synthesis; HTML5 DnD handlers and canvas apps may need additional setup)",
    json!({"type":"object","properties":{},"required":[]}),
    |self_, args| {
        let se = args.get("epoch").and_then(|v| v.as_u64()).unwrap_or(0);
        let sr = args.get("ref").and_then(|v| v.as_u64()).unwrap_or(0);
        let ee = args.get("end_epoch").and_then(|v| v.as_u64()).unwrap_or(se);
        let er = args.get("end_ref").and_then(|v| v.as_u64()).unwrap_or(0);
        dispatch_act(
            &self_.state,
            &self_.command_tx,
            crate::mcp::aci::drag_js(se, sr, ee, er),
        )
    }
);

act_tool!(
    BrowserSelectTool,
    "browser_select",
    "Select option(s) on the epoch-stamped <select> by value or visible text",
    with_refs(json!({"type":"object","properties":{
        "values": {"type":"array","items":{"type":"string"},"description":"Values or visible texts to select"}
    },"required":["values"]})),
    |self_, args| {
        let epoch = args.get("epoch").and_then(|v| v.as_u64()).unwrap_or(0);
        let ref_idx = args.get("ref").and_then(|v| v.as_u64()).unwrap_or(0);
        let values: Vec<String> = args
            .get("values")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        dispatch_act(
            &self_.state,
            &self_.command_tx,
            crate::mcp::aci::select_js(epoch, ref_idx, &values),
        )
    }
);

act_tool!(
    BrowserBulkActTool,
    "bulk_act",
    "Execute up to 50 actions in ONE page evaluation with per-action status (74% fewer round-trips vs sequential; 2511.19477). Actions reference the last snapshot's refs: {op:'click'|'hover'|'focus', epoch, ref}, {op:'type', epoch, ref, text, clear?}, {op:'key', key}, {op:'select', epoch, ref, values[]}, {op:'submit'}",
    json!({"type":"object","properties":{
        "actions": {"type":"array","items":{"type":"object"},"description":"Ordered action list","minItems":1,"maxItems":50}
    },"required":["actions"]}),
    |self_, args| {
        let actions = args.get("actions").cloned().unwrap_or(Value::Array(vec![]));
        let actions_json = serde_json::to_string(&actions)?;
        dispatch_act(
            &self_.state,
            &self_.command_tx,
            crate::mcp::aci::bulk_js(&actions_json),
        )
    }
);

/// Tool: browser_wait_for - poll-side wait (no page-JV spin): evaluates a
/// cheap condition repeatedly from the tool thread until met or deadline.
struct BrowserWaitForTool {
    command_tx: CommandTx,
}
impl BrowserWaitForTool {
    fn new(_state: McpState, command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
    fn check(&self, contains: Option<&str>, gone: Option<&str>) -> anyhow::Result<bool> {
        let cond = match (contains, gone) {
            (Some(t), _) => format!(
                "(document.body ? document.body.innerText.includes({}) : false)",
                serde_json::to_string(t)?
            ),
            (_, Some(t)) => format!(
                "(document.body ? !document.body.innerText.includes({}) : true)",
                serde_json::to_string(t)?
            ),
            _ => "true".into(),
        };
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ExecuteJs {
                code: cond,
                response_tx,
            })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;
        let r = response_rx.blocking_recv().unwrap_or_default();
        Ok(r.trim() == "true" || r.trim() == "1")
    }
}
impl McpTool for BrowserWaitForTool {
    fn name(&self) -> &str {
        "browser_wait_for"
    }
    fn description(&self) -> &str {
        "Wait until text appears (text), disappears (textGone), or a timeout - whichever first. Polls the page from the browser side; use after actions that trigger async loads"
    }
    fn input_schema(&self) -> Value {
        json!({"type":"object","properties":{
            "text": {"type":"string","description":"Wait until this text appears"},
            "textGone": {"type":"string","description":"Wait until this text disappears"},
            "timeout_ms": {"type":"integer","description":"Deadline in ms (default 10000, max 30000)"}
        },"required":[]})
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let contains = args.get("text").and_then(|v| v.as_str());
        let gone = args.get("textGone").and_then(|v| v.as_str());
        if contains.is_none() && gone.is_none() {
            anyhow::bail!("provide 'text' or 'textGone'");
        }
        let timeout = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(10_000)
            .min(30_000);
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout);
        loop {
            if self.check(contains, gone)? {
                return Ok(format!(
                    "{{\"met\":true,\"elapsed_ms\":{}}}",
                    (std::time::Instant::now()
                        - (deadline - std::time::Duration::from_millis(timeout)))
                    .as_millis()
                ));
            }
            if std::time::Instant::now() >= deadline {
                return Ok(format!("{{\"met\":false,\"timeout_ms\":{timeout}}}"));
            }
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
    }
}

/// Shared page-ring reader: evaluates a JS snippet returning JSON.
fn read_page_ring(
    state: &McpState,
    command_tx: &CommandTx,
    code: String,
) -> anyhow::Result<String> {
    dispatch_act(state, command_tx, code)
}

act_tool!(
    BrowserConsoleTool,
    "browser_console_messages",
    "Console messages of the active page (page-side ring, cap 500; levels log/info/warn/error/debug). Captured since page load",
    json!({"type":"object","properties":{
        "level": {"type":"string","enum":["all","error","warn","info","log","debug"],"description":"Filter (default all)"}
    },"required":[]}),
    |self_, args| {
        let filter = args.get("level").and_then(|v| v.as_str()).unwrap_or("all");
        let code = if filter == "all" {
            "JSON.stringify(window.__aileron_console || [])".to_string()
        } else {
            format!(
                "JSON.stringify((window.__aileron_console || []).filter(m => m.level === {}))",
                serde_json::to_string(filter)?
            )
        };
        read_page_ring(&self_.state, &self_.command_tx, code)
    }
);

act_tool!(
    BrowserNetworkTool,
    "browser_network_requests",
    "Network activity of the active page (fetch + XHR ring, cap 500): url, method, status, kind, duration ms. Response bodies are not captured (cross-origin)",
    json!({"type":"object","properties":{
        "limit": {"type":"integer","description":"Return only the last N entries (default all)"}
    },"required":[]}),
    |self_, args| {
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(0);
        let code = if limit == 0 {
            "JSON.stringify(window.__aileron_net || [])".to_string()
        } else {
            format!("JSON.stringify((window.__aileron_net || []).slice(-{limit}))")
        };
        read_page_ring(&self_.state, &self_.command_tx, code)
    }
);

act_tool!(
    BrowserCookiesTool,
    "browser_cookies",
    "Cookies visible to the active page (document.cookie). NOTE: HttpOnly cookies are invisible to page JS by design - this shows exactly what the page can see",
    json!({"type":"object","properties":{},"required":[]}),
    |self_, args| {
        let _ = args;
        read_page_ring(
            &self_.state,
            &self_.command_tx,
            "JSON.stringify({cookie: document.cookie})".to_string(),
        )
    }
);

/// Tool: browser_downloads - list download tasks with progress.
struct BrowserDownloadsTool {
    command_tx: CommandTx,
}
impl BrowserDownloadsTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}
impl McpTool for BrowserDownloadsTool {
    fn name(&self) -> &str {
        "browser_downloads"
    }
    fn description(&self) -> &str {
        "List browser download tasks: id, state, filename, progress, url"
    }
    fn input_schema(&self) -> Value {
        json!({"type":"object","properties":{},"required":[]})
    }
    fn execute(&self, _args: &Value) -> anyhow::Result<String> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ListDownloads { response_tx })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;
        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Downloads query cancelled"))
    }
}

act_tool!(
    SessionScopeTool,
    "session_scope",
    "Narrow THIS session's capabilities (monotone - only subsets of the current grant are accepted; widening is structurally impossible). panes: allowed pane UUIDs; tool_classes: subset of read|mutate|irreversible. Applies to all subsequent calls on this connection",
    json!({"type":"object","properties":{
        "panes": {"type":"array","items":{"type":"string"},"description":"Allowed pane UUIDs (from list_tabs)"},
        "tool_classes": {"type":"array","items":{"type":"string","enum":["read","mutate","irreversible"]},"description":"Allowed tool classes"}
    },"required":[]}),
    |self_, args| {
        let panes = args.get("panes").and_then(|v| v.as_array()).map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().and_then(|s| uuid::Uuid::parse_str(s).ok()))
                .collect::<Vec<_>>()
        });
        let classes = args
            .get("tool_classes")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            });
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self_
            .command_tx
            .send(crate::mcp::bridge::McpCommand::SessionScope {
                panes,
                tool_classes: classes,
                response_tx,
            })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;
        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Session scope update cancelled"))
    }
);

act_tool!(
    SessionInfoTool,
    "session_info",
    "Report this connection's session id and capability set (panes scope, tool classes)",
    json!({"type":"object","properties":{},"required":[]}),
    |self_, args| {
        let _ = args;
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self_
            .command_tx
            .send(crate::mcp::bridge::McpCommand::SessionInfo { response_tx })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;
        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Session info cancelled"))
    }
);

act_tool!(
    WorkspaceCheckpointTool,
    "workspace_checkpoint",
    "Save the live pane tree (all panes + URLs) as a named checkpoint - the environment-memory primitive (AgentRewind pattern); restore via the workspaces UI or a later diff",
    json!({"type":"object","properties":{
        "name": {"type":"string","description":"Checkpoint name"}
    },"required":["name"]}),
    |self_, args| {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self_
            .command_tx
            .send(crate::mcp::bridge::McpCommand::WorkspaceCheckpoint { name, response_tx })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;
        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Checkpoint cancelled"))
    }
);

act_tool!(
    WorkspaceDiffTool,
    "workspace_diff",
    "Diff a saved checkpoint against the live pane tree: added/removed URLs, same flag",
    json!({"type":"object","properties":{
        "name": {"type":"string","description":"Checkpoint name"}
    },"required":["name"]}),
    |self_, args| {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self_
            .command_tx
            .send(crate::mcp::bridge::McpCommand::WorkspaceDiff { name, response_tx })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;
        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Diff cancelled"))
    }
);

act_tool!(
    WorkspaceListTool,
    "workspace_list",
    "List saved checkpoints/workspaces",
    json!({"type":"object","properties":{},"required":[]}),
    |self_, args| {
        let _ = args;
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self_
            .command_tx
            .send(crate::mcp::bridge::McpCommand::WorkspaceList { response_tx })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;
        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("List cancelled"))
    }
);

/// Tool: Search the web using DuckDuckGo HTML lite endpoint.
/// Performs a real HTTP request to DuckDuckGo's lite HTML interface
/// and extracts search result titles and URLs.
pub struct SearchWebTool;

impl McpTool for SearchWebTool {
    fn name(&self) -> &str {
        "search_web"
    }
    fn description(&self) -> &str {
        "Search the web using DuckDuckGo. Returns top results with titles and URLs."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                }
            },
            "required": ["query"]
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;

        // Use DuckDuckGo HTML lite endpoint (returns simple HTML, no JS required)
        let encoded = urlencoding::encode(query);
        let ddg_url = format!("https://duckduckgo.com/?q={encoded}");
        let url = format!("https://lite.duckduckgo.com/lite/?q={encoded}");

        let response = attohttpc::get(&url)
            .header("User-Agent", "Aileron/0.18 (MCP search tool)")
            .header("Accept", "text/html")
            .send();

        let body = match response {
            Ok(resp) => match resp.text() {
                Ok(text) => text,
                Err(e) => {
                    return Ok(format!(
                        "Search request failed (body read error): {e}\nFallback URL: {ddg_url}"
                    ));
                }
            },
            Err(e) => {
                return Ok(format!(
                    "Search request failed: {e}\nFallback URL: {ddg_url}"
                ));
            }
        };

        // Extract result links from DuckDuckGo lite HTML.
        // Lite results are in <a class="result-link" href="...">Title</a> tags,
        // and the main results table has <td class="result-link"> anchors.
        let mut results: Vec<String> = Vec::new();
        let mut seen_urls: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Parse simple <a href="URL">Title</a> patterns from result rows
        for line in body.lines() {
            // DuckDuckGo lite uses <a class="result-link" href="URL">
            if let Some(start) = line.find("href=\"") {
                let rest = &line[start + 6..];
                if let Some(end) = rest.find('"') {
                    let href = &rest[..end];
                    // Skip DuckDuckGo internal links and ads
                    if href.contains("duckduckgo.com")
                        || href.contains("ad.")
                        || href.starts_with('#')
                        || href.starts_with('/')
                    {
                        continue;
                    }
                    // Extract title from the link text
                    let title = if let Some(title_start) = line.find('>') {
                        let title_rest = &line[title_start + 1..];
                        if let Some(title_end) = title_rest.find("</a>") {
                            let t = title_rest[..title_end].trim();
                            // Decode HTML entities minimally
                            t.replace("&amp;", "&")
                                .replace("&lt;", "<")
                                .replace("&gt;", ">")
                                .replace("&quot;", "\"")
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    };

                    if seen_urls.insert(href.to_string()) && results.len() < 10 {
                        results.push(format!("{}. {} - {}", results.len() + 1, title, href));
                    }
                }
            }
        }

        if results.is_empty() {
            Ok(format!(
                "No results found for '{query}'.\nDirect search URL: {ddg_url}"
            ))
        } else {
            Ok(format!(
                "Search results for '{}':\n\n{}\n\nDirect search URL: {}",
                query,
                results.join("\n"),
                ddg_url
            ))
        }
    }
}

/// Tool: Extract visible text content from the active pane.
/// A convenience wrapper around `run_js` that returns `document.body.innerText`.
pub struct BrowserGetTextTool {
    command_tx: CommandTx,
}

impl BrowserGetTextTool {
    pub fn new(_state: McpState, command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for BrowserGetTextTool {
    fn name(&self) -> &str {
        "browser_get_text"
    }
    fn description(&self) -> &str {
        "Extract all visible text content from the active browser pane"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "max_length": {
                    "type": "integer",
                    "description": "Maximum number of characters to return (default 10000)",
                    "default": 10000
                }
            },
            "required": []
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let max_length = args
            .get("max_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(10000);

        // JS that extracts visible text and truncates to max_length
        let code = format!(
            "(function() {{ \
                var text = document.body ? document.body.innerText : ''; \
                if (text.length > {max_length}) {{ text = text.substring(0, {max_length}) + '... [truncated]'; }} \
                return text; \
            }})()"
        );

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ExecuteJs { code, response_tx })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;

        let result = response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Text extraction cancelled"))?;

        if result.is_empty() || result == "String(\"\")" {
            Ok("No visible text content found on the page.".into())
        } else {
            Ok(result)
        }
    }
}

// ── List Tabs Tool ──────────────────────────────────────────────

struct ListTabsTool {
    command_tx: CommandTx,
}

impl ListTabsTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for ListTabsTool {
    fn name(&self) -> &str {
        "list_tabs"
    }
    fn description(&self) -> &str {
        "List all open tabs with URLs and titles"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }
    fn execute(&self, _args: &serde_json::Value) -> anyhow::Result<String> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ListTabs { response_tx })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;
        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Response channel cancelled"))
    }
}

// ── Bookmark CRUD Tool ──────────────────────────────────────────

struct BookmarkCrudTool {
    command_tx: CommandTx,
}

impl BookmarkCrudTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for BookmarkCrudTool {
    fn name(&self) -> &str {
        "bookmark_crud"
    }
    fn description(&self) -> &str {
        "List, add, or remove bookmarks. Action: list/add/remove."
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["list", "add", "remove"], "description": "CRUD action" },
                "url": { "type": "string", "description": "URL (for add/remove)" },
                "title": { "type": "string", "description": "Title (for add)" },
                "folder": { "type": "string", "description": "Folder name (optional, for add)" },
            },
            "required": ["action"],
        })
    }
    fn execute(&self, args: &serde_json::Value) -> anyhow::Result<String> {
        let action = args["action"].as_str().unwrap_or("list");
        match action {
            "list" => {
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                self.command_tx
                    .send(McpCommand::ListBookmarks { response_tx })
                    .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;
                response_rx
                    .blocking_recv()
                    .map_err(|_| anyhow::anyhow!("Response channel cancelled"))
            }
            "add" => {
                let url = args["url"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'url'"))?;
                let title = args["title"].as_str().unwrap_or(url);
                let folder = args["folder"].as_str().unwrap_or("");
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                self.command_tx
                    .send(McpCommand::AddBookmark {
                        url: url.into(),
                        title: title.into(),
                        folder: folder.into(),
                        response_tx,
                    })
                    .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;
                response_rx
                    .blocking_recv()
                    .map_err(|_| anyhow::anyhow!("Response channel cancelled"))
            }
            "remove" => {
                let url = args["url"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'url'"))?;
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                self.command_tx
                    .send(McpCommand::RemoveBookmark {
                        url: url.into(),
                        response_tx,
                    })
                    .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;
                response_rx
                    .blocking_recv()
                    .map_err(|_| anyhow::anyhow!("Response channel cancelled"))
            }
            other => Err(anyhow::anyhow!("Unknown action: {other}")),
        }
    }
}

// ── History Search Tool ──────────────────────────────────────────

struct HistorySearchTool {
    command_tx: CommandTx,
}

impl HistorySearchTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for HistorySearchTool {
    fn name(&self) -> &str {
        "history_search"
    }
    fn description(&self) -> &str {
        "Search browsing history by query string"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Max results (default 10)" },
            },
            "required": ["query"],
        })
    }
    fn execute(&self, args: &serde_json::Value) -> anyhow::Result<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'query'"))?;
        let limit = args["limit"].as_u64().unwrap_or(10) as usize;
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::SearchHistory {
                query: query.into(),
                limit,
                response_tx,
            })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;
        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Response channel cancelled"))
    }
}

/// Tool: Fill a form field in the active pane via CSS selector.
/// Sets the value, dispatches input/change events so frameworks (React, Vue)
/// pick up the change, and optionally submits the form.
pub struct BrowserFillFormTool {
    command_tx: CommandTx,
}

impl BrowserFillFormTool {
    pub fn new(_state: McpState, command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for BrowserFillFormTool {
    fn name(&self) -> &str {
        "browser_fill_form"
    }
    fn description(&self) -> &str {
        "Fill a form field identified by a CSS selector with a given value"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector for the form element (e.g. '#username', 'input[name=\"email\"]', '.search-box')"
                },
                "value": {
                    "type": "string",
                    "description": "The value to fill into the form field"
                },
                "submit": {
                    "type": "boolean",
                    "description": "Whether to submit the form after filling (default false)",
                    "default": false
                }
            },
            "required": ["selector", "value"]
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let selector = args
            .get("selector")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'selector' parameter"))?;

        let value = args
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'value' parameter"))?;

        let submit = args
            .get("submit")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Escape strings for safe JS interpolation
        let escaped_selector = selector.replace('\\', "\\\\").replace('\'', "\\'");
        let escaped_value = value.replace('\\', "\\\\").replace('\'', "\\'");

        // JS that finds the element, sets its value, and fires events
        let code = if submit {
            format!(
                "(function() {{ \
                    var el = document.querySelector('{escaped_selector}'); \
                    if (!el) return 'Error: element not found'; \
                    el.value = '{escaped_value}'; \
                    el.dispatchEvent(new Event('input', {{ bubbles: true }})); \
                    el.dispatchEvent(new Event('change', {{ bubbles: true }})); \
                    var form = el.closest('form'); \
                    if (form) {{ form.submit(); return 'Filled and submitted'; }} \
                    return 'Filled (no form to submit)'; \
                }})()"
            )
        } else {
            format!(
                "(function() {{ \
                    var el = document.querySelector('{escaped_selector}'); \
                    if (!el) return 'Error: element not found'; \
                    el.value = '{escaped_value}'; \
                    el.dispatchEvent(new Event('input', {{ bubbles: true }})); \
                    el.dispatchEvent(new Event('change', {{ bubbles: true }})); \
                    return 'Filled'; \
                }})()"
            )
        };

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ExecuteJs { code, response_tx })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {e}"))?;

        let result = response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Form fill cancelled"))?;

        Ok(result)
    }
}

// ── Navigate Tool ──────────────────────────────────────────────

struct NavigateTool {
    command_tx: CommandTx,
}

impl NavigateTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for NavigateTool {
    fn name(&self) -> &str {
        "navigate"
    }
    fn description(&self) -> &str {
        "Navigate to a URL in the active or a new tab"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to navigate to" },
                "new_tab": { "type": "boolean", "default": false, "description": "Open in new tab instead of active" },
            },
            "required": ["url"],
        })
    }
    fn execute(&self, args: &serde_json::Value) -> anyhow::Result<String> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' parameter"))?;

        if url::Url::parse(url).is_err() {
            return Err(anyhow::anyhow!("Invalid URL: {url}"));
        }

        let new_tab = args
            .get("new_tab")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        self.command_tx
            .send(McpCommand::Navigate {
                url: url.to_string(),
                new_tab,
            })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        if new_tab {
            Ok(format!("Opening in new tab: {url}"))
        } else {
            Ok(format!("Navigating to: {url}"))
        }
    }
}

// ── Execute JS Tool ────────────────────────────────────────────

struct ExecuteJsTool {
    command_tx: CommandTx,
}

impl ExecuteJsTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for ExecuteJsTool {
    fn name(&self) -> &str {
        "execute_js"
    }
    fn description(&self) -> &str {
        "Execute JavaScript in the active tab and return the result"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "code": { "type": "string", "description": "JavaScript code to execute" },
            },
            "required": ["code"],
        })
    }
    fn execute(&self, args: &serde_json::Value) -> anyhow::Result<String> {
        let code = args
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'code' parameter"))?;

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ExecuteJs {
                code: code.to_string(),
                response_tx,
            })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("JS execution cancelled"))
    }
}

// ── Screenshot Tool ────────────────────────────────────────────

struct ScreenshotTool {
    command_tx: CommandTx,
}

impl ScreenshotTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for ScreenshotTool {
    fn name(&self) -> &str {
        "screenshot"
    }
    fn description(&self) -> &str {
        "Capture the active pane as a base64-encoded PNG image"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }
    fn execute(&self, _args: &serde_json::Value) -> anyhow::Result<String> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::Screenshot { response_tx })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;
        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Screenshot cancelled"))
    }
}

// ── Click Tool ────────────────────────────────────────────────

struct ClickTool {
    command_tx: CommandTx,
}

impl ClickTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for ClickTool {
    fn name(&self) -> &str {
        "click"
    }
    fn description(&self) -> &str {
        "Click an element on the page by CSS selector"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector of element to click" }
            },
            "required": ["selector"],
        })
    }
    fn execute(&self, args: &serde_json::Value) -> anyhow::Result<String> {
        let selector = args
            .get("selector")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'selector' parameter"))?;

        let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
        let code = format!(
            "(function() {{ \
                var el = document.querySelector('{escaped}'); \
                if (!el) return 'Error: element not found'; \
                el.click(); \
                return 'clicked'; \
            }})()"
        );

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ExecuteJs { code, response_tx })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;
        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Click cancelled"))
    }
}

// ── Get Cookies Tool ──────────────────────────────────────────

struct GetCookiesTool {
    command_tx: CommandTx,
}

impl GetCookiesTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for GetCookiesTool {
    fn name(&self) -> &str {
        "get_cookies"
    }
    fn description(&self) -> &str {
        "Read cookies for the active page domain"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "domain": { "type": "string", "description": "Optional domain filter (defaults to active page domain)" }
            },
        })
    }
    fn execute(&self, args: &serde_json::Value) -> anyhow::Result<String> {
        let code = "(function() { return document.cookie; })()".to_string();

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ExecuteJs { code, response_tx })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        let raw = response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Cookie read cancelled"))?;

        let domain_filter = args.get("domain").and_then(|v| v.as_str());

        let mut cookies: Vec<serde_json::Value> = Vec::new();
        for pair in raw.split(';') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            if let Some(eq_pos) = pair.find('=') {
                let name = pair[..eq_pos].trim().to_string();
                let value = pair[eq_pos + 1..].trim().to_string();
                if let Some(filter) = domain_filter
                    && !name.contains(filter)
                    && !value.contains(filter)
                {
                    continue;
                }
                cookies.push(serde_json::json!({ "name": name, "value": value }));
            }
        }

        if cookies.is_empty() {
            Ok("No cookies found.".to_string())
        } else {
            Ok(serde_json::to_string_pretty(&cookies)?)
        }
    }
}

// ── Wait For Tool ────────────────────────────────────────────

struct WaitForTool {
    command_tx: CommandTx,
}

impl WaitForTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for WaitForTool {
    fn name(&self) -> &str {
        "wait_for"
    }
    fn description(&self) -> &str {
        "Check if an element matching a CSS selector exists on the page"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector to wait for" },
            },
            "required": ["selector"],
        })
    }
    fn execute(&self, args: &serde_json::Value) -> anyhow::Result<String> {
        let selector = args
            .get("selector")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'selector' parameter"))?;

        let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
        let code = format!(
            "(function() {{ \
                var el = document.querySelector('{escaped}'); \
                return el ? 'found' : 'not_found'; \
            }})()"
        );

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ExecuteJs {
                code: code.clone(),
                response_tx,
            })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        let result = response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Wait cancelled"))?;

        // wry wraps string results JSON-encoded ("\"found\""); strip it.
        let unquoted = result.trim().trim_matches('"').to_string();
        if unquoted == "found" {
            return Ok(format!("Element '{selector}' is present on the page."));
        }

        // Not there yet: actually wait — re-check every 250ms up to 10s.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let (response_tx, response_rx) = tokio::sync::oneshot::channel();
            if self
                .command_tx
                .send(McpCommand::ExecuteJs {
                    code: code.clone(),
                    response_tx,
                })
                .is_err()
            {
                break;
            }
            let Ok(result) = response_rx.blocking_recv() else {
                break;
            };
            if result.trim().trim_matches('"') == "found" {
                return Ok(format!(
                    "Element '{selector}' appeared after {:.1}s.",
                    deadline.elapsed().as_secs_f32()
                ));
            }
        }

        Ok(format!(
            "Element '{selector}' not found after 10s. Retry after a delay or check the selector."
        ))
    }
}

// ── Create Tab Tool ──────────────────────────────────────────

struct CreateTabTool {
    command_tx: CommandTx,
}

impl CreateTabTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for CreateTabTool {
    fn name(&self) -> &str {
        "create_tab"
    }
    fn description(&self) -> &str {
        "Open a new tab with an optional URL"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "default": "aileron://new", "description": "URL to open in the new tab" }
            },
        })
    }
    fn execute(&self, args: &serde_json::Value) -> anyhow::Result<String> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("aileron://new");

        self.command_tx
            .send(McpCommand::Navigate {
                url: url.to_string(),
                new_tab: true,
            })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        Ok(format!("Opened new tab: {url}"))
    }
}

// ── Close Tab Tool ───────────────────────────────────────────

struct CloseTabTool {
    command_tx: CommandTx,
}

impl CloseTabTool {
    fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for CloseTabTool {
    fn name(&self) -> &str {
        "close_tab"
    }
    fn description(&self) -> &str {
        "Close a tab by index (0-based)"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "index": { "type": "integer", "description": "Tab index to close (0-based)" }
            },
            "required": ["index"],
        })
    }
    fn execute(&self, args: &serde_json::Value) -> anyhow::Result<String> {
        let index = args
            .get("index")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'index' parameter"))?;

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::CloseTab {
                index: index as usize,
                response_tx,
            })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Close tab cancelled"))
    }
}

/// Tool: Scroll the active pane by pixel deltas.
pub struct ScrollTool {
    command_tx: CommandTx,
}

impl ScrollTool {
    pub fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for ScrollTool {
    fn name(&self) -> &str {
        "scroll"
    }
    fn description(&self) -> &str {
        "Scroll the active pane by pixel deltas. Use positive values to scroll down/right."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "delta_x": {
                    "type": "integer",
                    "description": "Horizontal scroll delta in pixels (positive = right)",
                    "default": 0
                },
                "delta_y": {
                    "type": "integer",
                    "description": "Vertical scroll delta in pixels (positive = down)",
                    "default": 100
                }
            },
            "required": []
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let delta_x = args.get("delta_x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let delta_y = args.get("delta_y").and_then(|v| v.as_i64()).unwrap_or(100) as i32;

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::Scroll {
                delta_x,
                delta_y,
                response_tx,
            })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Scroll cancelled"))
    }
}

/// Tool: Navigate back in browser history.
pub struct NavigateBackTool {
    command_tx: CommandTx,
}

impl NavigateBackTool {
    pub fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for NavigateBackTool {
    fn name(&self) -> &str {
        "navigate_back"
    }
    fn description(&self) -> &str {
        "Navigate the active pane back in history"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    fn execute(&self, _args: &Value) -> anyhow::Result<String> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::NavigateBack { response_tx })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Navigate back cancelled"))
    }
}

/// Tool: Navigate forward in browser history.
pub struct NavigateForwardTool {
    command_tx: CommandTx,
}

impl NavigateForwardTool {
    pub fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for NavigateForwardTool {
    fn name(&self) -> &str {
        "navigate_forward"
    }
    fn description(&self) -> &str {
        "Navigate the active pane forward in history"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    fn execute(&self, _args: &Value) -> anyhow::Result<String> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::NavigateForward { response_tx })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Navigate forward cancelled"))
    }
}

/// Tool: Select an option in a `&lt;select&gt;` element.
pub struct SelectOptionTool {
    command_tx: CommandTx,
}

impl SelectOptionTool {
    pub fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for SelectOptionTool {
    fn name(&self) -> &str {
        "select_option"
    }
    fn description(&self) -> &str {
        "Select an option in a `select` dropdown by CSS selector and option value"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "selector": {
                    "type": "string",
                    "description": "CSS selector for the <select> element"
                },
                "value": {
                    "type": "string",
                    "description": "The value attribute of the option to select"
                }
            },
            "required": ["selector", "value"]
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let selector = args
            .get("selector")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'selector' parameter"))?;
        let value = args
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'value' parameter"))?;

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::SelectOption {
                selector: selector.to_string(),
                value: value.to_string(),
                response_tx,
            })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Select option cancelled"))
    }
}

/// Tool: Read the accessibility tree of the active pane.
pub struct ReadPageStructureTool {
    command_tx: CommandTx,
}

impl ReadPageStructureTool {
    pub fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for ReadPageStructureTool {
    fn name(&self) -> &str {
        "read_page_structure"
    }
    fn description(&self) -> &str {
        "Read the accessibility tree of the active page. Returns structured information about headings, landmarks, links, buttons, and form elements."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    fn execute(&self, _args: &Value) -> anyhow::Result<String> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ReadPageStructure { response_tx })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Read page structure cancelled"))
    }
}

/// Tool: Extract structured data (JSON-LD, OpenGraph, tables) from active pane.
pub struct ExtractStructuredDataTool {
    command_tx: CommandTx,
}

impl ExtractStructuredDataTool {
    pub fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for ExtractStructuredDataTool {
    fn name(&self) -> &str {
        "extract_structured_data"
    }
    fn description(&self) -> &str {
        "Extract structured data from the active page: JSON-LD schema, OpenGraph meta tags, and HTML tables as JSON."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    fn execute(&self, _args: &Value) -> anyhow::Result<String> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::ExtractStructuredData { response_tx })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Extract structured data cancelled"))
    }
}

/// Tool: Summarize the active page content using local LLM.
pub struct SummarizePageTool {
    command_tx: CommandTx,
}

impl SummarizePageTool {
    pub fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for SummarizePageTool {
    fn name(&self) -> &str {
        "summarize_page"
    }
    fn description(&self) -> &str {
        "Summarize the active page content using a local LLM (Ollama). Returns a concise summary."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    fn execute(&self, _args: &Value) -> anyhow::Result<String> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::SummarizePage { response_tx })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Summarize page cancelled"))
    }
}

/// Tool: Translate the active page content to a target language.
pub struct TranslatePageTool {
    command_tx: CommandTx,
}

impl TranslatePageTool {
    pub fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for TranslatePageTool {
    fn name(&self) -> &str {
        "translate_page"
    }
    fn description(&self) -> &str {
        "Translate the active page content to a target language using a local LLM (Ollama)."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target_lang": {
                    "type": "string",
                    "description": "Target language code (e.g., 'en', 'zh', 'ja', 'es', 'fr', 'de')"
                }
            },
            "required": ["target_lang"]
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let target_lang = args
            .get("target_lang")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'target_lang' parameter"))?;

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::TranslatePage {
                target_lang: target_lang.to_string(),
                response_tx,
            })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Translate page cancelled"))
    }
}

/// Tool: Analyze the active page content using local LLM.
pub struct AnalyzePageTool {
    command_tx: CommandTx,
}

impl AnalyzePageTool {
    pub fn new(command_tx: CommandTx) -> Self {
        Self { command_tx }
    }
}

impl McpTool for AnalyzePageTool {
    fn name(&self) -> &str {
        "analyze_page"
    }
    fn description(&self) -> &str {
        "Analyze the active page content using a local LLM (Ollama). Returns structured analysis with topic, key points, sentiment, and language."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
    fn execute(&self, _args: &Value) -> anyhow::Result<String> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        self.command_tx
            .send(McpCommand::AnalyzePage { response_tx })
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))?;

        response_rx
            .blocking_recv()
            .map_err(|_| anyhow::anyhow!("Analyze page cancelled"))
    }
}

/// Create all MCP tools wired to the given bridge.
pub fn create_tools(state: McpState, command_tx: CommandTx) -> Vec<Box<dyn McpTool + Send + Sync>> {
    vec![
        Box::new(ReadActivePaneTool::new(state.clone())),
        Box::new(BrowserObserveTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserSnapshotTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserClickTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserHoverTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserFocusTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserTypeTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserPressKeyTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserSelectTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserDragTool::new(state.clone(), command_tx.clone())),
        Box::new(SessionScopeTool::new(state.clone(), command_tx.clone())),
        Box::new(SessionInfoTool::new(state.clone(), command_tx.clone())),
        Box::new(WorkspaceCheckpointTool::new(
            state.clone(),
            command_tx.clone(),
        )),
        Box::new(WorkspaceDiffTool::new(state.clone(), command_tx.clone())),
        Box::new(WorkspaceListTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserBulkActTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserWaitForTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserConsoleTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserNetworkTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserCookiesTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserDownloadsTool::new(command_tx.clone())),
        Box::new(BrowserGetTextTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserFillFormTool::new(state.clone(), command_tx.clone())),
        Box::new(SearchWebTool),
        Box::new(ListTabsTool::new(command_tx.clone())),
        Box::new(BookmarkCrudTool::new(command_tx.clone())),
        Box::new(HistorySearchTool::new(command_tx.clone())),
        Box::new(NavigateTool::new(command_tx.clone())),
        Box::new(ExecuteJsTool::new(command_tx.clone())),
        Box::new(ScreenshotTool::new(command_tx.clone())),
        Box::new(ClickTool::new(command_tx.clone())),
        Box::new(GetCookiesTool::new(command_tx.clone())),
        Box::new(WaitForTool::new(command_tx.clone())),
        Box::new(CreateTabTool::new(command_tx.clone())),
        Box::new(CloseTabTool::new(command_tx.clone())),
        // AI-native browsing tools
        Box::new(ScrollTool::new(command_tx.clone())),
        Box::new(NavigateBackTool::new(command_tx.clone())),
        Box::new(NavigateForwardTool::new(command_tx.clone())),
        Box::new(SelectOptionTool::new(command_tx.clone())),
        Box::new(ReadPageStructureTool::new(command_tx.clone())),
        Box::new(ExtractStructuredDataTool::new(command_tx.clone())),
        // LLM integration tools
        Box::new(SummarizePageTool::new(command_tx.clone())),
        Box::new(TranslatePageTool::new(command_tx.clone())),
        Box::new(AnalyzePageTool::new(command_tx)),
    ]
}

/// Create default stub tools (for testing without a bridge).
pub fn default_tools() -> Vec<Box<dyn McpTool + Send + Sync>> {
    vec![
        Box::new(ReadActivePaneTool::new(McpState::default())),
        Box::new(BrowserGetTextTool::new(
            McpState::default(),
            CommandTx::new(std::sync::mpsc::channel().0, None),
        )),
        Box::new(BrowserFillFormTool::new(
            McpState::default(),
            CommandTx::new(std::sync::mpsc::channel().0, None),
        )),
        Box::new(SearchWebTool),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_active_pane() {
        let state = McpState::default();
        let tool = ReadActivePaneTool::new(state);
        assert_eq!(tool.name(), "read_active_pane");
        let result = tool
            .execute(&Value::Object(serde_json::Map::new()))
            .unwrap();
        assert!(result.contains("No active pane"));
    }

    #[test]
    fn test_read_active_pane_with_state() {
        let state = McpState::default();
        {
            let mut url = state.active_url.write().unwrap();
            *url = "https://example.com".to_string();
        }
        {
            let mut title = state.active_title.write().unwrap();
            *title = "Example".to_string();
        }
        let tool = ReadActivePaneTool::new(state);
        let result = tool
            .execute(&Value::Object(serde_json::Map::new()))
            .unwrap();
        assert!(result.contains("example.com"));
        assert!(result.contains("Example"));
    }

    #[test]
    fn test_search_web() {
        if std::env::var("AILERON_NETWORK_TESTS").is_err() {
            return; // Skip: requires network access to DuckDuckGo API
        }
        let tool = SearchWebTool;
        let args = json!({"query": "rust programming"});
        let result = tool.execute(&args).unwrap();
        assert!(result.contains("rust programming"));
    }

    #[test]
    fn test_create_tools() {
        let state = McpState::default();
        let (tx, _) = std::sync::mpsc::channel();
        let tx = CommandTx::new(tx, None);
        let tools = create_tools(state, tx);
        assert_eq!(tools.len(), 44);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"read_active_pane"));
        assert!(names.contains(&"browser_get_text"));
        assert!(names.contains(&"browser_fill_form"));
        assert!(names.contains(&"search_web"));
        assert!(names.contains(&"list_tabs"));
        assert!(names.contains(&"bookmark_crud"));
        assert!(names.contains(&"history_search"));
        assert!(names.contains(&"navigate"));
        assert!(names.contains(&"execute_js"));
        assert!(names.contains(&"screenshot"));
        assert!(names.contains(&"click"));
        assert!(names.contains(&"get_cookies"));
        assert!(names.contains(&"wait_for"));
        assert!(names.contains(&"create_tab"));
        assert!(names.contains(&"close_tab"));
        // AI-native browsing tools
        assert!(names.contains(&"scroll"));
        assert!(names.contains(&"navigate_back"));
        assert!(names.contains(&"navigate_forward"));
        assert!(names.contains(&"select_option"));
        assert!(names.contains(&"read_page_structure"));
        assert!(names.contains(&"extract_structured_data"));
        // LLM integration tools
        assert!(names.contains(&"summarize_page"));
        assert!(names.contains(&"translate_page"));
        assert!(names.contains(&"analyze_page"));
    }

    #[test]
    fn test_default_tools() {
        let tools = default_tools();
        assert_eq!(tools.len(), 4);
    }

    #[test]
    fn test_browser_get_text_missing_body() {
        let state = McpState::default();
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        // Simulate a page with no body
        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, .. })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                let _ = response_tx.send("String(\"\")".to_string());
            }
        });

        let tool = BrowserGetTextTool::new(state, tx);
        let args = json!({});
        let result = tool.execute(&args).unwrap();
        assert!(result.contains("No visible text content"));
    }

    #[test]
    fn test_browser_get_text_with_max_length() {
        let state = McpState::default();
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, code })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                // Verify max_length is in the JS code
                assert!(code.contains("100"), "JS should contain max_length=100");
                let _ = response_tx.send("String(\"hello world\")".to_string());
            }
        });

        let tool = BrowserGetTextTool::new(state, tx);
        let args = json!({"max_length": 100});
        let result = tool.execute(&args).unwrap();
        assert!(result.contains("hello world"));
    }

    #[test]
    fn test_browser_fill_form_missing_selector() {
        let state = McpState::default();
        let (tx, _) = std::sync::mpsc::channel();
        let tx = CommandTx::new(tx, None);
        let tool = BrowserFillFormTool::new(state, tx);
        let args = json!({"value": "test"});
        let result = tool.execute(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("selector"));
    }

    #[test]
    fn test_browser_fill_form_missing_value() {
        let state = McpState::default();
        let (tx, _) = std::sync::mpsc::channel();
        let tx = CommandTx::new(tx, None);
        let tool = BrowserFillFormTool::new(state, tx);
        let args = json!({"selector": "#email"});
        let result = tool.execute(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("value"));
    }

    #[test]
    fn test_browser_fill_form_success() {
        let state = McpState::default();
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, code })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                // Verify selector and value are in the JS code
                assert!(code.contains("#email"), "JS should contain selector");
                assert!(code.contains("test@example.com"), "JS should contain value");
                assert!(
                    code.contains("submit"),
                    "JS should contain submit logic when submit=true"
                );
                let _ = response_tx.send("String(\"Filled and submitted\")".to_string());
            }
        });

        let tool = BrowserFillFormTool::new(state, tx);
        let args = json!({"selector": "#email", "value": "test@example.com", "submit": true});
        let result = tool.execute(&args).unwrap();
        assert!(result.contains("Filled and submitted"));
    }

    #[test]
    fn test_browser_fill_form_no_submit() {
        let state = McpState::default();
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, code })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                // When submit is false, should NOT contain submit()
                assert!(
                    !code.contains("form.submit()"),
                    "JS should not submit form when submit=false"
                );
                let _ = response_tx.send("String(\"Filled\")".to_string());
            }
        });

        let tool = BrowserFillFormTool::new(state, tx);
        let args = json!({"selector": "#q", "value": "search term"});
        let result = tool.execute(&args).unwrap();
        assert!(result.contains("Filled"));
    }

    #[test]
    fn test_browser_fill_form_element_not_found() {
        let state = McpState::default();
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, .. })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                let _ = response_tx.send("String(\"Error: element not found\")".to_string());
            }
        });

        let tool = BrowserFillFormTool::new(state, tx);
        let args = json!({"selector": "#nonexistent", "value": "test"});
        let result = tool.execute(&args).unwrap();
        assert!(result.contains("element not found"));
    }

    #[test]
    fn test_navigate_tool_active_tab() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);
        let tool = NavigateTool::new(tx);
        let args = json!({"url": "https://example.com"});
        let result = tool.execute(&args).unwrap();
        assert!(result.contains("Navigating to"));
        let cmd = rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .unwrap();
        match cmd {
            (_, McpCommand::Navigate { url, new_tab }) => {
                assert_eq!(url, "https://example.com");
                assert!(!new_tab);
            }
            _ => panic!("Unexpected command: {cmd:?}"),
        }
    }

    #[test]
    fn test_navigate_tool_new_tab() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);
        let tool = NavigateTool::new(tx);
        let args = json!({"url": "https://example.com", "new_tab": true});
        let result = tool.execute(&args).unwrap();
        assert!(result.contains("new tab"));
        let cmd = rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .unwrap();
        match cmd {
            (_, McpCommand::Navigate { url, new_tab }) => {
                assert_eq!(url, "https://example.com");
                assert!(new_tab);
            }
            _ => panic!("Unexpected command: {cmd:?}"),
        }
    }

    #[test]
    fn test_navigate_tool_invalid_url() {
        let (tx, _) = std::sync::mpsc::channel();
        let tx = CommandTx::new(tx, None);
        let tool = NavigateTool::new(tx);
        let args = json!({"url": "not-a-url"});
        let result = tool.execute(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_js_tool() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, .. })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                let _ = response_tx.send("42".to_string());
            }
        });

        let tool = ExecuteJsTool::new(tx);
        let args = json!({"code": "1 + 1"});
        let result = tool.execute(&args).unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn test_execute_js_tool_missing_code() {
        let (tx, _) = std::sync::mpsc::channel();
        let tx = CommandTx::new(tx, None);
        let tool = ExecuteJsTool::new(tx);
        let args = json!({});
        let result = tool.execute(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("code"));
    }

    #[test]
    fn test_screenshot_tool() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::Screenshot { response_tx })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                let _ = response_tx.send("data:image/png;base64,test".to_string());
            }
        });

        let tool = ScreenshotTool::new(tx);
        let result = tool.execute(&json!({})).unwrap();
        assert!(result.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_screenshot_tool_no_pane() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::Screenshot { response_tx })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                let _ = response_tx.send("Error: no active pane".to_string());
            }
        });

        let tool = ScreenshotTool::new(tx);
        let result = tool.execute(&json!({})).unwrap();
        assert!(result.contains("no active pane"));
    }

    #[test]
    fn test_click_tool() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, code })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                assert!(
                    code.contains("querySelector"),
                    "JS should contain querySelector"
                );
                assert!(code.contains(".click()"), "JS should contain click()");
                let _ = response_tx.send("clicked".to_string());
            }
        });

        let tool = ClickTool::new(tx);
        let args = json!({"selector": "#submit-btn"});
        let result = tool.execute(&args).unwrap();
        assert_eq!(result, "clicked");
    }

    #[test]
    fn test_click_tool_missing_selector() {
        let (tx, _) = std::sync::mpsc::channel();
        let tx = CommandTx::new(tx, None);
        let tool = ClickTool::new(tx);
        let args = json!({});
        let result = tool.execute(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("selector"));
    }

    #[test]
    fn test_get_cookies_no_cookies() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, .. })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                let _ = response_tx.send("".to_string());
            }
        });

        let tool = GetCookiesTool::new(tx);
        let result = tool.execute(&json!({})).unwrap();
        assert!(result.contains("No cookies found"));
    }

    #[test]
    fn test_get_cookies_with_cookies() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, .. })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                let _ = response_tx.send("session=abc123; user=john".to_string());
            }
        });

        let tool = GetCookiesTool::new(tx);
        let result = tool.execute(&json!({})).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
        assert_eq!(parsed[0]["name"], "session");
        assert_eq!(parsed[0]["value"], "abc123");
        assert_eq!(parsed[1]["name"], "user");
        assert_eq!(parsed[1]["value"], "john");
    }

    #[test]
    fn test_get_cookies_with_domain_filter() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, .. })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                let _ = response_tx.send("session=abc123; _ga=GA123".to_string());
            }
        });

        let tool = GetCookiesTool::new(tx);
        let result = tool.execute(&json!({"domain": "ga"})).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 1);
        assert_eq!(parsed[0]["name"], "_ga");
    }

    #[test]
    fn test_wait_for_found() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, code })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                assert!(code.contains("#result"), "JS should contain selector");
                let _ = response_tx.send("found".to_string());
            }
        });

        let tool = WaitForTool::new(tx);
        let result = tool.execute(&json!({"selector": "#result"})).unwrap();
        assert!(result.contains("present"));
    }

    #[test]
    fn test_wait_for_not_found() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::ExecuteJs { response_tx, .. })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                let _ = response_tx.send("not_found".to_string());
            }
        });

        let tool = WaitForTool::new(tx);
        let result = tool.execute(&json!({"selector": "#nonexistent"})).unwrap();
        assert!(result.contains("not found"));
    }

    #[test]
    fn test_wait_for_missing_selector() {
        let (tx, _) = std::sync::mpsc::channel();
        let tx = CommandTx::new(tx, None);
        let tool = WaitForTool::new(tx);
        let result = tool.execute(&json!({}));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("selector"));
    }

    #[test]
    fn test_create_tab_default_url() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);
        let tool = CreateTabTool::new(tx);
        let result = tool.execute(&json!({})).unwrap();
        assert!(result.contains("aileron://new"));
        let cmd = rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .unwrap();
        match cmd {
            (_, McpCommand::Navigate { url, new_tab }) => {
                assert_eq!(url, "aileron://new");
                assert!(new_tab);
            }
            _ => panic!("Unexpected command: {cmd:?}"),
        }
    }

    #[test]
    fn test_create_tab_custom_url() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);
        let tool = CreateTabTool::new(tx);
        let result = tool
            .execute(&json!({"url": "https://example.com"}))
            .unwrap();
        assert!(result.contains("example.com"));
        let cmd = rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .unwrap();
        match cmd {
            (_, McpCommand::Navigate { url, new_tab }) => {
                assert_eq!(url, "https://example.com");
                assert!(new_tab);
            }
            _ => panic!("Unexpected command: {cmd:?}"),
        }
    }

    #[test]
    fn test_close_tab() {
        let (tx, rx) = std::sync::mpsc::channel::<(Option<uuid::Uuid>, McpCommand)>();
        let tx = CommandTx::new(tx, None);

        std::thread::spawn(move || {
            if let Ok((_, McpCommand::CloseTab { index, response_tx })) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                assert_eq!(index, 2);
                let _ = response_tx.send("Closed tab 2.".to_string());
            }
        });

        let tool = CloseTabTool::new(tx);
        let result = tool.execute(&json!({"index": 2})).unwrap();
        assert!(result.contains("Closed tab 2"));
    }

    #[test]
    fn test_close_tab_missing_index() {
        let (tx, _) = std::sync::mpsc::channel();
        let tx = CommandTx::new(tx, None);
        let tool = CloseTabTool::new(tx);
        let result = tool.execute(&json!({}));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("index"));
    }
}
