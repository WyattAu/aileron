//! MCP tool implementations for the Aileron Model Context Protocol server.
//!
//! Tools access pane state via McpBridge (shared state + command channel).

use crate::mcp::bridge::{McpCommand, McpState};
use serde_json::{json, Value};

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
                "## Active Pane\n\n**URL:** {}\n**Title:** {}\n",
                url, title
            ))
        }
    }
}

/// Tool: Navigate to a URL in the active pane.
/// Sends a command to the main thread via McpBridge.
pub struct BrowserNavigateTool {
    #[allow(dead_code)]
    state: McpState,
    command_tx: std::sync::mpsc::Sender<McpCommand>,
}

impl BrowserNavigateTool {
    pub fn new(state: McpState, command_tx: std::sync::mpsc::Sender<McpCommand>) -> Self {
        Self { state, command_tx }
    }
}

impl McpTool for BrowserNavigateTool {
    fn name(&self) -> &str {
        "browser_navigate"
    }
    fn description(&self) -> &str {
        "Navigate the active browser pane to a URL"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to navigate to"
                }
            },
            "required": ["url"]
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' parameter"))?;

        // Validate URL
        if url::Url::parse(url).is_err() {
            return Err(anyhow::anyhow!("Invalid URL: {}", url));
        }

        // Send navigate command to main thread
        self.command_tx
            .send(McpCommand::Navigate {
                url: url.to_string(),
            })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;

        Ok(format!("Navigating to: {}", url))
    }
}

/// Tool: Execute JavaScript in the active pane.
/// Sends a command to the main thread via McpBridge.
pub struct RunJsTool {
    #[allow(dead_code)]
    state: McpState,
    command_tx: std::sync::mpsc::Sender<McpCommand>,
}

impl RunJsTool {
    pub fn new(state: McpState, command_tx: std::sync::mpsc::Sender<McpCommand>) -> Self {
        Self { state, command_tx }
    }
}

impl McpTool for RunJsTool {
    fn name(&self) -> &str {
        "run_js"
    }
    fn description(&self) -> &str {
        "Execute JavaScript in the active browser pane and return the result"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "JavaScript code to execute"
                }
            },
            "required": ["code"]
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let code = args
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'code' parameter"))?;

        // Create a response channel
        let (response_tx, response_rx) = std::sync::mpsc::channel();

        // Send JS execution command to main thread
        self.command_tx
            .send(McpCommand::ExecuteJs {
                code: code.to_string(),
                response_tx,
            })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;

        // Wait for the result (with timeout)
        let result = response_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|e| anyhow::anyhow!("JS execution timed out: {}", e))?;

        Ok(result)
    }
}

/// Tool: Search the web (stub — requires actual search engine integration).
pub struct SearchWebTool;

impl McpTool for SearchWebTool {
    fn name(&self) -> &str {
        "search_web"
    }
    fn description(&self) -> &str {
        "Search the web using a search engine (stub — requires integration)"
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

        // Stub: return a DuckDuckGo search URL
        let encoded = urlencoding::encode(query);
        let search_url = format!("https://duckduckgo.com/?q={}", encoded);
        Ok(format!(
            "Search results for '{}': {}\n\nNote: Aileron does not yet have a built-in search API. \
             Opening in the active pane may work.",
            query, search_url
        ))
    }
}

/// Tool: Extract visible text content from the active pane.
/// A convenience wrapper around `run_js` that returns `document.body.innerText`.
pub struct BrowserGetTextTool {
    #[allow(dead_code)]
    state: McpState,
    command_tx: std::sync::mpsc::Sender<McpCommand>,
}

impl BrowserGetTextTool {
    pub fn new(state: McpState, command_tx: std::sync::mpsc::Sender<McpCommand>) -> Self {
        Self { state, command_tx }
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
                if (text.length > {}) {{ text = text.substring(0, {}) + '... [truncated]'; }} \
                return text; \
            }})()",
            max_length, max_length
        );

        let (response_tx, response_rx) = std::sync::mpsc::channel();
        self.command_tx
            .send(McpCommand::ExecuteJs { code, response_tx })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;

        let result = response_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|e| anyhow::anyhow!("Text extraction timed out: {}", e))?;

        if result.is_empty() || result == "String(\"\")" {
            Ok("No visible text content found on the page.".into())
        } else {
            Ok(result)
        }
    }
}

/// Tool: Fill a form field in the active pane via CSS selector.
/// Sets the value, dispatches input/change events so frameworks (React, Vue)
/// pick up the change, and optionally submits the form.
pub struct BrowserFillFormTool {
    #[allow(dead_code)]
    state: McpState,
    command_tx: std::sync::mpsc::Sender<McpCommand>,
}

impl BrowserFillFormTool {
    pub fn new(state: McpState, command_tx: std::sync::mpsc::Sender<McpCommand>) -> Self {
        Self { state, command_tx }
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
                    var el = document.querySelector('{}'); \
                    if (!el) return 'Error: element not found'; \
                    el.value = '{}'; \
                    el.dispatchEvent(new Event('input', {{ bubbles: true }})); \
                    el.dispatchEvent(new Event('change', {{ bubbles: true }})); \
                    var form = el.closest('form'); \
                    if (form) {{ form.submit(); return 'Filled and submitted'; }} \
                    return 'Filled (no form to submit)'; \
                }})()",
                escaped_selector, escaped_value
            )
        } else {
            format!(
                "(function() {{ \
                    var el = document.querySelector('{}'); \
                    if (!el) return 'Error: element not found'; \
                    el.value = '{}'; \
                    el.dispatchEvent(new Event('input', {{ bubbles: true }})); \
                    el.dispatchEvent(new Event('change', {{ bubbles: true }})); \
                    return 'Filled'; \
                }})()",
                escaped_selector, escaped_value
            )
        };

        let (response_tx, response_rx) = std::sync::mpsc::channel();
        self.command_tx
            .send(McpCommand::ExecuteJs { code, response_tx })
            .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))?;

        let result = response_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|e| anyhow::anyhow!("Form fill timed out: {}", e))?;

        Ok(result)
    }
}

/// Create all MCP tools wired to the given bridge.
pub fn create_tools(
    state: McpState,
    command_tx: std::sync::mpsc::Sender<McpCommand>,
) -> Vec<Box<dyn McpTool + Send + Sync>> {
    vec![
        Box::new(ReadActivePaneTool::new(state.clone())),
        Box::new(BrowserNavigateTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserGetTextTool::new(state.clone(), command_tx.clone())),
        Box::new(BrowserFillFormTool::new(state.clone(), command_tx.clone())),
        Box::new(RunJsTool::new(state, command_tx)),
        Box::new(SearchWebTool),
    ]
}

/// Create default stub tools (for testing without a bridge).
pub fn default_tools() -> Vec<Box<dyn McpTool + Send + Sync>> {
    vec![
        Box::new(ReadActivePaneTool::new(McpState::default())),
        Box::new(BrowserNavigateTool::new(
            McpState::default(),
            std::sync::mpsc::channel().0,
        )),
        Box::new(BrowserGetTextTool::new(
            McpState::default(),
            std::sync::mpsc::channel().0,
        )),
        Box::new(BrowserFillFormTool::new(
            McpState::default(),
            std::sync::mpsc::channel().0,
        )),
        Box::new(RunJsTool::new(
            McpState::default(),
            std::sync::mpsc::channel().0,
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
    fn test_browser_navigate() {
        let state = McpState::default();
        let (tx, rx) = std::sync::mpsc::channel();
        let tool = BrowserNavigateTool::new(state, tx);
        let args = json!({"url": "https://example.com"});
        let result = tool.execute(&args).unwrap();
        assert!(result.contains("example.com"));
        // Command should be in the channel
        let cmd = rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .unwrap();
        match cmd {
            McpCommand::Navigate { url } => assert_eq!(url, "https://example.com"),
            _ => panic!("Unexpected command: {:?}", cmd),
        }
    }

    #[test]
    fn test_browser_navigate_invalid_url() {
        let state = McpState::default();
        let (tx, _) = std::sync::mpsc::channel();
        let tool = BrowserNavigateTool::new(state, tx);
        let args = json!({"url": "not-a-url"});
        let result = tool.execute(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_search_web() {
        let tool = SearchWebTool;
        let args = json!({"query": "rust programming"});
        let result = tool.execute(&args).unwrap();
        assert!(result.contains("rust programming"));
    }

    #[test]
    fn test_run_js() {
        let state = McpState::default();
        let (tx, rx) = std::sync::mpsc::channel();

        // Spawn a thread to respond to the JS execution command
        std::thread::spawn(move || {
            if let Ok(McpCommand::ExecuteJs { response_tx, .. }) =
                rx.recv_timeout(std::time::Duration::from_secs(5))
            {
                let _ = response_tx.send("Executed JS: undefined".to_string());
            }
        });

        let tool = RunJsTool::new(state, tx);
        let args = json!({"code": "console.log('hello')"});
        let result = tool.execute(&args).unwrap();
        assert!(result.contains("Executed JS"));
    }

    #[test]
    fn test_create_tools() {
        let state = McpState::default();
        let (tx, _) = std::sync::mpsc::channel();
        let tools = create_tools(state, tx);
        assert_eq!(tools.len(), 6);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"read_active_pane"));
        assert!(names.contains(&"browser_navigate"));
        assert!(names.contains(&"browser_get_text"));
        assert!(names.contains(&"browser_fill_form"));
        assert!(names.contains(&"run_js"));
        assert!(names.contains(&"search_web"));
    }

    #[test]
    fn test_default_tools() {
        let tools = default_tools();
        assert_eq!(tools.len(), 6);
    }

    #[test]
    fn test_browser_get_text_missing_body() {
        let state = McpState::default();
        let (tx, rx) = std::sync::mpsc::channel();

        // Simulate a page with no body
        std::thread::spawn(move || {
            if let Ok(McpCommand::ExecuteJs { response_tx, .. }) =
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
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Ok(McpCommand::ExecuteJs { response_tx, code }) =
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
        let tool = BrowserFillFormTool::new(state, tx);
        let args = json!({"selector": "#email"});
        let result = tool.execute(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("value"));
    }

    #[test]
    fn test_browser_fill_form_success() {
        let state = McpState::default();
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Ok(McpCommand::ExecuteJs { response_tx, code }) =
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
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Ok(McpCommand::ExecuteJs { response_tx, code }) =
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
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Ok(McpCommand::ExecuteJs { response_tx, .. }) =
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
}
