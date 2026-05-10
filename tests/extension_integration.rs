use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use aileron::extensions::*;

fn make_extension_dir(
    dir: &tempfile::TempDir,
    name: &str,
    manifest_json: &str,
) -> std::path::PathBuf {
    let ext_dir = dir.path().join(name);
    std::fs::create_dir_all(&ext_dir).unwrap();
    std::fs::write(ext_dir.join("manifest.json"), manifest_json).unwrap();
    ext_dir
}

// ─── 1. Load a valid extension from a temp directory ───────────────────

#[test]
fn test_load_valid_extension_with_background_script() {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = make_extension_dir(
        &dir,
        "valid-ext",
        r#"{
            "manifest_version": 3,
            "name": "Valid Extension",
            "version": "1.0.0",
            "background": {
                "service_worker": "background.js"
            }
        }"#,
    );
    std::fs::write(ext_dir.join("background.js"), "console.log('hello');").unwrap();

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].as_ref(), "valid-ext");

    let api = manager.get(&loaded[0]).unwrap();
    assert_eq!(api.manifest().name, "Valid Extension");
    assert_eq!(api.manifest().version, "1.0.0");

    let bg = api
        .background_script()
        .expect("background script should be loaded");
    assert_eq!(bg.filename, "background.js");
    assert!(bg.source.contains("hello"));
}

#[test]
fn test_load_valid_extension_mv2_background_scripts() {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = make_extension_dir(
        &dir,
        "mv2-ext",
        r#"{
            "manifest_version": 3,
            "name": "MV2 Fallback Extension",
            "version": "2.0.0",
            "background": {
                "scripts": ["bg.js", "extra.js"]
            }
        }"#,
    );
    std::fs::write(ext_dir.join("bg.js"), "// primary").unwrap();
    std::fs::write(ext_dir.join("extra.js"), "// secondary").unwrap();

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();

    assert_eq!(loaded.len(), 1);
    let bg = manager
        .get(&loaded[0])
        .unwrap()
        .background_script()
        .unwrap();
    assert_eq!(bg.filename, "bg.js");
}

// ─── 2. Reject extension with missing required fields ──────────────────

#[test]
fn test_reject_missing_version_field() {
    let dir = tempfile::tempdir().unwrap();
    make_extension_dir(
        &dir,
        "no-version",
        r#"{
            "manifest_version": 3,
            "name": "No Version Extension"
        }"#,
    );

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();
    assert!(
        loaded.is_empty(),
        "extension missing 'version' should be rejected"
    );
}

#[test]
fn test_reject_missing_name_field() {
    let dir = tempfile::tempdir().unwrap();
    make_extension_dir(
        &dir,
        "no-name",
        r#"{
            "manifest_version": 3,
            "version": "1.0.0"
        }"#,
    );

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();
    assert!(
        loaded.is_empty(),
        "extension missing 'name' should be rejected"
    );
}

#[test]
fn test_reject_missing_manifest_version() {
    let dir = tempfile::tempdir().unwrap();
    make_extension_dir(
        &dir,
        "no-mv",
        r#"{
            "name": "No MV Extension",
            "version": "1.0.0"
        }"#,
    );

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();
    assert!(
        loaded.is_empty(),
        "extension missing 'manifest_version' should be rejected"
    );
}

#[test]
fn test_reject_invalid_json() {
    let dir = tempfile::tempdir().unwrap();
    make_extension_dir(&dir, "bad-json", "this is not json at all {{{");

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();
    assert!(
        loaded.is_empty(),
        "invalid JSON manifest should be rejected"
    );
}

#[test]
fn test_reject_completely_empty_manifest() {
    let dir = tempfile::tempdir().unwrap();
    make_extension_dir(&dir, "empty-manifest", "{}");

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();
    assert!(loaded.is_empty(), "empty manifest should be rejected");
}

// ─── 3. Content script URL matching ────────────────────────────────────

#[test]
fn test_content_script_url_matching_correct_urls() {
    let registry = ExtensionContentScriptRegistry::new();

    registry.register(ExtensionContentScriptEntry {
        extension_id: "matcher-ext".into(),
        script_id: "matcher-ext-0".into(),
        js_code: "console.log('matched');".into(),
        css_code: String::new(),
        matches: vec!["https://*.example.com/*".into()],
        run_at: ExtensionRunAt::DocumentIdle,
    });

    let matching_urls = [
        "https://www.example.com/page",
        "https://api.example.com/v1/resource",
        "https://deep.nested.sub.example.com/path?q=1",
    ];

    for url in &matching_urls {
        let scripts = registry.scripts_for_url(url, ExtensionRunAt::DocumentIdle);
        assert_eq!(
            scripts.len(),
            1,
            "URL '{url}' should match the pattern https://*.example.com/*"
        );
    }
}

#[test]
fn test_content_script_url_rejects_non_matching_urls() {
    let registry = ExtensionContentScriptRegistry::new();

    registry.register(ExtensionContentScriptEntry {
        extension_id: "matcher-ext".into(),
        script_id: "matcher-ext-0".into(),
        js_code: String::new(),
        css_code: String::new(),
        matches: vec!["https://*.example.com/*".into()],
        run_at: ExtensionRunAt::DocumentIdle,
    });

    let non_matching_urls = [
        "https://other.com/",
        "http://www.example.com/", // wrong scheme
        "https://example.org/",
        "ftp://example.com/file",
    ];

    for url in &non_matching_urls {
        let scripts = registry.scripts_for_url(url, ExtensionRunAt::DocumentIdle);
        assert!(
            scripts.is_empty(),
            "URL '{url}' should NOT match the pattern https://*.example.com/*"
        );
    }
}

#[test]
fn test_content_script_url_matching_filters_by_run_at() {
    let registry = ExtensionContentScriptRegistry::new();

    registry.register(ExtensionContentScriptEntry {
        extension_id: "timing-ext".into(),
        script_id: "timing-ext-start".into(),
        js_code: "console.log('start');".into(),
        css_code: String::new(),
        matches: vec!["https://*.test.com/*".into()],
        run_at: ExtensionRunAt::DocumentStart,
    });

    registry.register(ExtensionContentScriptEntry {
        extension_id: "timing-ext".into(),
        script_id: "timing-ext-end".into(),
        js_code: "console.log('end');".into(),
        css_code: String::new(),
        matches: vec!["https://*.test.com/*".into()],
        run_at: ExtensionRunAt::DocumentEnd,
    });

    let url = "https://www.test.com/page";

    let start_scripts = registry.scripts_for_url(url, ExtensionRunAt::DocumentStart);
    assert_eq!(start_scripts.len(), 1);
    assert_eq!(start_scripts[0].script_id, "timing-ext-start");

    let end_scripts = registry.scripts_for_url(url, ExtensionRunAt::DocumentEnd);
    assert_eq!(end_scripts.len(), 1);
    assert_eq!(end_scripts[0].script_id, "timing-ext-end");

    let idle_scripts = registry.scripts_for_url(url, ExtensionRunAt::DocumentIdle);
    assert!(idle_scripts.is_empty(), "no idle scripts registered");
}

#[test]
fn test_content_script_multiple_patterns_match() {
    let registry = ExtensionContentScriptRegistry::new();

    registry.register(ExtensionContentScriptEntry {
        extension_id: "multi-ext".into(),
        script_id: "multi-ext-0".into(),
        js_code: String::new(),
        css_code: String::new(),
        matches: vec![
            "https://*.github.com/*".into(),
            "https://*.gitlab.com/*".into(),
        ],
        run_at: ExtensionRunAt::DocumentIdle,
    });

    assert_eq!(
        registry
            .scripts_for_url("https://api.github.com/repos", ExtensionRunAt::DocumentIdle)
            .len(),
        1,
        "should match github pattern"
    );
    assert_eq!(
        registry
            .scripts_for_url(
                "https://www.gitlab.com/user/project",
                ExtensionRunAt::DocumentIdle
            )
            .len(),
        1,
        "should match gitlab pattern"
    );
    assert!(
        registry
            .scripts_for_url("https://bitbucket.org/repo", ExtensionRunAt::DocumentIdle)
            .is_empty(),
        "should not match bitbucket"
    );
}

#[test]
fn test_content_scripts_registered_via_manager_load() {
    let dir = tempfile::tempdir().unwrap();
    let ext_dir = make_extension_dir(
        &dir,
        "cs-ext",
        r#"{
            "manifest_version": 3,
            "name": "Content Script Ext",
            "version": "1.0.0",
            "content_scripts": [{
                "matches": ["https://*.test.local/*"],
                "js": ["inject.js"],
                "css": ["style.css"],
                "run_at": "document_start"
            }]
        }"#,
    );
    std::fs::write(ext_dir.join("inject.js"), "console.log('injected');").unwrap();
    std::fs::write(ext_dir.join("style.css"), "body { margin: 0; }").unwrap();

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();
    assert_eq!(loaded.len(), 1);

    let registry = manager.content_script_registry();
    let all = registry.all_scripts();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].extension_id, "cs-ext");
    assert!(all[0].js_code.contains("injected"));
    assert!(all[0].css_code.contains("margin"));

    let matched =
        registry.scripts_for_url("https://app.test.local/page", ExtensionRunAt::DocumentStart);
    assert_eq!(matched.len(), 1);
}

// ─── 4. Message bus: register handler, send, receive response ──────────

#[test]
fn test_message_bus_direct_send_and_response() {
    let bus = Arc::new(MessageBus::new());
    let target_id = ExtensionId("responder".into());
    let sender_id = ExtensionId("caller".into());

    bus.register_handler(
        target_id.clone(),
        Box::new(|msg| {
            if msg.as_str() == Some("ping") {
                Some(serde_json::json!({"status": "ok", "echo": "pong"}))
            } else {
                None
            }
        }),
    );

    let response = bus.send_message(
        Some(&sender_id),
        Some(&target_id),
        serde_json::json!("ping"),
    );

    assert!(response.is_some());
    let resp = response.unwrap();
    assert_eq!(resp["status"], "ok");
    assert_eq!(resp["echo"], "pong");
}

#[test]
fn test_message_bus_send_to_unregistered_returns_none() {
    let bus = MessageBus::new();
    let sender_id = ExtensionId("caller".into());
    let unknown_id = ExtensionId("nobody-home".into());

    let response = bus.send_message(
        Some(&sender_id),
        Some(&unknown_id),
        serde_json::json!("hello"),
    );

    assert!(
        response.is_none(),
        "sending to unregistered extension should return None"
    );
}

#[test]
fn test_message_bus_broadcast_skips_source() {
    let bus = Arc::new(MessageBus::new());

    let call_count = Arc::new(AtomicUsize::new(0));

    let count_a = call_count.clone();
    bus.register_handler(
        ExtensionId("ext-a".into()),
        Box::new(move |_| {
            count_a.fetch_add(1, Ordering::Relaxed);
            None
        }),
    );

    let count_b = call_count.clone();
    bus.register_handler(
        ExtensionId("ext-b".into()),
        Box::new(move |_| {
            count_b.fetch_add(1, Ordering::Relaxed);
            Some(serde_json::json!("ack"))
        }),
    );

    let count_c = call_count.clone();
    bus.register_handler(
        ExtensionId("ext-c".into()),
        Box::new(move |_| {
            count_c.fetch_add(1, Ordering::Relaxed);
            None
        }),
    );

    // Broadcast from ext-a — should reach ext-b and ext-c, but not ext-a
    let response = bus.send_message(
        Some(&ExtensionId("ext-a".into())),
        None,
        serde_json::json!("broadcast"),
    );

    assert_eq!(
        call_count.load(Ordering::Relaxed),
        2,
        "broadcast should skip source"
    );
    assert_eq!(
        response,
        Some(serde_json::json!("ack")),
        "last response should be returned"
    );
}

#[test]
fn test_message_bus_multiple_handlers_first_responder_wins() {
    let bus = Arc::new(MessageBus::new());
    let target = ExtensionId("multi-handler".into());

    bus.register_handler(
        target.clone(),
        Box::new(|_| Some(serde_json::json!("first"))),
    );
    bus.register_handler(
        target.clone(),
        Box::new(|_| Some(serde_json::json!("second"))),
    );

    let response = bus.send_message(
        Some(&ExtensionId("sender".into())),
        Some(&target),
        serde_json::json!("msg"),
    );

    assert_eq!(
        response,
        Some(serde_json::json!("first")),
        "first handler to respond should win"
    );
}

#[test]
fn test_message_bus_handler_lifecycle() {
    let bus = MessageBus::new();
    let ext_id = ExtensionId("lifecycle-ext".into());

    assert!(!bus.has_handlers(&ext_id));

    bus.register_handler(ext_id.clone(), Box::new(|_| None));
    assert!(bus.has_handlers(&ext_id));

    bus.remove_handlers(&ext_id);
    assert!(
        !bus.has_handlers(&ext_id),
        "handlers should be removed after unload"
    );
}

#[test]
fn test_message_bus_via_manager() {
    let dir = tempfile::tempdir().unwrap();
    make_extension_dir(
        &dir,
        "msg-ext",
        r#"{
            "manifest_version": 3,
            "name": "Message Ext",
            "version": "1.0.0"
        }"#,
    );

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    manager.load_all();

    let bus = manager.message_bus();
    let ext_id = ExtensionId("msg-ext".into());

    bus.register_handler(
        ext_id.clone(),
        Box::new(|msg| {
            if msg.as_str() == Some("test") {
                Some(serde_json::json!(42))
            } else {
                None
            }
        }),
    );

    let response = bus.send_message(
        Some(&ExtensionId("external".into())),
        Some(&ext_id),
        serde_json::json!("test"),
    );
    assert_eq!(response, Some(serde_json::json!(42)));
}

// ─── 5. Extension lifecycle: load, fire installed callback, state ──────

#[test]
fn test_lifecycle_load_fire_installed_verify_state() {
    let dir = tempfile::tempdir().unwrap();
    make_extension_dir(
        &dir,
        "lifecycle-ext",
        r#"{
            "manifest_version": 3,
            "name": "Lifecycle Extension",
            "version": "1.0.0"
        }"#,
    );

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    assert_eq!(manager.count(), 0);

    let loaded = manager.load_all();
    assert_eq!(loaded.len(), 1);
    assert_eq!(manager.count(), 1);

    let ext_id = loaded[0].clone();
    let api = manager.get(&ext_id).unwrap();
    assert_eq!(api.manifest().name, "Lifecycle Extension");
    assert_eq!(api.id().as_ref(), "lifecycle-ext");
}

#[test]
fn test_lifecycle_installed_callback_fires() {
    let dir = tempfile::tempdir().unwrap();
    make_extension_dir(
        &dir,
        "install-cb-ext",
        r#"{
            "manifest_version": 3,
            "name": "Install Callback Ext",
            "version": "1.0.0"
        }"#,
    );

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();
    assert_eq!(loaded.len(), 1);

    let fire_count = Arc::new(AtomicUsize::new(0));
    let count_clone = fire_count.clone();
    let ext_id = loaded[0].clone();

    manager
        .get(&ext_id)
        .unwrap()
        .runtime()
        .on_installed(Arc::new(move |details| {
            assert_eq!(details.reason, InstallReason::Install);
            assert!(details.previous_version.is_none());
            count_clone.fetch_add(1, Ordering::SeqCst);
        }));

    manager
        .get(&ext_id)
        .unwrap()
        .fire_installed(InstalledDetails {
            reason: InstallReason::Install,
            previous_version: None,
            id: ext_id.clone(),
        });

    assert_eq!(
        fire_count.load(Ordering::SeqCst),
        1,
        "on_installed callback should have fired once"
    );
}

#[test]
fn test_lifecycle_startup_fires_for_all_loaded_extensions() {
    let dir = tempfile::tempdir().unwrap();

    for name in &["startup-a", "startup-b"] {
        make_extension_dir(
            &dir,
            name,
            &format!(
                r#"{{
                "manifest_version": 3,
                "name": "Startup {name}",
                "version": "1.0.0"
            }}"#
            ),
        );
    }

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();
    assert_eq!(loaded.len(), 2);

    let fire_count = Arc::new(AtomicUsize::new(0));
    for id in &loaded {
        let count_clone = fire_count.clone();
        manager
            .get(id)
            .unwrap()
            .runtime()
            .on_startup(Arc::new(move || {
                count_clone.fetch_add(1, Ordering::SeqCst);
            }));
    }

    manager.fire_all_startup();
    assert_eq!(
        fire_count.load(Ordering::SeqCst),
        2,
        "both extensions' on_startup should fire"
    );
}

#[test]
fn test_lifecycle_unload_removes_extension() {
    let dir = tempfile::tempdir().unwrap();
    make_extension_dir(
        &dir,
        "unload-ext",
        r#"{
            "manifest_version": 3,
            "name": "Unload Extension",
            "version": "1.0.0"
        }"#,
    );

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();
    assert_eq!(loaded.len(), 1);
    assert_eq!(manager.count(), 1);

    let name = manager.unload(&loaded[0]);
    assert_eq!(name, Some("Unload Extension".to_string()));
    assert_eq!(manager.count(), 0);
    assert!(manager.get(&loaded[0]).is_none());
}

#[test]
fn test_lifecycle_load_unload_reload() {
    let dir = tempfile::tempdir().unwrap();
    make_extension_dir(
        &dir,
        "reload-ext",
        r#"{
            "manifest_version": 3,
            "name": "Reload Extension",
            "version": "1.0.0"
        }"#,
    );

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());

    // Load
    let loaded = manager.load_all();
    assert_eq!(loaded.len(), 1);
    let ext_id = loaded[0].clone();

    // Unload
    manager.unload(&ext_id);
    assert_eq!(manager.count(), 0);

    // Reload via load_all
    let reloaded = manager.load_all();
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0], ext_id);
    assert_eq!(manager.count(), 1);

    let api = manager.get(&ext_id).unwrap();
    assert_eq!(api.manifest().name, "Reload Extension");
}

#[test]
fn test_lifecycle_multiple_extensions_independent_state() {
    let dir = tempfile::tempdir().unwrap();

    for (name, ver) in &[
        ("alpha-ext", "1.0.0"),
        ("beta-ext", "2.3.4"),
        ("gamma-ext", "0.1.0"),
    ] {
        make_extension_dir(
            &dir,
            name,
            &format!(
                r#"{{
                "manifest_version": 3,
                "name": "{name}",
                "version": "{ver}"
            }}"#
            ),
        );
    }

    let mut manager = ExtensionManager::new(dir.path().to_path_buf());
    let loaded = manager.load_all();
    assert_eq!(loaded.len(), 3);

    // Each extension has independent manifest
    for id in &loaded {
        let api = manager.get(id).unwrap();
        assert!(
            api.manifest()
                .name
                .contains(id.as_ref().split('-').next().unwrap())
        );
    }

    // Unloading one doesn't affect others
    manager.unload(&loaded[1]);
    assert_eq!(manager.count(), 2);
    assert!(manager.get(&loaded[0]).is_some());
    assert!(manager.get(&loaded[1]).is_none());
    assert!(manager.get(&loaded[2]).is_some());
}
