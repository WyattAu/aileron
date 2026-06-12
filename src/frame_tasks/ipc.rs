use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::app::AppState;
use crate::extensions::{ExtensionId, MessageBus};
use crate::frame_tasks::event_proc::{EventPane, EventPaneManager};
use crate::offscreen_webview::OffscreenWebViewManager;
use crate::servo::WryPaneManager;

pub(crate) const EXTENSION_RUNTIME_SHIM_JS: &str = r#"
(function() {
    if (window.__aileron_ext_shim_loaded) return;
    window.__aileron_ext_shim_loaded = true;
    var _pending = {};
    var _counter = 0;
    function _sendMessage(targetId, message) {
        var reqId = '__aer_req_' + (++_counter);
        return new Promise(function(resolve) {
            _pending[reqId] = resolve;
            window.ipc.postMessage(JSON.stringify({
                t: 'ext-send-message',
                sourceId: window.__aileron_extension_id || null,
                targetId: targetId || null,
                message: message != null ? message : {},
                reqId: reqId
            }));
        });
    }
    window.__aileron_ext_response = function(reqId, response) {
        var resolve = _pending[reqId];
        if (resolve) { delete _pending[reqId]; resolve(response); }
    };
    var rt = {
        sendMessage: _sendMessage,
        id: window.__aileron_extension_id || '',
        getURL: function(path) {
            return 'aileron://extensions/' + (window.__aileron_extension_id || '') + '/' + path;
        }
    };
    if (!window.browser) window.browser = {};
    window.browser.runtime = rt;
    if (!window.chrome) window.chrome = {};
    window.chrome.runtime = rt;
})();
"#;

pub(crate) fn handle_ipc_message_generic<M: EventPaneManager>(
    app_state: &mut AppState,
    panes: &mut M,
    pane_id: Uuid,
    message: &str,
) {
    // Check for navigation error detection from ERROR_MONITOR_JS
    if let Some(error_msg) = message.strip_prefix("__aileron_nav_error__|") {
        let parts: Vec<&str> = error_msg.splitn(2, '|').collect();
        let failed_url = parts.first().copied().unwrap_or("unknown");
        let error_detail = parts.get(1).copied().unwrap_or("Unknown error");
        info!(
            "Navigation error detected in pane {}: {} — {}",
            &pane_id.to_string()[..8],
            failed_url,
            error_detail
        );
        app_state.update_a11y(&format!(
            "Load failed: {}",
            &error_detail[..error_detail.len().min(60)]
        ));
        // Navigate to our error page
        if let Some(pane) = panes.get_mut(&pane_id) {
            let display_msg = format!("Failed to load: {failed_url}\n\n{error_detail}");
            let encoded = urlencoding::encode(&display_msg);
            if let Ok(error_url) = url::Url::parse(&format!("aileron://error?msg={encoded}")) {
                pane.navigate(&error_url);
                pane.mark_dirty();
            }
        }
        return;
    }

    let msg: serde_json::Value = match serde_json::from_str(message) {
        Ok(m) => m,
        Err(_) => return,
    };
    match msg.get("t").and_then(|v| v.as_str()) {
        Some("get-config") => {
            let config_json = if app_state.cache.config_json_dirty {
                app_state.cache.config_json_cache =
                    serde_json::to_string(&app_state.config).unwrap_or_default();
                app_state.cache.config_json_dirty = false;
                app_state.cache.config_json_cache.clone()
            } else {
                app_state.cache.config_json_cache.clone()
            };
            let js = format!(
                "window._aileron_config = {config_json}; window._onConfigLoaded && window._onConfigLoaded(window._aileron_config);"
            );
            if let Some(pane) = panes.get_mut(&pane_id) {
                pane.execute_js_code(&js);
                pane.mark_dirty();
            }
        }
        Some("set-config") => {
            if let Some(config_obj) = msg.get("config") {
                if let Some(v) = config_obj.get("homepage").and_then(|v| v.as_str()) {
                    app_state.config.homepage = v.to_string();
                }
                if let Some(v) = config_obj.get("search_engine").and_then(|v| v.as_str()) {
                    app_state.config.search_engine = v.to_string();
                }
                if let Some(v) = config_obj.get("restore_session").and_then(|v| v.as_bool()) {
                    app_state.config.restore_session = v;
                }
                if let Some(v) = config_obj.get("tab_layout").and_then(|v| v.as_str()) {
                    app_state.config.tab_layout = v.to_string();
                }
                if let Some(v) = config_obj.get("tab_sidebar_width").and_then(|v| v.as_f64()) {
                    app_state.config.tab_sidebar_width = v as f32;
                }
                if let Some(v) = config_obj
                    .get("tab_sidebar_right")
                    .and_then(|v| v.as_bool())
                {
                    app_state.config.tab_sidebar_right = v;
                }
                if let Some(v) = config_obj.get("adblock_enabled").and_then(|v| v.as_bool()) {
                    app_state.config.adblock_enabled = v;
                }
                if let Some(v) = config_obj
                    .get("https_upgrade_enabled")
                    .and_then(|v| v.as_bool())
                {
                    app_state.config.https_upgrade_enabled = v;
                }
                if let Some(v) = config_obj
                    .get("tracking_protection_enabled")
                    .and_then(|v| v.as_bool())
                {
                    app_state.config.tracking_protection_enabled = v;
                }
                if let Some(v) = config_obj.get("devtools").and_then(|v| v.as_bool()) {
                    app_state.config.devtools = v;
                }
                if let Some(v) = config_obj.get("proxy") {
                    app_state.config.proxy =
                        v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("custom_css") {
                    app_state.config.custom_css =
                        v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("engine_selection").and_then(|v| v.as_str()) {
                    app_state.config.engine_selection = v.to_string();
                }
                if let Some(v) = config_obj.get("language") {
                    app_state.config.language =
                        v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("adaptive_quality").and_then(|v| v.as_bool()) {
                    app_state.config.adaptive_quality = v;
                }
                if let Some(v) = config_obj
                    .get("popup_blocker_enabled")
                    .and_then(|v| v.as_bool())
                {
                    app_state.config.popup_blocker_enabled = v;
                }
                if let Some(v) = config_obj
                    .get("adblock_update_interval_hours")
                    .and_then(|v| v.as_u64())
                {
                    app_state.config.adblock_update_interval_hours = v;
                }
                if let Some(v) = config_obj.get("theme").and_then(|v| v.as_str()) {
                    app_state.config.theme = v.to_string();
                }
                if let Some(v) = config_obj
                    .get("adblock_cosmetic_filtering")
                    .and_then(|v| v.as_bool())
                {
                    app_state.config.adblock_cosmetic_filtering = v;
                }
                if let Some(v) = config_obj.get("auto_save").and_then(|v| v.as_bool()) {
                    app_state.config.auto_save = v;
                }
                if let Some(v) = config_obj.get("sync_target").and_then(|v| v.as_str()) {
                    app_state.config.sync_target = v.to_string();
                }
                if let Some(v) = config_obj.get("sync_encrypted").and_then(|v| v.as_bool()) {
                    app_state.config.sync_encrypted = v;
                }
                // sync_passphrase is stored in keyring, not config — handled below
                #[cfg(feature = "passwords")]
                if let Some(v) = config_obj
                    .get("sync_passphrase")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    match crate::passwords::keyring::store_credential("aileron-sync", v) {
                        Ok(()) => {
                            app_state.config.sync_passphrase = v.to_string();
                            info!("Sync passphrase stored in system keyring");
                        }
                        Err(e) => {
                            warn!("Failed to store sync passphrase in keyring: {}", e);
                            app_state.ui.status_message =
                                format!("Failed to store passphrase: {e}");
                        }
                    }
                }
                if let Some(v) = config_obj.get("sync_auto").and_then(|v| v.as_bool()) {
                    app_state.config.sync_auto = v;
                }
                if let Some(v) = config_obj
                    .get("sync_auto_interval_sec")
                    .and_then(|v| v.as_u64())
                {
                    app_state.config.sync_auto_interval_sec = v;
                }
                if let Err(e) = crate::config::Config::save(&app_state.config) {
                    warn!("Failed to save config: {}", e);
                }
                if let Some(pane) = panes.get_mut(&pane_id) {
                    pane.execute_js_code("window._onConfigSaved && window._onConfigSaved();");
                    pane.mark_dirty();
                }
                app_state.ui.status_message = "Settings saved".into();
                app_state.cache.config_json_dirty = true;
            }
        }
        #[cfg(feature = "passwords")]
        Some("credential_save") => {
            if let (Some(username), Some(password), Some(url)) = (
                msg.get("username").and_then(|v| v.as_str()),
                msg.get("password").and_then(|v| v.as_str()),
                msg.get("url").and_then(|v| v.as_str()),
            ) {
                let key = format!("{username}@{url}");
                match crate::passwords::keyring::store_credential(&key, password) {
                    Ok(()) => {
                        info!("Saved credential for {}", username);
                        app_state.ui.status_message = format!("Credential saved for {username}");
                    }
                    Err(e) => {
                        warn!("Failed to store credential: {}", e);
                        app_state.ui.status_message = format!("Credential save failed: {e}");
                    }
                }
            } else {
                app_state.ui.status_message = "No pending credentials to save".into();
            }
        }
        Some("scroll-fraction") => {
            if let Some(frac) = msg.get("frac").and_then(|v| v.as_f64())
                && let Some(mark_char) = app_state.session.pending_mark_set.take()
            {
                let frac = frac.clamp(0.0, 1.0);
                app_state.store_mark_fraction(pane_id, mark_char, frac);
                tracing::debug!("Mark '{}' set at fraction {}", mark_char, frac);
                // Persist to database keyed by URL
                if let Some(pane) = panes.get(&pane_id) {
                    let url = pane.url().to_string();
                    if let Some(ref conn) = app_state.db
                        && let Err(e) =
                            crate::db::scroll_marks::set_scroll_mark(conn, &url, mark_char, frac)
                    {
                        tracing::warn!("Failed to persist scroll mark: {}", e);
                    }
                }
            }
        }
        Some("hint-clicked") => {
            app_state.ui.hint_mode = false;
            app_state.ui.hint_buffer.clear();
            app_state.ui.status_message.clear();
        }
        #[cfg(feature = "passwords")]
        Some("login-form-detected") => {
            if msg
                .get("has_login")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                && pane_id == app_state.wm.active_pane_id()
                && app_state.bitwarden.is_unlocked()
                && let Some(pane) = panes.get_mut(&pane_id)
            {
                let url = pane.url().to_string();
                if let Ok(items) = app_state.bitwarden.search_for_url(&url)
                    && !items.is_empty()
                {
                    app_state.autofill.available = true;
                    if let Some(uid) = msg.get("username_id").and_then(|v| v.as_str()) {
                        app_state.autofill.username_id = uid.to_string();
                    }
                    if let Some(pid) = msg.get("password_id").and_then(|v| v.as_str()) {
                        app_state.autofill.password_id = pid.to_string();
                    }
                    if let Ok(cred) = app_state.bitwarden.get_credential(&items[0].id) {
                        let domain = url::Url::parse(&url)
                            .ok()
                            .and_then(|u| u.domain().map(String::from))
                            .unwrap_or_else(|| "unknown".into());
                        let js = app_state.bitwarden.autofill_by_id_js(
                            &app_state.autofill.username_id,
                            &app_state.autofill.password_id,
                            &cred,
                        );
                        app_state.autofill.js = Some(js);
                        app_state.autofill.status_msg =
                            format!("Auto-filled credentials for {domain}");
                    }
                }
            } else {
                app_state.autofill.available = false;
                app_state.autofill.js = None;
            }
        }
        Some("get-newtab-data") => {
            let bookmarks: Vec<serde_json::Value> = if let Some(db) = app_state.db.as_ref() {
                crate::db::bookmarks::all_bookmarks(db)
                    .unwrap_or_default()
                    .into_iter()
                    .take(8)
                    .map(|b| {
                        serde_json::json!({
                            "url": b.url,
                            "title": b.title,
                            "folder": b.folder,
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let history: Vec<serde_json::Value> = if let Some(db) = app_state.db.as_ref() {
                crate::db::history::recent_entries(db, 8)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|h| {
                        serde_json::json!({
                            "url": h.url,
                            "title": h.title,
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let data = serde_json::json!({ "bookmarks": bookmarks, "history": history });
            let js = format!(
                "window._aileron_newtab_data = {data}; if (window._onNewTabData) window._onNewTabData(window._aileron_newtab_data);"
            );
            if let Some(pane) = panes.get_mut(&pane_id) {
                pane.execute_js_code(&js);
                pane.mark_dirty();
            }
        }
        Some("ext-send-message") => {
            let source_id = msg
                .get("sourceId")
                .and_then(|v| v.as_str())
                .map(|s| ExtensionId(s.to_string()));
            let target_id = msg
                .get("targetId")
                .and_then(|v| v.as_str())
                .map(|s| ExtensionId(s.to_string()));
            let message = msg
                .get("message")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let req_id = msg
                .get("reqId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let mgr = app_state.extension_manager.read();
            let bus: &Arc<MessageBus> = mgr.message_bus();
            let response = bus.send_message(source_id.as_ref(), target_id.as_ref(), message);
            let response_json = serde_json::to_string(&response).unwrap_or_else(|_| "null".into());
            if let Some(pane) = panes.get_mut(&pane_id) {
                pane.execute_js_code(&format!(
                    "if (window.__aileron_ext_response) \
                     window.__aileron_ext_response({}, {});",
                    serde_json::to_string(&req_id).unwrap_or_else(|_| "\"\"".into()),
                    response_json
                ));
                pane.mark_dirty();
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_ipc_message(
    app_state: &mut AppState,
    wry_panes: &mut WryPaneManager,
    pane_id: Uuid,
    message: &str,
) {
    handle_ipc_message_generic(app_state, wry_panes, pane_id, message);
}

pub(crate) fn handle_ipc_message_offscreen(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
    pane_id: Uuid,
    message: &str,
) {
    handle_ipc_message_generic(app_state, offscreen_panes, pane_id, message);
}

pub(crate) trait ExecuteJs {
    fn execute_js_code(&self, js: &str);
}

impl ExecuteJs for crate::servo::WryPane {
    fn execute_js_code(&self, js: &str) {
        self.execute_js(js);
    }
}

impl ExecuteJs for crate::offscreen_webview::OffscreenWebView {
    fn execute_js_code(&self, js: &str) {
        self.execute_js(js);
    }
}

pub(crate) fn inject_extension_shim_and_script<T>(
    ext_script: &crate::extensions::scripting::ExtensionContentScriptEntry,
    pane: &T,
    pane_id: Uuid,
    app_state: &mut AppState,
    use_idle_delay: bool,
) where
    T: ExecuteJs,
{
    let ext_id = ExtensionId(ext_script.extension_id.clone());
    let is_loaded = app_state.extension_manager.read().get(&ext_id).is_some();
    if !is_loaded {
        warn!(
            "Extension '{}' is not loaded, skipping content script '{}'",
            ext_script.extension_id, ext_script.script_id
        );
        return;
    }

    if !app_state.is_script_injected(pane_id, &format!("shim:{}", ext_script.extension_id)) {
        pane.execute_js_code(EXTENSION_RUNTIME_SHIM_JS);
        pane.execute_js_code(&format!(
            "window.__aileron_extension_id = {};",
            serde_json::to_string(&ext_script.extension_id).unwrap_or_default()
        ));
        app_state.mark_script_injected(pane_id, &format!("shim:{}", ext_script.extension_id));
    }

    if !ext_script.css_code.is_empty() {
        let escaped = ext_script
            .css_code
            .replace('\\', "\\\\")
            .replace('`', "\\`")
            .replace('$', "\\$");
        let css_js = if use_idle_delay {
            format!(
                "setTimeout(function() {{ \
                    var s = document.createElement('style'); \
                    s.textContent = `{escaped}`; \
                    (document.head || document.documentElement).appendChild(s); \
                }}, 0);"
            )
        } else {
            format!(
                "var s = document.createElement('style'); \
                 s.textContent = `{escaped}`; \
                 (document.head || document.documentElement).appendChild(s);"
            )
        };
        pane.execute_js_code(&css_js);
    }
    if !ext_script.js_code.is_empty() {
        info!(
            "Injecting extension content script '{}' ({}) into pane {}",
            ext_script.script_id,
            ext_script.extension_id,
            &pane_id.to_string()[..8],
        );
        pane.execute_js_code(&ext_script.js_code);
    }
}
