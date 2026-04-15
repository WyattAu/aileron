use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn};

use super::tools::McpTool;

/// JSON-RPC request as defined by the MCP protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(-32601, format!("Method not found: {}", method))
    }

    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self::new(-32602, msg)
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self::new(-32603, msg)
    }
}

/// MCP protocol message types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
pub enum McpMessage {
    /// Initialize the MCP connection.
    #[serde(rename = "initialize")]
    Initialize { id: Option<Value>, params: Value },
    /// List available tools.
    #[serde(rename = "tools/list")]
    ToolsList { id: Option<Value> },
    /// Call a specific tool.
    #[serde(rename = "tools/call")]
    ToolsCall { id: Option<Value>, params: Value },
}

/// The MCP server handling JSON-RPC requests.
pub struct McpServer {
    /// Registered tools.
    tools: HashMap<String, Box<dyn McpTool + Send + Sync>>,
    /// Server metadata.
    server_info: ServerInfo,
}

/// Server metadata sent during initialization.
#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub protocol_version: String,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            server_info: ServerInfo {
                name: "aileron".to_string(),
                version: "0.1.0".to_string(),
                protocol_version: "2024-11-05".to_string(),
            },
        }
    }

    /// Register a tool.
    pub fn register_tool(&mut self, tool: Box<dyn McpTool + Send + Sync>) {
        let name = tool.name().to_string();
        info!(target: "mcp", "Registered tool: {}", name);
        self.tools.insert(name, tool);
    }

    /// Process an incoming JSON-RPC request and produce a response.
    pub fn handle_request(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request),
            "tools/list" => self.handle_tools_list(request),
            "tools/call" => self.handle_tools_call(request),
            "notifications/initialized" => {
                // Notification — no response needed, but we return one for simplicity
                JsonRpcResponse::ok(request.id.clone(), Value::Object(serde_json::Map::new()))
            }
            _ => JsonRpcResponse::error(
                request.id.clone(),
                JsonRpcError::method_not_found(&request.method),
            ),
        }
    }

    fn handle_initialize(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let result = serde_json::json!({
            "protocolVersion": self.server_info.protocol_version,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": self.server_info.name,
                "version": self.server_info.version
            }
        });
        JsonRpcResponse::ok(request.id.clone(), result)
    }

    fn handle_tools_list(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let tools: Vec<Value> = self
            .tools
            .values()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name(),
                    "description": tool.description(),
                    "inputSchema": tool.input_schema(),
                })
            })
            .collect();

        let result = serde_json::json!({ "tools": tools });
        JsonRpcResponse::ok(request.id.clone(), result)
    }

    fn handle_tools_call(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let params = match &request.params {
            Some(p) => p,
            None => {
                return JsonRpcResponse::error(
                    request.id.clone(),
                    JsonRpcError::invalid_params("Missing params"),
                );
            }
        };

        let tool_name = params.get("name").and_then(|v| v.as_str());
        let tool_name = match tool_name {
            Some(n) => n,
            None => {
                return JsonRpcResponse::error(
                    request.id.clone(),
                    JsonRpcError::invalid_params("Missing tool name"),
                );
            }
        };

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));

        let tool = match self.tools.get(tool_name) {
            Some(t) => t,
            None => {
                return JsonRpcResponse::error(
                    request.id.clone(),
                    JsonRpcError::invalid_params(format!("Unknown tool: {}", tool_name)),
                );
            }
        };

        match tool.execute(&arguments) {
            Ok(result) => {
                let response = serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": result
                    }]
                });
                JsonRpcResponse::ok(request.id.clone(), response)
            }
            Err(e) => {
                warn!(target: "mcp", "Tool '{}' failed: {}", tool_name, e);
                JsonRpcResponse::error(
                    request.id.clone(),
                    JsonRpcError::internal_error(format!("Tool execution failed: {}", e)),
                )
            }
        }
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonRpcResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyTool;

    impl McpTool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "A dummy tool for testing"
        }
        fn input_schema(&self) -> Value {
            serde_json::json!({"type": "object"})
        }
        fn execute(&self, _args: &Value) -> anyhow::Result<String> {
            Ok("dummy result".into())
        }
    }

    #[test]
    fn test_initialize() {
        let server = McpServer::new();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(1.into())),
            method: "initialize".into(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"}
            })),
        };
        let response = server.handle_request(&request);
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_tools_list_empty() {
        let server = McpServer::new();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(2.into())),
            method: "tools/list".into(),
            params: None,
        };
        let response = server.handle_request(&request);
        let result = response.result.unwrap();
        assert_eq!(result["tools"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_tools_list_with_tool() {
        let mut server = McpServer::new();
        server.register_tool(Box::new(DummyTool));
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(3.into())),
            method: "tools/list".into(),
            params: None,
        };
        let response = server.handle_request(&request);
        let result = response.result.unwrap();
        assert_eq!(result["tools"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_tools_call() {
        let mut server = McpServer::new();
        server.register_tool(Box::new(DummyTool));
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(4.into())),
            method: "tools/call".into(),
            params: Some(serde_json::json!({"name": "dummy", "arguments": {}})),
        };
        let response = server.handle_request(&request);
        assert!(response.error.is_none());
        let content = &response.result.unwrap()["content"][0];
        assert_eq!(content["text"], "dummy result");
    }

    #[test]
    fn test_tools_call_unknown() {
        let server = McpServer::new();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(5.into())),
            method: "tools/call".into(),
            params: Some(serde_json::json!({"name": "nonexistent", "arguments": {}})),
        };
        let response = server.handle_request(&request);
        assert!(response.error.is_some());
    }

    #[test]
    fn test_method_not_found() {
        let server = McpServer::new();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::Number(6.into())),
            method: "nonexistent/method".into(),
            params: None,
        };
        let response = server.handle_request(&request);
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32601);
    }
}
