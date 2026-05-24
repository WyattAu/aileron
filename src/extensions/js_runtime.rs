//! Background script JavaScript runtime using QuickJS.
//!
//! Each extension with a `background` field in its manifest gets an isolated
//! QuickJS context. The runtime exposes a `chrome` global shim that bridges
//! API calls back into the Rust `AileronExtensionApi` implementation.

use std::sync::{Arc, Mutex};

use rquickjs::context::Context;
use rquickjs::{CatchResultExt, Function, Runtime as QjsRuntime};

use crate::extensions::impls::AileronExtensionApi;
use crate::extensions::message_bus::MessageBus;
use crate::extensions::types::{ExtensionId, RuntimeMessage};

/// Handle to an initialised background-script QuickJS context.
pub struct JsRuntime {
    #[allow(dead_code)]
    runtime: QjsRuntime,
    ctx: Context,
    /// Queue of messages received from the MessageBus since last drain.
    pending_messages: Arc<Mutex<Vec<String>>>,
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

        Ok(Self {
            runtime,
            ctx,
            pending_messages: Arc::new(Mutex::new(Vec::new())),
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
            };
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
}
