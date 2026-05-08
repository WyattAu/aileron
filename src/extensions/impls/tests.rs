use super::storage::AileronStorageArea;
use super::*;
use crate::extensions::storage::{StorageArea, StorageGetKeys};
use std::collections::HashMap;
use url::Url;

use crate::extensions::scripting::{CssInjection, InjectionTarget, ScriptInjection};
use crate::extensions::tabs::TabQuery;
use crate::extensions::types::UrlPattern;
use crate::extensions::web_request::{BlockingResponse, RequestDetails, RequestFilter};

const MINIMAL_MANIFEST: &str = r#"{
    "manifest_version": 3,
    "name": "Test Extension",
    "version": "1.0.0",
    "permissions": ["storage", "tabs", "scripting", "webRequest"]
}"#;

fn make_api() -> AileronExtensionApi {
    let manifest = ExtensionManifest::from_json(MINIMAL_MANIFEST).unwrap();
    AileronExtensionApi::new(ExtensionId("test@example.com".into()), manifest)
}

#[test]
fn test_api_creation() {
    let api = make_api();
    assert_eq!(api.extension_id().as_ref(), "test@example.com");
    assert_eq!(api.manifest().name, "Test Extension");
    assert_eq!(api.manifest().version, "1.0.0");
    assert_eq!(api.id().as_ref(), "test@example.com");
}

#[test]
fn test_storage_get_set_clear() {
    let api = make_api();

    let result = api.storage().local().get(StorageGetKeys::All).unwrap();
    assert!(result.is_empty());

    let mut items = HashMap::new();
    items.insert("key1".into(), serde_json::Value::String("value1".into()));
    api.storage().local().set(items).unwrap();

    let result = api
        .storage()
        .local()
        .get(StorageGetKeys::Single("key1".into()))
        .unwrap();
    assert_eq!(
        result.get("key1").unwrap(),
        &serde_json::Value::String("value1".into())
    );

    let result = api
        .storage()
        .local()
        .get(StorageGetKeys::Single("nonexistent".into()))
        .unwrap();
    assert!(result.is_empty());

    api.storage().local().clear().unwrap();
    let result = api.storage().local().get(StorageGetKeys::All).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_storage_remove() {
    let api = make_api();

    let mut items = HashMap::new();
    items.insert("a".into(), serde_json::json!(1));
    items.insert("b".into(), serde_json::json!(2));
    api.storage().local().set(items).unwrap();

    api.storage().local().remove(vec!["a".into()]).unwrap();

    let result = api.storage().local().get(StorageGetKeys::All).unwrap();
    assert_eq!(result.len(), 1);
    assert!(result.contains_key("b"));
}

#[test]
fn test_storage_get_bytes_in_use() {
    let api = make_api();

    let mut items = HashMap::new();
    items.insert("key".into(), serde_json::Value::String("value".into()));
    api.storage().local().set(items).unwrap();

    let bytes = api
        .storage()
        .local()
        .get_bytes_in_use(Some(vec!["key".into()]))
        .unwrap();
    assert!(bytes > 0);

    let all_bytes = api.storage().local().get_bytes_in_use(None).unwrap();
    assert_eq!(bytes, all_bytes);
}

#[test]
fn test_tabs_query_empty() {
    let api = make_api();
    let tabs = api.tabs().query(TabQuery::default()).unwrap();
    assert!(tabs.is_empty());
}

#[test]
fn test_runtime_get_id_and_manifest() {
    let api = make_api();
    assert_eq!(api.runtime().get_id().as_ref(), "test@example.com");
    let m = api.runtime().get_manifest().unwrap();
    assert_eq!(m.name, "Test Extension");
}

#[test]
fn test_runtime_get_url() {
    let api = make_api();
    let url = api.runtime().get_url("styles.css").unwrap();
    assert_eq!(
        url.as_str(),
        "aileron://extensions/test@example.com/styles.css"
    );
}

#[test]
fn test_scripting_get_registered_empty() {
    let api = make_api();
    let scripts = api
        .scripting()
        .get_registered_content_scripts(None)
        .unwrap();
    assert!(scripts.is_empty());
}

#[test]
fn test_web_request_remove_listener_not_found() {
    let api = make_api();
    let result = api.web_request().remove_listener(ListenerId(999));
    assert!(result.is_err());
}

// ── Persistent Storage Tests ──

fn make_persistent_api(dir: &std::path::Path) -> AileronExtensionApi {
    let manifest = ExtensionManifest::from_json(MINIMAL_MANIFEST).unwrap();
    AileronExtensionApi::with_registry_and_storage(
        ExtensionId("test-persist".into()),
        manifest,
        ExtensionContentScriptRegistry::new(),
        Some(dir.to_path_buf()),
        None,
        None,
    )
}

#[test]
fn test_persistent_storage_set_and_reload() {
    let dir = std::env::temp_dir().join("aileron_test_persist_set");
    let _ = std::fs::remove_dir_all(&dir);

    // Write data
    {
        let api = make_persistent_api(&dir);
        let mut items = HashMap::new();
        items.insert("key1".into(), serde_json::json!("hello"));
        items.insert("key2".into(), serde_json::json!(42));
        api.storage().local().set(items).unwrap();
    }

    // Reload and verify
    {
        let api = make_persistent_api(&dir);
        let result = api.storage().local().get(StorageGetKeys::All).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("key1").unwrap(), &serde_json::json!("hello"));
        assert_eq!(result.get("key2").unwrap(), &serde_json::json!(42));
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_persistent_storage_remove_and_reload() {
    let dir = std::env::temp_dir().join("aileron_test_persist_remove");
    let _ = std::fs::remove_dir_all(&dir);

    // Write data
    {
        let api = make_persistent_api(&dir);
        let mut items = HashMap::new();
        items.insert("a".into(), serde_json::json!(1));
        items.insert("b".into(), serde_json::json!(2));
        api.storage().local().set(items).unwrap();
        api.storage().local().remove(vec!["a".into()]).unwrap();
    }

    // Reload and verify only "b" remains
    {
        let api = make_persistent_api(&dir);
        let result = api.storage().local().get(StorageGetKeys::All).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("b"));
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_persistent_storage_clear_and_reload() {
    let dir = std::env::temp_dir().join("aileron_test_persist_clear");
    let _ = std::fs::remove_dir_all(&dir);

    // Write data then clear
    {
        let api = make_persistent_api(&dir);
        let mut items = HashMap::new();
        items.insert("x".into(), serde_json::json!("deleted"));
        api.storage().local().set(items).unwrap();
        api.storage().local().clear().unwrap();
    }

    // Reload and verify empty
    {
        let api = make_persistent_api(&dir);
        let result = api.storage().local().get(StorageGetKeys::All).unwrap();
        assert!(result.is_empty());
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_persistent_storage_separate_areas() {
    let dir = std::env::temp_dir().join("aileron_test_persist_areas");
    let _ = std::fs::remove_dir_all(&dir);

    {
        let api = make_persistent_api(&dir);
        let mut items = HashMap::new();
        items.insert("key".into(), serde_json::json!("local_value"));
        api.storage().local().set(items.clone()).unwrap();
        items.insert("key".into(), serde_json::json!("sync_value"));
        api.storage().sync().set(items).unwrap();
    }

    {
        let api = make_persistent_api(&dir);
        let local = api
            .storage()
            .local()
            .get(StorageGetKeys::Single("key".into()))
            .unwrap();
        assert_eq!(local.get("key").unwrap(), &serde_json::json!("local_value"));
        let sync = api
            .storage()
            .sync()
            .get(StorageGetKeys::Single("key".into()))
            .unwrap();
        assert_eq!(sync.get("key").unwrap(), &serde_json::json!("sync_value"));
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_persistent_storage_corrupted_file_graceful() {
    let dir = std::env::temp_dir().join("aileron_test_persist_corrupt");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);

    // Write garbage to the storage file
    let file_path = dir.join("test-persist").join("local.json");
    let _ = std::fs::create_dir_all(file_path.parent().unwrap());
    std::fs::write(&file_path, "this is not json {{{").unwrap();

    // Should load gracefully with empty data
    let api = make_persistent_api(&dir);
    let result = api.storage().local().get(StorageGetKeys::All).unwrap();
    assert!(result.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_storage_change_callback_fired_on_set() {
    let api = make_api();
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let call_count = Arc::new(AtomicUsize::new(0));
    let count_clone = call_count.clone();

    api.storage()
        .local()
        .on_changed(Arc::new(move |_changes, _area| {
            count_clone.fetch_add(1, Ordering::Relaxed);
        }));

    let mut items = HashMap::new();
    items.insert("key".into(), serde_json::json!("value"));
    api.storage().local().set(items).unwrap();

    assert_eq!(call_count.load(Ordering::Relaxed), 1);
}

#[test]
fn test_storage_change_callback_fired_on_remove() {
    let api = make_api();
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let call_count = Arc::new(AtomicUsize::new(0));
    let count_clone = call_count.clone();

    api.storage()
        .local()
        .on_changed(Arc::new(move |_changes, _area| {
            count_clone.fetch_add(1, Ordering::Relaxed);
        }));

    let mut items = HashMap::new();
    items.insert("key".into(), serde_json::json!("value"));
    api.storage().local().set(items).unwrap();

    api.storage().local().remove(vec!["key".into()]).unwrap();
    assert_eq!(call_count.load(Ordering::Relaxed), 2);
}

#[test]
fn test_storage_change_callback_not_fired_on_clear_empty() {
    let api = make_api();
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let call_count = Arc::new(AtomicUsize::new(0));
    let count_clone = call_count.clone();

    api.storage()
        .local()
        .on_changed(Arc::new(move |_changes, _area| {
            count_clone.fetch_add(1, Ordering::Relaxed);
        }));

    // Clear empty storage — no callback should fire
    api.storage().local().clear().unwrap();
    assert_eq!(call_count.load(Ordering::Relaxed), 0);
}

#[test]
fn test_web_request_handler_storage_and_firing() {
    let api = make_api();
    let filter = RequestFilter {
        urls: vec![UrlPattern("*://*.example.com/*".into())],
        types: None,
        tab_id: None,
        window_id: None,
    };

    // Register a handler that cancels matching requests
    let listener_id = api.web_request().on_before_request(
        filter.clone(),
        vec![],
        Box::new(|details| {
            if details.url.host_str() == Some("blocked.example.com") {
                BlockingResponse {
                    cancel: Some(true),
                    ..Default::default()
                }
            } else {
                BlockingResponse::default()
            }
        }),
    );

    // Fire a blocked request
    let _details = RequestDetails {
        request_id: crate::extensions::types::RequestId(1),
        url: Url::parse("https://blocked.example.com/page").unwrap(),
        method: "GET".into(),
        frame_id: crate::extensions::types::FrameId(0),
        parent_frame_id: crate::extensions::types::FrameId(u32::MAX),
        tab_id: None,
        type_: crate::extensions::web_request::ResourceType::MainFrame,
        origin_url: None,
        timestamp: 0.0,
        request_headers: None,
    };

    // Access the inner AileronWebRequestApi via the trait — we can't call
    // fire_on_before_request through the trait, so test via remove_listener
    assert!(api.web_request().remove_listener(listener_id).is_ok());
    // Removing again should fail
    assert!(api.web_request().remove_listener(listener_id).is_err());
}

#[test]
fn test_web_request_multiple_listeners() {
    let api = make_api();

    let filter1 = RequestFilter {
        urls: vec![UrlPattern("*://*.a.com/*".into())],
        types: None,
        tab_id: None,
        window_id: None,
    };
    let filter2 = RequestFilter {
        urls: vec![UrlPattern("*://*.b.com/*".into())],
        types: None,
        tab_id: None,
        window_id: None,
    };

    let id1 = api.web_request().on_before_request(
        filter1,
        vec![],
        Box::new(|_| BlockingResponse::default()),
    );
    let id2 = api.web_request().on_before_request(
        filter2,
        vec![],
        Box::new(|_| BlockingResponse::default()),
    );

    // Both should be removable
    assert!(api.web_request().remove_listener(id1).is_ok());
    assert!(api.web_request().remove_listener(id2).is_ok());
}

#[test]
fn test_url_pattern_matching() {
    assert!(web_request::simple_url_pattern_match(
        "*://*.example.com/*",
        "https://sub.example.com/page"
    ));
    assert!(web_request::simple_url_pattern_match(
        "*://*.example.com/*",
        "https://example.com/page"
    ));
    assert!(!web_request::simple_url_pattern_match(
        "*://*.example.com/*",
        "https://other.com/page"
    ));
    assert!(web_request::simple_url_pattern_match(
        "<all_urls>",
        "https://anything.com/path"
    ));
    assert!(web_request::simple_url_pattern_match(
        "https://example.com/*",
        "https://example.com/page"
    ));
    assert!(!web_request::simple_url_pattern_match(
        "https://example.com/*",
        "http://example.com/page"
    ));
}

#[test]
fn test_scripting_execute_script() {
    let api = make_api();
    let target = InjectionTarget {
        tab_id: crate::extensions::types::TabId(1),
        frame_ids: None,
        all_frames: false,
    };

    let result = api.scripting().execute_script(
        target,
        ScriptInjection::Function {
            func: "function() { return 42; }".into(),
            args: vec![],
        },
    );

    assert!(result.is_ok());
    let results = result.unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_scripting_insert_css() {
    let api = make_api();
    let target = InjectionTarget {
        tab_id: crate::extensions::types::TabId(1),
        frame_ids: None,
        all_frames: false,
    };

    let result = api.scripting().insert_css(
        target.clone(),
        CssInjection::Css {
            css: "body { color: red; }".into(),
        },
    );

    assert!(result.is_ok());
}

#[test]
fn test_scripting_remove_css() {
    let api = make_api();
    let target = InjectionTarget {
        tab_id: crate::extensions::types::TabId(1),
        frame_ids: None,
        all_frames: false,
    };

    let result = api.scripting().remove_css(
        target,
        CssInjection::Css {
            css: "body { color: red; }".into(),
        },
    );

    assert!(result.is_ok());
}

#[test]
fn test_scripting_execute_script_file_unsupported() {
    let api = make_api();
    let target = InjectionTarget {
        tab_id: crate::extensions::types::TabId(1),
        frame_ids: None,
        all_frames: false,
    };

    let result = api.scripting().execute_script(
        target,
        ScriptInjection::File {
            file: "content.js".into(),
        },
    );

    assert!(result.is_err());
}

// ── A04: Extension Messaging Tests ──

#[test]
fn test_content_script_send_message_to_background() {
    use std::sync::Arc;

    let bus = Arc::new(MessageBus::new());

    let target_manifest = ExtensionManifest::from_json(MINIMAL_MANIFEST).unwrap();
    let target_api = AileronExtensionApi::with_registry_and_storage(
        ExtensionId("background-ext".into()),
        target_manifest,
        ExtensionContentScriptRegistry::new(),
        None,
        None,
        Some(bus.clone()),
    );

    target_api
        .runtime()
        .on_message(Arc::new(move |msg, _sender| {
            if msg.as_str() == Some("ping") {
                Some(serde_json::json!("pong"))
            } else {
                None
            }
        }));

    let source_manifest = ExtensionManifest::from_json(MINIMAL_MANIFEST).unwrap();
    let source_api = AileronExtensionApi::with_registry_and_storage(
        ExtensionId("content-ext".into()),
        source_manifest,
        ExtensionContentScriptRegistry::new(),
        None,
        None,
        Some(bus.clone()),
    );

    let response = source_api
        .runtime()
        .send_message(
            Some(ExtensionId("background-ext".into())),
            serde_json::json!("ping"),
        )
        .unwrap();

    assert_eq!(response, Some(serde_json::json!("pong")));
}

#[test]
fn test_content_script_send_message_no_handler() {
    use std::sync::Arc;

    let bus = Arc::new(MessageBus::new());

    let manifest = ExtensionManifest::from_json(MINIMAL_MANIFEST).unwrap();
    let source_api = AileronExtensionApi::with_registry_and_storage(
        ExtensionId("source-ext".into()),
        manifest,
        ExtensionContentScriptRegistry::new(),
        None,
        None,
        Some(bus.clone()),
    );

    let response = source_api
        .runtime()
        .send_message(
            Some(ExtensionId("nobody-ext".into())),
            serde_json::json!("hello"),
        )
        .unwrap();

    assert!(response.is_none());
}

#[test]
fn test_messaging_response_flows_back_through_bus() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let bus = Arc::new(MessageBus::new());
    let received = Arc::new(AtomicBool::new(false));

    let target_manifest = ExtensionManifest::from_json(MINIMAL_MANIFEST).unwrap();
    let target_api = AileronExtensionApi::with_registry_and_storage(
        ExtensionId("handler-ext".into()),
        target_manifest,
        ExtensionContentScriptRegistry::new(),
        None,
        None,
        Some(bus.clone()),
    );

    let rec = received.clone();
    target_api
        .runtime()
        .on_message(Arc::new(move |msg, _sender| {
            if msg
                .as_object()
                .and_then(|o| o.get("type"))
                .and_then(|v| v.as_str())
                == Some("greeting")
            {
                rec.store(true, Ordering::Relaxed);
                Some(serde_json::json!({ "reply": "hello back" }))
            } else {
                None
            }
        }));

    let source_manifest = ExtensionManifest::from_json(MINIMAL_MANIFEST).unwrap();
    let source_api = AileronExtensionApi::with_registry_and_storage(
        ExtensionId("sender-ext".into()),
        source_manifest,
        ExtensionContentScriptRegistry::new(),
        None,
        None,
        Some(bus.clone()),
    );

    let response = source_api
        .runtime()
        .send_message(
            Some(ExtensionId("handler-ext".into())),
            serde_json::json!({ "type": "greeting", "data": 42 }),
        )
        .unwrap();

    assert!(received.load(Ordering::Relaxed));
    assert_eq!(response, Some(serde_json::json!({ "reply": "hello back" })));
}

// ── A05: Storage Persistence Tests ──

#[test]
fn test_storage_area_persistence_direct() {
    let dir = std::env::temp_dir().join("aileron_test_storage_area_direct");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);

    let file_path = dir.join("test_area.json");

    {
        let mut area = AileronStorageArea::with_persistence(file_path.clone());
        area.set_permissions(permissions::parse_permissions(&["storage".into()]));

        let mut items = HashMap::new();
        items.insert("color".into(), serde_json::json!("blue"));
        items.insert("count".into(), serde_json::json!(7));
        area.set(items).unwrap();
    }

    {
        let area2 = AileronStorageArea::with_persistence(file_path.clone());
        let result = area2.get(StorageGetKeys::All).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("color").unwrap(), &serde_json::json!("blue"));
        assert_eq!(result.get("count").unwrap(), &serde_json::json!(7));
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_storage_area_survives_restart() {
    let dir = std::env::temp_dir().join("aileron_test_storage_restart");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);

    let file_path = dir.join("restart.json");

    {
        let mut area = AileronStorageArea::with_persistence(file_path.clone());
        area.set_permissions(permissions::parse_permissions(&["storage".into()]));
        let mut items = HashMap::new();
        items.insert("session".into(), serde_json::json!("active"));
        items.insert("ts".into(), serde_json::json!(12345));
        area.set(items).unwrap();
    }

    let mut area2 = AileronStorageArea::with_persistence(file_path.clone());
    area2.set_permissions(permissions::parse_permissions(&["storage".into()]));

    let session = area2.get(StorageGetKeys::Single("session".into())).unwrap();
    assert_eq!(
        session.get("session").unwrap(),
        &serde_json::json!("active")
    );

    area2.remove(vec!["session".into()]).unwrap();
    area2
        .set(
            [("session".into(), serde_json::json!("restarted"))]
                .into_iter()
                .collect(),
        )
        .unwrap();

    let mut area3 = AileronStorageArea::with_persistence(file_path.clone());
    area3.set_permissions(permissions::parse_permissions(&["storage".into()]));
    let all = area3.get(StorageGetKeys::All).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all.get("session").unwrap(), &serde_json::json!("restarted"));

    let _ = std::fs::remove_dir_all(&dir);
}

// ── A06: Permission Enforcement Tests ──

fn make_api_no_permissions() -> AileronExtensionApi {
    let manifest = ExtensionManifest::from_json(
        r#"{
        "manifest_version": 3,
        "name": "No Perms",
        "version": "1.0.0"
    }"#,
    )
    .unwrap();
    AileronExtensionApi::new(ExtensionId("noperm".into()), manifest)
}

#[test]
fn test_storage_denied_without_permission() {
    let api = make_api_no_permissions();

    let mut items = HashMap::new();
    items.insert("key".into(), serde_json::json!("value"));
    let result = api.storage().local().set(items);
    assert!(result.is_ok(), "Should not error, just skip");

    let data = api
        .storage()
        .local()
        .get(StorageGetKeys::Single("key".into()))
        .unwrap();
    assert!(data.is_empty(), "Data should not have been written");
}

#[test]
fn test_storage_allowed_with_permission() {
    let api = make_api();

    let mut items = HashMap::new();
    items.insert("key".into(), serde_json::json!("value"));
    api.storage().local().set(items).unwrap();

    let data = api
        .storage()
        .local()
        .get(StorageGetKeys::Single("key".into()))
        .unwrap();
    assert_eq!(data.len(), 1);
}

#[test]
fn test_storage_remove_denied_without_permission() {
    let api = make_api_no_permissions();

    let result = api.storage().local().remove(vec!["key".into()]);
    assert!(result.is_ok());
}

#[test]
fn test_storage_clear_denied_without_permission() {
    let api = make_api_no_permissions();

    let result = api.storage().local().clear();
    assert!(result.is_ok());
}

#[test]
fn test_web_request_denied_without_permission() {
    let api = make_api_no_permissions();
    let filter = RequestFilter {
        urls: vec![],
        types: None,
        tab_id: None,
        window_id: None,
    };

    let listener_id = api.web_request().on_before_request(
        filter,
        vec![],
        Box::new(|_| BlockingResponse {
            cancel: Some(true),
            ..Default::default()
        }),
    );

    let remove_result = api.web_request().remove_listener(listener_id);
    assert!(
        remove_result.is_err(),
        "Handler should not have been registered without webRequest permission"
    );
}

#[test]
fn test_web_request_allowed_with_permission() {
    let api = make_api();
    let filter = RequestFilter {
        urls: vec![],
        types: None,
        tab_id: None,
        window_id: None,
    };

    let listener_id = api.web_request().on_before_request(
        filter,
        vec![],
        Box::new(|_| BlockingResponse {
            cancel: Some(true),
            ..Default::default()
        }),
    );

    let remove_result = api.web_request().remove_listener(listener_id);
    assert!(
        remove_result.is_ok(),
        "Handler should be registered with webRequest permission"
    );
}

#[test]
fn test_check_api_permission_storage() {
    let api = make_api();
    assert!(api.check_api_permission("storage", "set").is_ok());
    assert!(api.check_api_permission("storage", "get").is_ok());
}

#[test]
fn test_check_api_permission_webrequest() {
    let api = make_api();
    assert!(
        api.check_api_permission("webRequest", "onBeforeRequest")
            .is_ok()
    );
}

#[test]
fn test_check_api_permission_denied() {
    let api = make_api_no_permissions();
    assert!(api.check_api_permission("storage", "set").is_err());
    assert!(
        api.check_api_permission("webRequest", "onBeforeRequest")
            .is_err()
    );
}

#[test]
fn test_has_permission() {
    let api = make_api();
    assert!(api.has_permission("storage"));
    assert!(api.has_permission("webRequest"));
    assert!(api.has_permission("tabs"));
    assert!(!api.has_permission("bookmarks"));

    let no_perms = make_api_no_permissions();
    assert!(!no_perms.has_permission("storage"));
}
