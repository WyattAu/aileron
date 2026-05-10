#![cfg(feature = "mcp")]

use aileron::mcp::{McpServer, McpTool, McpTransport, server::JsonRpcResponse};
use serde_json::{Value, json};

struct AddTool;

impl McpTool for AddTool {
    fn name(&self) -> &str {
        "add"
    }
    fn description(&self) -> &str {
        "Add two numbers"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "a": { "type": "integer" },
                "b": { "type": "integer" }
            },
            "required": ["a", "b"]
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let a = args["a"].as_i64().unwrap_or(0);
        let b = args["b"].as_i64().unwrap_or(0);
        Ok((a + b).to_string())
    }
}

struct GreetTool;

impl McpTool for GreetTool {
    fn name(&self) -> &str {
        "greet"
    }
    fn description(&self) -> &str {
        "Greet a person by name"
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        })
    }
    fn execute(&self, args: &Value) -> anyhow::Result<String> {
        let name = args["name"].as_str().unwrap_or("stranger");
        Ok(format!("Hello, {name}!"))
    }
}

fn server_with_tools() -> McpTransport {
    let mut server = McpServer::new();
    server.register_tool(Box::new(AddTool));
    server.register_tool(Box::new(GreetTool));
    McpTransport::new(server)
}

fn parse_response(raw: &str) -> JsonRpcResponse {
    serde_json::from_str(raw).expect("response should be valid JSON-RPC")
}

#[test]
fn test_initialize_e2e() {
    let transport = server_with_tools();
    let raw = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test-client","version":"0.1"}}}"#;
    let resp_raw = transport
        .handle_message(raw)
        .expect("should return a response");
    let resp = parse_response(&resp_raw);

    assert_eq!(resp.jsonrpc, "2.0");
    assert_eq!(resp.id, Some(json!(1)));
    assert!(
        resp.error.is_none(),
        "initialize should not return an error"
    );
    assert!(resp.result.is_some());

    let result = resp.result.unwrap();
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert_eq!(result["serverInfo"]["name"], "aileron");
    assert_eq!(result["serverInfo"]["version"], "0.1.0");
    assert!(result["capabilities"]["tools"].is_object());
}

#[test]
fn test_tools_list_returns_registered_tools() {
    let transport = server_with_tools();
    let raw = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let resp_raw = transport
        .handle_message(raw)
        .expect("should return a response");
    let resp = parse_response(&resp_raw);

    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    let tools = result["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);

    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"add"));
    assert!(names.contains(&"greet"));

    let add_tool = tools.iter().find(|t| t["name"] == "add").unwrap();
    assert_eq!(add_tool["description"], "Add two numbers");
    assert_eq!(add_tool["inputSchema"]["type"], "object");
    assert!(
        add_tool["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("a"))
    );
    assert!(
        add_tool["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("b"))
    );
}

#[test]
fn test_tools_call_valid_tool_executes_and_returns_result() {
    let transport = server_with_tools();
    let raw = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add","arguments":{"a":7,"b":5}}}"#;
    let resp_raw = transport
        .handle_message(raw)
        .expect("should return a response");
    let resp = parse_response(&resp_raw);

    assert!(resp.error.is_none());
    let content = &resp.result.unwrap()["content"][0];
    assert_eq!(content["type"], "text");
    assert_eq!(content["text"], "12");
}

#[test]
fn test_tools_call_greet_tool() {
    let transport = server_with_tools();
    let raw = r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"greet","arguments":{"name":"World"}}}"#;
    let resp_raw = transport
        .handle_message(raw)
        .expect("should return a response");
    let resp = parse_response(&resp_raw);

    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    let text = result["content"][0]["text"].as_str().unwrap();
    assert_eq!(text, "Hello, World!");
}

#[test]
fn test_tools_call_unknown_tool_returns_error() {
    let transport = server_with_tools();
    let raw = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nonexistent_tool","arguments":{}}}"#;
    let resp_raw = transport
        .handle_message(raw)
        .expect("should return a response");
    let resp = parse_response(&resp_raw);

    assert!(resp.result.is_none());
    let err = resp.error.expect("should return an error for unknown tool");
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("Unknown tool"));
    assert!(err.message.contains("nonexistent_tool"));
}

#[test]
fn test_tools_call_missing_tool_name_returns_error() {
    let transport = server_with_tools();
    let raw = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"arguments":{}}}"#;
    let resp_raw = transport
        .handle_message(raw)
        .expect("should return a response");
    let resp = parse_response(&resp_raw);

    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, -32602);
}

#[test]
fn test_tools_call_missing_params_returns_error() {
    let transport = server_with_tools();
    let raw = r#"{"jsonrpc":"2.0","id":6,"method":"tools/call"}"#;
    let resp_raw = transport
        .handle_message(raw)
        .expect("should return a response");
    let resp = parse_response(&resp_raw);

    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, -32602);
}

#[test]
fn test_malformed_json_returns_parse_error() {
    let transport = server_with_tools();
    let resp_raw = transport
        .handle_message("this is not json")
        .expect("should return a response");
    let resp = parse_response(&resp_raw);

    assert!(resp.result.is_none());
    let err = resp
        .error
        .expect("malformed JSON should return parse error");
    assert_eq!(err.code, -32700);
    assert!(err.message.contains("Parse error"));
}

#[test]
fn test_malformed_json_truncated_object() {
    let transport = server_with_tools();
    let resp_raw = transport
        .handle_message(r#"{"jsonrpc":"2.0","id":7,"method":"#)
        .expect("should return a response");
    let resp = parse_response(&resp_raw);

    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, -32700);
}

#[test]
fn test_empty_input_returns_none() {
    let transport = server_with_tools();
    assert!(transport.handle_message("").is_none());
    assert!(transport.handle_message("   ").is_none());
}

#[test]
fn test_unknown_method_returns_method_not_found() {
    let transport = server_with_tools();
    let raw = r#"{"jsonrpc":"2.0","id":8,"method":"foo/bar","params":{}}"#;
    let resp_raw = transport
        .handle_message(raw)
        .expect("should return a response");
    let resp = parse_response(&resp_raw);

    assert!(resp.result.is_none());
    let err = resp.error.expect("unknown method should return error");
    assert_eq!(err.code, -32601);
    assert!(err.message.contains("foo/bar"));
}

#[test]
fn test_sequential_requests_maintain_state() {
    let transport = server_with_tools();

    let init = transport.handle_message(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"seq-test","version":"1.0"}}}"#
    ).expect("init should respond");
    let init_resp = parse_response(&init);
    assert!(init_resp.result.is_some());

    let list = transport
        .handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
        .expect("tools/list should respond");
    let list_resp = parse_response(&list);
    assert_eq!(
        list_resp.result.unwrap()["tools"].as_array().unwrap().len(),
        2
    );

    let call1 = transport.handle_message(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add","arguments":{"a":10,"b":20}}}"#
    ).expect("first tools/call should respond");
    assert_eq!(
        parse_response(&call1).result.unwrap()["content"][0]["text"],
        "30"
    );

    let call2 = transport.handle_message(
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"greet","arguments":{"name":"Alice"}}}"#
    ).expect("second tools/call should respond");
    assert_eq!(
        parse_response(&call2).result.unwrap()["content"][0]["text"],
        "Hello, Alice!"
    );

    let call3 = transport.handle_message(
        r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"add","arguments":{"a":100,"b":200}}}"#
    ).expect("third tools/call should respond");
    assert_eq!(
        parse_response(&call3).result.unwrap()["content"][0]["text"],
        "300"
    );

    let list2 = transport
        .handle_message(r#"{"jsonrpc":"2.0","id":6,"method":"tools/list"}"#)
        .expect("second tools/list should respond");
    let list2_resp = parse_response(&list2);
    assert_eq!(
        list2_resp.result.unwrap()["tools"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn test_response_ids_match_request_ids() {
    let transport = server_with_tools();

    let cases: Vec<(&str, Value)> = vec![
        (
            r#"{"jsonrpc":"2.0","id":42,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
            json!(42),
        ),
        (
            r#"{"jsonrpc":"2.0","id":"req-abc","method":"tools/list"}"#,
            json!("req-abc"),
        ),
    ];

    for (raw, expected_id) in cases {
        let resp_raw = transport.handle_message(raw).expect("should respond");
        let resp = parse_response(&resp_raw);
        assert_eq!(
            resp.id,
            Some(expected_id),
            "response id should match request id for input: {raw}"
        );
    }

    let null_resp = transport
        .handle_message(r#"{"jsonrpc":"2.0","id":null,"method":"tools/list"}"#)
        .unwrap();
    let null_resp_str: Value = serde_json::from_str(&null_resp).unwrap();
    assert_eq!(
        null_resp_str["id"],
        Value::Null,
        "null id should round-trip through raw JSON"
    );
}

#[test]
fn test_notification_initialized_returns_empty_result() {
    let transport = server_with_tools();
    let raw = r#"{"jsonrpc":"2.0","id":9,"method":"notifications/initialized"}"#;
    let resp_raw = transport
        .handle_message(raw)
        .expect("should return a response");
    let resp = parse_response(&resp_raw);

    assert!(resp.error.is_none());
    assert_eq!(resp.result.unwrap(), json!({}));
}
