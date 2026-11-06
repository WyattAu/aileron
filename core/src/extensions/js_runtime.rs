//! Background script JavaScript runtime using QuickJS.
//!
//! Each extension with a `background` field in its manifest gets an isolated
//! QuickJS context. The runtime exposes a `chrome` global shim that bridges
//! API calls back into the Rust `AileronExtensionApi` implementation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rquickjs::context::Context;
use rquickjs::{CatchResultExt, Function, Runtime as QjsRuntime};

use crate::extensions::impls::AileronExtensionApi;
use crate::extensions::message_bus::MessageBus;
use crate::extensions::types::{ExtensionId, RuntimeMessage};

type PortMessageCallback = Box<dyn Fn(String) + Send + Sync>;
type PortDisconnectCallback = Box<dyn Fn() + Send + Sync>;

/// Tracks active JS port connections with their callback handlers.
pub(crate) struct PortManager {
    ports: Mutex<HashMap<u64, PortEntry>>,
    next_id: Mutex<u64>,
}

struct PortEntry {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    message_callbacks: Vec<PortMessageCallback>,
    #[allow(dead_code)]
    disconnect_callbacks: Vec<PortDisconnectCallback>,
}

impl PortManager {
    fn new() -> Self {
        Self {
            ports: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }

    fn add_port(
        &self,
        name: String,
        message_callbacks: Vec<PortMessageCallback>,
        disconnect_callbacks: Vec<PortDisconnectCallback>,
    ) -> u64 {
        let id = {
            let mut next = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
            let id = *next;
            *next += 1;
            id
        };
        self.ports.lock().unwrap_or_else(|e| e.into_inner()).insert(
            id,
            PortEntry {
                name,
                message_callbacks,
                disconnect_callbacks,
            },
        );
        id
    }

    fn remove_port(&self, port_id: u64) -> Option<PortEntry> {
        self.ports
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&port_id)
    }
}

/// Handle to an initialised background-script QuickJS context.
pub struct JsRuntime {
    #[allow(dead_code)]
    runtime: QjsRuntime,
    ctx: Context,
    /// Queue of messages received from the MessageBus since last drain.
    pending_messages: Arc<Mutex<Vec<String>>>,
    /// Tracks active port connections.
    port_manager: Arc<PortManager>,
}

impl std::fmt::Debug for JsRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsRuntime").finish_non_exhaustive()
    }
}

impl JsRuntime {
    /// Create a new JS runtime, inject the `chrome` API shim, and evaluate
    /// the extension's background script source.
    pub fn new(api: &AileronExtensionApi, source: &str) -> Result<Self, String> {
        let runtime = QjsRuntime::new().map_err(|e| format!("QuickJS init: {e}"))?;
        let ctx = Context::full(&runtime).map_err(|e| format!("QuickJS context: {e}"))?;

        inject_chrome_shim(&ctx, api.extension_id())?;

        let filename = api
            .background_script()
            .map(|bs| bs.filename.as_str())
            .unwrap_or("background.js");

        ctx.with(|cx| {
            cx.eval::<(), _>(source)
                .catch(&cx)
                .map_err(|e| format!("{filename}: {e}"))
        })?;

        let port_manager = Arc::new(PortManager::new());

        Ok(Self {
            runtime,
            ctx,
            pending_messages: Arc::new(Mutex::new(Vec::new())),
            port_manager,
        })
    }

    /// Fire a lifecycle event by calling a global JS function.
    pub fn fire_event(&self, event_name: &str, json_arg: &str) {
        self.ctx.with(|cx| {
            let fn_name = format!("__aileron_fire_{event_name}");
            if let Ok(func) = cx.globals().get::<_, Function>(&fn_name) {
                let _ = func.call::<_, ()>((json_arg,));
            }
        });
    }

    /// Fire the onInstalled event by iterating registered listeners.
    pub fn fire_on_installed(&self, reason: &str, id: &ExtensionId) {
        self.ctx.with(|cx| {
            let js = format!(
                "if (typeof __aileron_listeners !== 'undefined') {{ \
                 __aileron_listeners.onInstalled.forEach(function(fn) {{ \
                 fn({{reason: '{reason}', id: '{id}'}}); \
                 }}); \
                 }}"
            );
            let _ = cx.eval::<(), _>(js.as_str());
        });
    }

    /// Fire the onStartup event by iterating registered listeners.
    pub fn fire_on_startup(&self) {
        self.ctx.with(|cx| {
            let js = "if (typeof __aileron_listeners !== 'undefined') { \
                 __aileron_listeners.onStartup.forEach(function(fn) { fn(); }); \
                 }";
            let _ = cx.eval::<(), _>(js);
        });
    }

    /// Connect the JS runtime's message listeners to the shared MessageBus.
    /// Messages are queued and delivered via `drain_pending_messages()`.
    pub fn connect_message_bus(&self, ext_id: ExtensionId, bus: Arc<MessageBus>) {
        let pending = self.pending_messages.clone();
        bus.register_handler(
            ext_id,
            Box::new(move |msg: RuntimeMessage| {
                let json_str = serde_json::to_string(&msg).unwrap_or_default();
                pending
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(json_str);
                None
            }),
        );
    }

    /// Drain pending messages from the MessageBus and deliver them to
    /// registered `chrome.runtime.onMessage` listeners.
    pub fn drain_pending_messages(&self) {
        let messages: Vec<String> = {
            let mut guard = self
                .pending_messages
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            guard.drain(..).collect()
        };

        if messages.is_empty() {
            return;
        }

        self.ctx.with(|cx| {
            for json_str in messages {
                let escaped = json_str.replace('\\', "\\\\").replace('\'', "\\'");
                let js = format!(
                    "if (typeof __aileron_listeners !== 'undefined' && \
                     __aileron_listeners.onMessage) {{ \
                     __aileron_listeners.onMessage.forEach(function(fn) {{ \
                     fn(JSON.parse('{escaped}'), {{}}, {{}}); \
                     }}); \
                     }}"
                );
                let _ = cx.eval::<(), _>(js.as_str());
            }
        });
    }

    /// Create a new port in the JS context and register native callbacks.
    /// Returns the port ID for use with `fire_incoming_port_message` and
    /// `fire_port_disconnect`.
    pub fn create_port_in_js_context(&self, port_name: &str, ext_id: &str) -> u64 {
        let ext_id_escaped = ext_id.replace('\'', "\\'");
        let name_escaped = port_name.replace('\'', "\\'");

        // JS: register message/disconnect callbacks and invoke onConnect listeners.
        // The callbacks are stored in __aileron_port_callbacks[portId] so Rust
        // can later invoke them via fire_incoming_port_message / fire_port_disconnect.
        let js_create = format!(
            "(function() {{ \
                var portId = __aileron_next_port_id++; \
                var port = {{ \
                    name: '{name_escaped}', \
                    sender: {{ id: '{ext_id_escaped}' }}, \
                    postMessage: function(data) {{ \
                        __aileron_native_connect_postMessage(portId, JSON.stringify(data)); \
                    }}, \
                    disconnect: function() {{ \
                        __aileron_native_connect_disconnect(portId); \
                    }}, \
                    onMessage: {{ addListener: function(fn) {{ \
                        __aileron_port_callbacks[portId].message.push(fn); \
                    }} }}, \
                    onDisconnect: {{ addListener: function(fn) {{ \
                        __aileron_port_callbacks[portId].disconnect.push(fn); \
                    }} }} \
                }}; \
                __aileron_port_callbacks[portId] = {{ \
                    message: [], \
                    disconnect: [], \
                    name: '{name_escaped}', \
                    sender: {{ id: '{ext_id_escaped}' }} \
                }}; \
                if (typeof __aileron_listeners !== 'undefined' && \
                    __aileron_listeners.onConnect) {{ \
                    __aileron_listeners.onConnect.forEach(function(fn) {{ fn(port); }}); \
                }} \
                return portId; \
            }})()"
        );

        let port_id: u64 = self.ctx.with(|cx| {
            cx.eval::<u64, _>(js_create.as_bytes())
                .catch(&cx)
                .map_err(|e| format!("create_port_in_js_context: {e}"))
                .unwrap_or(0)
        });

        // Register the port with the Rust PortManager so we can fire callbacks.
        let pm = self.port_manager.clone();
        pm.add_port(port_name.to_string(), vec![], vec![]);

        port_id
    }

    /// Deliver an incoming message to a port's JS callbacks.
    pub fn fire_incoming_port_message(&self, port_id: u64, json_data: &str) {
        let escaped = json_data.replace('\\', "\\\\").replace('\'', "\\'");
        let js = format!(
            "(function() {{ \
                var cb = __aileron_port_callbacks[{port_id}]; \
                if (cb && cb.message) {{ \
                    cb.message.forEach(function(fn) {{ \
                        fn(JSON.parse('{escaped}')); \
                    }}); \
                }} \
            }})()"
        );
        self.ctx.with(|cx| {
            let _ = cx.eval::<(), _>(js.as_bytes());
        });
    }

    /// Fire the disconnect handlers for a port and clean up.
    pub fn fire_port_disconnect(&self, port_id: u64) {
        let js = format!(
            "(function() {{ \
                var cb = __aileron_port_callbacks[{port_id}]; \
                if (cb && cb.disconnect) {{ \
                    cb.disconnect.forEach(function(fn) {{ fn(); }}); \
                }} \
                delete __aileron_port_callbacks[{port_id}]; \
            }})()"
        );
        self.ctx.with(|cx| {
            let _ = cx.eval::<(), _>(js.as_bytes());
        });

        self.port_manager.remove_port(port_id);
    }

    /// Get a reference to the port manager (test-only).
    #[cfg(test)]
    pub(crate) fn port_manager(&self) -> &Arc<PortManager> {
        &self.port_manager
    }
}

/// Inject a minimal `chrome` global object with API stubs.
fn inject_chrome_shim(ctx: &Context, _ext_id: &ExtensionId) -> Result<(), String> {
    ctx.with(|cx| {
        let shim = r#"
            var chrome = chrome || {};
            chrome.runtime = chrome.runtime || {};
            var __aileron_listeners = {
                onInstalled: [],
                onStartup: [],
                onMessage: [],
                onConnect: [],
            };
            var __aileron_port_callbacks = {};
            var __aileron_next_port_id = 1;
            chrome.runtime.onInstalled = {
                addListener: function(fn) {
                    __aileron_listeners.onInstalled.push(fn);
                }
            };
            chrome.runtime.onStartup = {
                addListener: function(fn) {
                    __aileron_listeners.onStartup.push(fn);
                }
            };
            chrome.runtime.onMessage = {
                addListener: function(fn) {
                    __aileron_listeners.onMessage.push(fn);
                }
            };
            chrome.runtime.onConnect = {
                addListener: function(fn) {
                    __aileron_listeners.onConnect.push(fn);
                }
            };
            chrome.runtime.connect = function(connectInfo) {
                var info = connectInfo || {};
                var portId = __aileron_native_connect_create(info.name || '');
                var name = info.name || '';
                var port = {
                    name: name,
                    sender: { id: '' },
                    postMessage: function(data) {
                        __aileron_native_connect_postMessage(portId, JSON.stringify(data));
                    },
                    disconnect: function() {
                        __aileron_native_connect_disconnect(portId);
                    },
                    onMessage: { addListener: function(fn) {
                        __aileron_port_callbacks[portId].message.push(fn);
                    } },
                    onDisconnect: { addListener: function(fn) {
                        __aileron_port_callbacks[portId].disconnect.push(fn);
                    } }
                };
                __aileron_port_callbacks[portId] = {
                    message: [],
                    disconnect: [],
                    name: name,
                    sender: { id: '' }
                };
                // Fire onConnect listeners so background scripts can receive the port.
                if (__aileron_listeners && __aileron_listeners.onConnect) {
                    __aileron_listeners.onConnect.forEach(function(fn) { fn(port); });
                }
                return port;
            };
            chrome.runtime.sendMessage = function(msg, cb) {
                if (typeof __aileron_native_sendMessage === 'function') {
                    __aileron_native_sendMessage(JSON.stringify(msg));
                }
                if (cb) cb({});
            };
            chrome.storage = chrome.storage || {};
            chrome.storage.local = {
                get: function(keys, cb) {
                    if (typeof __aileron_native_storage_get === 'function') {
                        var result = JSON.parse(__aileron_native_storage_get(JSON.stringify(keys)));
                        if (cb) cb(result);
                    } else if (cb) {
                        cb({});
                    }
                },
                set: function(items, cb) {
                    if (typeof __aileron_native_storage_set === 'function') {
                        __aileron_native_storage_set(JSON.stringify(items));
                    }
                    if (cb) cb();
                },
                remove: function(keys, cb) {
                    if (typeof __aileron_native_storage_remove === 'function') {
                        __aileron_native_storage_remove(JSON.stringify(keys));
                    }
                    if (cb) cb();
                },
            };
            chrome.alarms = chrome.alarms || {};
            chrome.alarms.create = function(name, info) {
                if (typeof __aileron_native_alarm_create === 'function') {
                    __aileron_native_alarm_create(name || '', JSON.stringify(info || {}));
                }
            };
            chrome.alarms.onAlarm = {
                addListener: function(fn) {
                    __aileron_listeners.__alarm = __aileron_listeners.__alarm || [];
                    __aileron_listeners.__alarm.push(fn);
                }
            };
            chrome.tabs = chrome.tabs || {};
            chrome.tabs.query = function(queryInfo, cb) {
                if (typeof __aileron_native_tabs_query === 'function') {
                    var result = JSON.parse(__aileron_native_tabs_query(JSON.stringify(queryInfo || {})));
                    if (cb) cb(result);
                } else if (cb) {
                    cb([]);
                }
            };
            chrome.tabs.create = function(createProperties, cb) {
                if (typeof __aileron_native_tabs_create === 'function') {
                    var result = JSON.parse(__aileron_native_tabs_create(JSON.stringify(createProperties || {})));
                    if (cb) cb(result);
                } else if (cb) {
                    cb({});
                }
            };
            function __aileron_native_connect_create(name) {
                return 0;
            }
            function __aileron_native_connect_postMessage(portId, data) {
            }
            function __aileron_native_connect_disconnect(portId) {
            }
        "#;
        cx.eval::<(), _>(shim)
            .catch(&cx)
            .map_err(|e| format!("Chrome shim injection: {e}"))?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::manifest::ExtensionManifest;

    fn make_api(manifest_json: &str) -> AileronExtensionApi {
        let manifest: ExtensionManifest = serde_json::from_str(manifest_json).unwrap();
        AileronExtensionApi::new(ExtensionId("test-extension".into()), manifest)
    }

    #[test]
    fn test_js_runtime_evaluates_simple_script() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(&api, "var x = 42;");
        assert!(rt.is_ok(), "Should evaluate simple JS: {:?}", rt.err());
    }

    #[test]
    fn test_js_runtime_captures_syntax_error() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(&api, "var x = ;");
        assert!(rt.is_err(), "Should capture syntax error");
        let err = rt.unwrap_err();
        assert!(
            err.contains("syntax") || err.contains("expected") || err.contains("SyntaxError"),
            "Error: {err}"
        );
    }

    #[test]
    fn test_js_runtime_chrome_shim_injected() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(
            &api,
            "chrome.runtime.onInstalled.addListener(function() {});",
        );
        assert!(
            rt.is_ok(),
            "chrome.runtime.onInstalled.addListener should work"
        );
    }

    #[test]
    fn test_js_runtime_fire_event_noop_when_no_listener() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(&api, "var x = 1;").unwrap();
        rt.fire_event("onInstalled", "{}");
    }

    #[test]
    fn test_js_runtime_storage_shim() {
        let api = make_api(
            r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0", "permissions": ["storage"]}"#,
        );
        let rt = JsRuntime::new(
            &api,
            "chrome.storage.local.set({foo: 'bar'}, function() {}); var result = true;",
        );
        assert!(rt.is_ok(), "storage.local.set should not throw");
    }

    #[test]
    fn test_js_runtime_alarms_shim() {
        let api = make_api(
            r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0", "permissions": ["alarms"]}"#,
        );
        let rt = JsRuntime::new(&api, "chrome.alarms.create('test', {delayInMinutes: 1});");
        assert!(rt.is_ok(), "alarms.create should not throw");
    }

    #[test]
    fn test_js_runtime_runtime_error() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(&api, "undefined_function();");
        assert!(rt.is_err(), "Should capture reference error");
    }

    #[test]
    fn test_js_runtime_fire_on_installed() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(
            &api,
            "var __install_count = 0; \
             chrome.runtime.onInstalled.addListener(function(details) { \
                 __install_count++; \
             });",
        )
        .unwrap();

        rt.fire_on_installed("install", &ExtensionId("test-extension".into()));

        rt.ctx.with(|cx| {
            let count: i32 = cx.globals().get::<_, i32>("__install_count").unwrap_or(0);
            assert_eq!(count, 1, "onInstalled listener should have fired");
        });
    }

    #[test]
    fn test_js_runtime_fire_on_startup() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(
            &api,
            "var __startup_fired = false; \
             chrome.runtime.onStartup.addListener(function() { \
                 __startup_fired = true; \
             });",
        )
        .unwrap();

        rt.fire_on_startup();

        rt.ctx.with(|cx| {
            let fired: bool = cx
                .globals()
                .get::<_, bool>("__startup_fired")
                .unwrap_or(false);
            assert!(fired, "onStartup listener should have fired");
        });
    }

    #[test]
    fn test_js_runtime_tabs_shim() {
        let api = make_api(
            r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0", "permissions": ["tabs"]}"#,
        );
        let rt = JsRuntime::new(
            &api,
            "var __tab_result = null; \
             chrome.tabs.query({}, function(tabs) { __tab_result = tabs; });",
        );
        assert!(rt.is_ok(), "tabs.query should not throw");

        let rt = rt.unwrap();
        rt.ctx.with(|cx| {
            let json_str: String = cx
                .eval::<String, _>("JSON.stringify(__tab_result)")
                .unwrap_or_default();
            assert!(
                json_str == "[]" || json_str == "null",
                "Expected [] or null, got: {json_str}"
            );
        });
    }

    #[test]
    fn test_js_runtime_message_bus_drain() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let bus = Arc::new(MessageBus::new());

        let rt = JsRuntime::new(
            &api,
            "var __received_msgs = []; \
             chrome.runtime.onMessage.addListener(function(msg) { \
                 __received_msgs.push(msg); \
             });",
        )
        .unwrap();

        let ext_id = ExtensionId("test-extension".into());
        rt.connect_message_bus(ext_id.clone(), bus.clone());

        // Send a message via the bus
        bus.send_message(
            Some(&ExtensionId("other".into())),
            Some(&ext_id),
            serde_json::json!({"type": "ping"}),
        );

        // Message is queued; drain it
        rt.drain_pending_messages();

        // Verify the JS listener received it
        rt.ctx.with(|cx| {
            let json_str: String = cx
                .eval::<String, _>("JSON.stringify(__received_msgs)")
                .unwrap_or_default();
            assert!(
                json_str.contains("ping"),
                "Should contain ping message, got: {json_str}"
            );
        });
    }

    #[test]
    fn test_js_runtime_connect_creates_port() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(&api, "").unwrap();

        let port_id = rt.create_port_in_js_context("my-port", "test-ext");
        assert!(port_id > 0, "Port ID should be positive, got {port_id}");

        // Verify port exists in JS context
        rt.ctx.with(|cx| {
            let exists: bool = cx
                .eval::<bool, _>("typeof __aileron_port_callbacks[1] !== 'undefined'")
                .unwrap_or(false);
            assert!(exists, "Port should exist in __aileron_port_callbacks");
        });
    }

    #[test]
    fn test_js_runtime_connect_shim_creates_port() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(
            &api,
            "var __port_result = null; \
             chrome.runtime.onConnect.addListener(function(port) { \
                 __port_result = port.name; \
             }); \
             chrome.runtime.connect({name: 'test-port'});",
        )
        .unwrap();

        rt.ctx.with(|cx| {
            let name: String = cx.eval::<String, _>("__port_result").unwrap_or_default();
            assert_eq!(name, "test-port", "Port name should be 'test-port'");
        });
    }

    #[test]
    fn test_js_runtime_connect_port_postmessage() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(
            &api,
            "var __sent_data = null; \
             var __port = chrome.runtime.connect({name: 'msg-port'});",
        )
        .unwrap();

        // The native function is a no-op in the shim, but verify port.postMessage is callable
        rt.ctx.with(|cx| {
            let result: String = cx
                .eval::<String, _>(
                    "try { __port.postMessage({type: 'hello'}); 'ok' } catch(e) { e.message }",
                )
                .unwrap_or_default();
            assert_eq!(result, "ok", "postMessage should not throw");
        });
    }

    #[test]
    fn test_js_runtime_connect_port_disconnect() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(&api, "").unwrap();

        // Create port via Rust API (gives a real portId).
        let port_id = rt.create_port_in_js_context("disc-port", "test-ext");
        assert!(port_id > 0);

        // Register disconnect callback via Rust API.
        let js_code = format!(
            "var __disconnect_fired = false; \
             if (__aileron_port_callbacks[{port_id}]) {{ \
                 __aileron_port_callbacks[{port_id}].disconnect.push(function() {{ \
                     __disconnect_fired = true; \
                 }}); \
             }}"
        );
        rt.ctx.with(|cx| {
            cx.eval::<(), _>(js_code.as_str()).unwrap();
        });

        rt.fire_port_disconnect(port_id);

        rt.ctx.with(|cx| {
            let fired: bool = cx.eval::<bool, _>("__disconnect_fired").unwrap_or(false);
            assert!(fired, "onDisconnect should have fired");
        });
    }

    #[test]
    fn test_js_runtime_fire_incoming_port_message() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(
            &api,
            "var __port_msgs = []; \
             chrome.runtime.onConnect.addListener(function(port) { \
                 port.onMessage.addListener(function(msg) { \
                     __port_msgs.push(msg); \
                 }); \
             }); \
             chrome.runtime.connect({name: 'listener-port'});",
        )
        .unwrap();

        let port_id = rt.create_port_in_js_context("listener-port", "test-ext");

        rt.fire_incoming_port_message(port_id, r#"{"type":"ping"}"#);

        rt.ctx.with(|cx| {
            let json_str: String = cx
                .eval::<String, _>("JSON.stringify(__port_msgs)")
                .unwrap_or_default();
            assert!(
                json_str.contains("ping"),
                "Port should have received message, got: {json_str}"
            );
        });
    }

    #[test]
    fn test_js_runtime_fire_incoming_port_message_no_port() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(&api, "").unwrap();

        // Should not panic when port doesn't exist
        rt.fire_incoming_port_message(999, r#"{"type":"orphan"}"#);
    }

    #[test]
    fn test_js_runtime_port_disconnect_cleans_up() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(
            &api,
            "var __port = chrome.runtime.connect({name: 'cleanup-port'});",
        )
        .unwrap();

        let port_id = rt.create_port_in_js_context("cleanup-port", "test-ext");

        rt.fire_port_disconnect(port_id);

        // Port should be removed from JS context
        rt.ctx.with(|cx| {
            let exists: bool = cx
                .eval::<bool, _>("typeof __aileron_port_callbacks[2] !== 'undefined' && __aileron_port_callbacks[2] !== null")
                .unwrap_or(true);
            assert!(!exists, "Port should be removed after disconnect");
        });

        // Port should be removed from Rust PortManager
        assert!(
            rt.port_manager().remove_port(port_id).is_none(),
            "Port should be removed from PortManager"
        );
    }

    #[test]
    fn test_js_runtime_port_sender() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(&api, "").unwrap();

        rt.create_port_in_js_context("sender-port", "my-extension");

        rt.ctx.with(|cx| {
            let ext_id: String = cx
                .eval::<String, _>("__aileron_port_callbacks[1].sender.id")
                .unwrap_or_default();
            assert_eq!(
                ext_id, "my-extension",
                "Port sender should have extension ID"
            );
        });
    }

    #[test]
    fn test_js_runtime_onconnect_listener() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(
            &api,
            "var __connect_count = 0; \
             chrome.runtime.onConnect.addListener(function(port) { \
                 __connect_count++; \
             });",
        )
        .unwrap();

        rt.create_port_in_js_context("port-a", "ext-a");
        rt.create_port_in_js_context("port-b", "ext-b");

        rt.ctx.with(|cx| {
            let count: i32 = cx.eval::<i32, _>("__connect_count").unwrap_or(0);
            assert_eq!(count, 2, "onConnect should fire for each connection");
        });
    }

    #[test]
    fn test_js_runtime_multiple_port_messages() {
        let api = make_api(r#"{"manifest_version": 3, "name": "Test", "version": "1.0.0"}"#);
        let rt = JsRuntime::new(
            &api,
            "var __msgs = []; \
             chrome.runtime.onConnect.addListener(function(port) { \
                 port.onMessage.addListener(function(msg) { \
                     __msgs.push(msg); \
                 }); \
             }); \
             chrome.runtime.connect({name: 'multi-port'});",
        )
        .unwrap();

        let port_id = rt.create_port_in_js_context("multi-port", "test-ext");

        rt.fire_incoming_port_message(port_id, r#"{"n":1}"#);
        rt.fire_incoming_port_message(port_id, r#"{"n":2}"#);
        rt.fire_incoming_port_message(port_id, r#"{"n":3}"#);

        rt.ctx.with(|cx| {
            let count: i32 = cx.eval::<i32, _>("__msgs.length").unwrap_or(0);
            assert_eq!(count, 3, "Should receive all 3 messages");
        });
    }
}
