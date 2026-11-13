//! Offscreen event + IPC chain (S4 stage 2d-3: moved from frame_tasks).
//!
//! Everything takes `&mut CoreState` (core) and pane managers (engine)
//! — no AppState. The headed wry wrapper (`handle_ipc_message`) stays
//! in the main crate and calls `handle_ipc_message_generic` here.

use std::sync::Arc;
use uuid::Uuid;

use crate::offscreen_webview::OffscreenWebViewManager;
use crate::servo::wry_engine::{WryEvent, WryPane};
use aileron_core::extensions::web_request::WebRequestInterceptorRegistry;
use aileron_core::extensions::{ExtensionId, message_bus::MessageBus};
use aileron_core::scripts::{ContentScriptManager, RunAt};
use serde_json;
use tracing::{info, warn};
use urlencoding;

pub const EXTENSION_RUNTIME_SHIM_JS: &str = r#"
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

/// Trait abstracting over pane types for event processing.
///
/// Both `WryPane` and `OffscreenWebView` implement this trait.
/// The key difference: `mark_dirty()` is a no-op for WryPane (Wry re-renders
/// automatically) but real for OffscreenWebView (must signal frame invalidation).
pub trait EventPane: ExecuteJs {
    fn navigate(&mut self, url: &url::Url);
    fn url(&self) -> &url::Url;
    fn title(&self) -> &str;
    /// Cache a `TitleChanged` payload so later `LoadComplete` syncs carry the
    /// real title. Default no-op (wry updates its own title).
    fn set_title(&mut self, _title: &str) {}
    fn mark_dirty(&mut self);
    fn suppress_context_menu(&self);
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

impl EventPane for WryPane {
    fn navigate(&mut self, url: &url::Url) {
        WryPane::navigate(self, url);
    }
    fn url(&self) -> &url::Url {
        WryPane::url(self)
    }
    fn title(&self) -> &str {
        WryPane::title(self)
    }
    fn mark_dirty(&mut self) {
        WryPane::mark_dirty(self);
    }
    fn suppress_context_menu(&self) {
        WryPane::suppress_context_menu(self);
    }
}

impl EventPane for crate::offscreen_webview::OffscreenWebView {
    fn navigate(&mut self, url: &url::Url) {
        crate::offscreen_webview::OffscreenWebView::navigate(self, url);
    }
    fn url(&self) -> &url::Url {
        crate::offscreen_webview::OffscreenWebView::url(self)
    }
    fn title(&self) -> &str {
        crate::offscreen_webview::OffscreenWebView::title(self)
    }
    fn set_title(&mut self, title: &str) {
        crate::offscreen_webview::OffscreenWebView::set_title(self, title);
    }
    fn mark_dirty(&mut self) {
        crate::offscreen_webview::OffscreenWebView::mark_dirty(self);
    }
    fn suppress_context_menu(&self) {
        crate::offscreen_webview::OffscreenWebView::suppress_context_menu(self);
    }
}

/// Trait abstracting over pane manager types for event processing.
pub trait EventPaneManager {
    type Pane: EventPane;

    fn get_mut(&mut self, pane_id: &Uuid) -> Option<&mut Self::Pane>;
    fn get(&self, pane_id: &Uuid) -> Option<&Self::Pane>;
}

impl EventPaneManager for crate::servo::wry_engine::WryPaneManager {
    type Pane = WryPane;

    fn get_mut(&mut self, pane_id: &Uuid) -> Option<&mut WryPane> {
        crate::servo::wry_engine::WryPaneManager::get_mut(self, pane_id)
    }

    fn get(&self, pane_id: &Uuid) -> Option<&WryPane> {
        crate::servo::wry_engine::WryPaneManager::get(self, pane_id)
    }
}

impl EventPaneManager for crate::offscreen_webview::OffscreenWebViewManager {
    type Pane = crate::offscreen_webview::OffscreenWebView;

    fn get_mut(
        &mut self,
        pane_id: &Uuid,
    ) -> Option<&mut crate::offscreen_webview::OffscreenWebView> {
        crate::offscreen_webview::OffscreenWebViewManager::get_mut(self, pane_id)
    }

    fn get(&self, pane_id: &Uuid) -> Option<&crate::offscreen_webview::OffscreenWebView> {
        crate::offscreen_webview::OffscreenWebViewManager::get(self, pane_id)
    }
}

/// Extract pane_id from a WryEvent.
fn pane_id_from_event(event: &WryEvent) -> Uuid {
    match event {
        WryEvent::LoadStarted { pane_id, .. }
        | WryEvent::LoadComplete { pane_id, .. }
        | WryEvent::TitleChanged { pane_id, .. }
        | WryEvent::DownloadStarted { pane_id, .. }
        | WryEvent::HttpsUpgraded { pane_id, .. }
        | WryEvent::IpcMessage { pane_id, .. } => *pane_id,
        WryEvent::OpenFile { .. } => Uuid::nil(), // No pane associated
    }
}

/// Unified event processing for both Wry and Offscreen pane managers.
///
/// This replaces both `process_wry_events_inner` and `process_offscreen_events_inner`.
/// The `mark_dirty()` calls are uniform -- they're no-ops for WryPane and real
/// for OffscreenWebView, handled by the `EventPane` trait implementation.
///
/// Returns a list of IPC messages that need type-specific handling by the caller.
pub fn process_events<M: EventPaneManager>(
    core: &mut aileron_core::state::CoreState,
    manager: &mut M,
    content_scripts: &ContentScriptManager,
    adblocker: &aileron_core::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
    events: Vec<WryEvent>,
) -> Vec<(Uuid, String)> {
    let mut ipc_messages = Vec::new();
    for event in events {
        let pane_id = pane_id_from_event(&event);

        match event {
            WryEvent::LoadComplete { url, .. } => {
                core.session.session_dirty = true;
                core.cache.pane_count_dirty = true;
                if let Ok(parsed) = url::Url::parse(&url) {
                    // Visit recording (private panes excluded — core.private_pane_ids)
                    if let Some(ref conn) = core.db
                        && !core.private_pane_ids.contains(&core.wm.active_pane_id())
                        && let Err(e) = aileron_core::db::history::record_visit(conn, &parsed, &url)
                    {
                        tracing::warn!("Failed to record visit: {}", e);
                    }
                }

                // Sync tab URL/title
                let title = manager
                    .get(&pane_id)
                    .map(|p| p.title().to_string())
                    .unwrap_or_default();
                super::offscreen_events::sync_tab_url_title(core, pane_id, &url, &title);

                let a11y = format!("Loaded: {}", &url[..url.len().min(60)]);
                core.ui.status_message = a11y.clone();
                core.ui.accessibility_text = a11y;

                // Fire extension onCompleted lifecycle event
                if interceptor_registry.has_interceptors()
                    && let Ok(parsed_url) = url::Url::parse(&url)
                {
                    let details = aileron_core::extensions::web_request::CompletedDetails {
                        request_id: aileron_core::extensions::types::RequestId(0),
                        url: parsed_url,
                        frame_id: aileron_core::extensions::types::FrameId(0),
                        tab_id: None,
                        type_: aileron_core::extensions::web_request::ResourceType::MainFrame,
                        from_cache: false,
                        status_code: 200,
                        ip: None,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs_f64() * 1000.0)
                            .unwrap_or(0.0),
                    };
                    interceptor_registry.fire_on_completed(&details);
                }

                if let Some(pane) = manager.get_mut(&pane_id) {
                    pane.mark_dirty();
                }

                if !url.starts_with("aileron://") {
                    if let Some(ref conn) = core.db
                        && let Ok(db_marks) =
                            aileron_core::db::scroll_marks::load_scroll_marks_for_url(conn, &url)
                        && !db_marks.is_empty()
                    {
                        core.session
                            .marks
                            .entry(pane_id)
                            .or_default()
                            .extend(db_marks);
                    }

                    // Inject DocumentEnd scripts
                    let end_matching = content_scripts.scripts_for_url(&url, RunAt::DocumentEnd);
                    for script in end_matching {
                        let key = format!("userscript:{}", script.name);
                        if core.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            info!(
                                "Injecting document-end content script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            pane.execute_js_code(&script.js_code);
                            pane.mark_dirty();
                            core.mark_script_injected(pane_id, &key);
                        }
                    }
                    let ext_end_scripts =
                        content_scripts.extension_scripts_for_url(&url, RunAt::DocumentEnd);
                    for ext_script in ext_end_scripts {
                        let key = format!("{}:{}", ext_script.extension_id, ext_script.script_id);
                        if core.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            inject_extension_shim_and_script(
                                &ext_script,
                                pane,
                                pane_id,
                                core,
                                false,
                            );
                            pane.mark_dirty();
                            core.mark_script_injected(pane_id, &key);
                        }
                    }

                    // Inject DocumentIdle scripts
                    let matching = content_scripts.scripts_for_url(&url, RunAt::DocumentIdle);
                    for script in matching {
                        let key = format!("userscript:{}", script.name);
                        if core.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            info!(
                                "Injecting content script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            pane.execute_js_code(&script.js_code);
                            pane.mark_dirty();
                            core.mark_script_injected(pane_id, &key);
                        }
                    }
                    let ext_scripts =
                        content_scripts.extension_scripts_for_url(&url, RunAt::DocumentIdle);
                    for ext_script in ext_scripts {
                        let key = format!("{}:{}", ext_script.extension_id, ext_script.script_id);
                        if core.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            inject_extension_shim_and_script(
                                &ext_script,
                                pane,
                                pane_id,
                                core,
                                true,
                            );
                            pane.mark_dirty();
                            core.mark_script_injected(pane_id, &key);
                        }
                    }
                    if let Some(pane) = manager.get_mut(&pane_id) {
                        pane.execute_js_code(crate::servo::NETWORK_MONITOR_JS);
                        pane.execute_js_code(crate::servo::CONSOLE_CAPTURE_JS);
                        #[cfg(feature = "passwords")]
                        pane.execute_js_code(
                            aileron_core::passwords::bitwarden::BitwardenClient::form_submit_observer_js(),
                        );
                        pane.suppress_context_menu();
                        pane.execute_js_code(
                            "setTimeout(function() { \
                                if (window._aileron_scroll_pos) { \
                                    window.scrollTo(window._aileron_scroll_pos.x, window._aileron_scroll_pos.y); \
                                } \
                            }, 100);"
                        );
                        #[cfg(feature = "passwords")]
                        pane.execute_js_code(&format!(
                            "setTimeout(function() {{ {} }}, 500);",
                            aileron_core::passwords::bitwarden::BitwardenClient::form_detect_report_js()
                        ));
                        pane.execute_js_code(
                            "(function(){ \
                                var el = document.documentElement; \
                                var cs = getComputedStyle(el); \
                                if (cs && cs.scrollBehavior !== 'smooth') { \
                                    el.style.scrollBehavior = 'smooth'; \
                                } \
                            })();",
                        );
                        pane.mark_dirty();
                    }

                    if let Some(ref css) = core.config.custom_css
                        && !css.trim().is_empty()
                        && let Some(pane) = manager.get_mut(&pane_id)
                    {
                        let escaped = css
                            .replace('\\', "\\\\")
                            .replace('`', "\\`")
                            .replace('$', "\\$");
                        pane.execute_js_code(&format!(
                            "setTimeout(function() {{ \
                                var s = document.createElement('style'); \
                                s.textContent = `{escaped}`; \
                                (document.head || document.documentElement).appendChild(s); \
                            }}, 0);"
                        ));
                        pane.mark_dirty();
                    }

                    let csp_headers = adblocker.get_csp_headers(&url);
                    if !csp_headers.is_empty() {
                        let csp = csp_headers.join("; ");
                        let escaped = csp.replace('\\', "\\\\").replace('\'', "\\'");
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            pane.execute_js_code(&format!(
                                "var meta = document.createElement('meta'); meta['http-equiv'] = 'Content-Security-Policy'; meta.content = '{escaped}'; document.head.appendChild(meta);"
                            ));
                        }
                    }

                    // Apply per-site zoom if configured
                    if let Some(ref db) = core.db
                        && let Ok(settings) =
                            aileron_core::db::site_settings::get_site_settings_for_url(db, &url)
                        && let Some(zoom) = settings.iter().find_map(|s| s.zoom_level)
                        && let Some(pane) = manager.get_mut(&pane_id)
                    {
                        pane.execute_js_code(&format!(
                            "if(document.body) document.body.style.zoom = '{zoom:.2}';"
                        ));
                    }
                }
            }
            WryEvent::LoadStarted { url, .. } => {
                core.autofill.available = false;
                core.autofill.username_id.clear();
                core.autofill.password_id.clear();
                core.autofill.js = None;
                core.autofill.status_msg.clear();
                core.clear_injected_scripts(pane_id);
                {
                    let __m = &format!("Loading: {}...", &url[..url.len().min(40)]);
                    core.ui.status_message = __m.to_string();
                    core.ui.accessibility_text = __m.to_string();
                }

                // Fire extension onBeforeRequest lifecycle event
                if interceptor_registry.has_interceptors()
                    && let Ok(parsed_url) = url::Url::parse(&url)
                {
                    let details = aileron_core::extensions::web_request::RequestDetails {
                        request_id: aileron_core::extensions::types::RequestId(0),
                        url: parsed_url,
                        method: "GET".into(),
                        frame_id: aileron_core::extensions::types::FrameId(0),
                        parent_frame_id: aileron_core::extensions::types::FrameId(u32::MAX),
                        tab_id: None,
                        type_: aileron_core::extensions::web_request::ResourceType::MainFrame,
                        origin_url: None,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs_f64() * 1000.0)
                            .unwrap_or(0.0),
                        request_headers: None,
                    };
                    let response = interceptor_registry.fire_on_before_request(&details);
                    if response.cancel == Some(true) {
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            pane.navigate(&url::Url::parse("about:blank").unwrap());
                            pane.mark_dirty();
                        }
                        continue;
                    }
                    if let Some(redirect_url) = response.redirect_url {
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            pane.navigate(&redirect_url);
                            pane.mark_dirty();
                        }
                        continue;
                    }
                }

                if !url.starts_with("aileron://") {
                    let start_scripts = content_scripts.scripts_for_url(&url, RunAt::DocumentStart);
                    for script in start_scripts {
                        let key = format!("userscript:{}", script.name);
                        if core.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            info!(
                                "Injecting document-start script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            pane.execute_js_code(&script.js_code);
                            pane.mark_dirty();
                            core.mark_script_injected(pane_id, &key);
                        }
                    }
                    let ext_scripts =
                        content_scripts.extension_scripts_for_url(&url, RunAt::DocumentStart);
                    for ext_script in ext_scripts {
                        let key = format!("{}:{}", ext_script.extension_id, ext_script.script_id);
                        if core.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            inject_extension_shim_and_script(
                                &ext_script,
                                pane,
                                pane_id,
                                core,
                                false,
                            );
                            pane.mark_dirty();
                            core.mark_script_injected(pane_id, &key);
                        }
                    }
                }
            }
            WryEvent::TitleChanged { title, .. } => {
                {
                    let __m = &title[..title.len().min(60)];
                    core.ui.status_message = __m.to_string();
                    core.ui.accessibility_text = __m.to_string();
                }
                // Cache the title on the pane so the LoadComplete sync (which
                // reads pane.title()) carries the real title instead of "".
                if let Some(pane) = manager.get_mut(&pane_id) {
                    pane.set_title(&title);
                }
                let url = manager
                    .get(&pane_id)
                    .map(|p| p.url().to_string())
                    .unwrap_or_default();
                super::offscreen_events::sync_tab_url_title(core, pane_id, &url, &title);
            }
            WryEvent::DownloadStarted { url, filename, .. } => {
                let dl_id = core
                    .download_manager
                    .start(url.as_str(), Some(filename.as_str()));
                let short_url = if url.len() > 40 { &url[..37] } else { &url };
                core.ui.status_message = format!("Download #{dl_id}: {filename} ({short_url})");
                info!("Download #{} started: {} from {}", dl_id, filename, url);
                if let Some(db) = core.db.as_ref() {
                    let dest = core
                        .download_manager
                        .downloads_dir()
                        .join(filename.as_str());
                    if let Err(e) = aileron_core::db::downloads::record_download(
                        db,
                        url.as_str(),
                        filename.as_str(),
                        &dest.to_string_lossy(),
                    ) {
                        warn!("Failed to record download: {}", e);
                    }
                }
            }
            WryEvent::OpenFile { path } => {
                let _ = open::that(&path);
                core.ui.status_message = format!("Opened: {path}");
            }
            WryEvent::HttpsUpgraded { to, .. } => {
                core.ui.status_message = format!("HTTPS upgrade: {to}");
                if let Some(pane) = manager.get_mut(&pane_id) {
                    pane.mark_dirty();
                }
            }
            WryEvent::IpcMessage { message, .. } => {
                ipc_messages.push((pane_id, message));
            }
        }
    }
    ipc_messages
}

pub fn handle_ipc_message_generic<M: EventPaneManager>(
    core: &mut aileron_core::state::CoreState,
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
        let a11y_msg = format!(
            "Load failed: {}",
            &error_detail[..error_detail.len().min(60)]
        );
        core.ui.status_message = a11y_msg.clone();
        core.ui.accessibility_text = a11y_msg;
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
            let config_json = if core.cache.config_json_dirty {
                core.cache.config_json_cache =
                    serde_json::to_string(&core.config).unwrap_or_default();
                core.cache.config_json_dirty = false;
                core.cache.config_json_cache.clone()
            } else {
                core.cache.config_json_cache.clone()
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
                    core.config.homepage = v.to_string();
                }
                if let Some(v) = config_obj.get("search_engine").and_then(|v| v.as_str()) {
                    core.config.search_engine = v.to_string();
                }
                if let Some(v) = config_obj.get("restore_session").and_then(|v| v.as_bool()) {
                    core.config.restore_session = v;
                }
                if let Some(v) = config_obj.get("tab_layout").and_then(|v| v.as_str()) {
                    core.config.tab_layout = v.to_string();
                }
                if let Some(v) = config_obj.get("tab_sidebar_width").and_then(|v| v.as_f64()) {
                    core.config.tab_sidebar_width = v as f32;
                }
                if let Some(v) = config_obj
                    .get("tab_sidebar_right")
                    .and_then(|v| v.as_bool())
                {
                    core.config.tab_sidebar_right = v;
                }
                if let Some(v) = config_obj.get("adblock_enabled").and_then(|v| v.as_bool()) {
                    core.config.adblock_enabled = v;
                }
                if let Some(v) = config_obj
                    .get("https_upgrade_enabled")
                    .and_then(|v| v.as_bool())
                {
                    core.config.https_upgrade_enabled = v;
                }
                if let Some(v) = config_obj
                    .get("tracking_protection_enabled")
                    .and_then(|v| v.as_bool())
                {
                    core.config.tracking_protection_enabled = v;
                }
                if let Some(v) = config_obj.get("devtools").and_then(|v| v.as_bool()) {
                    core.config.devtools = v;
                }
                if let Some(v) = config_obj.get("proxy") {
                    core.config.proxy = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("custom_css") {
                    core.config.custom_css =
                        v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("engine_selection").and_then(|v| v.as_str()) {
                    core.config.engine_selection = v.to_string();
                }
                if let Some(v) = config_obj.get("language") {
                    core.config.language =
                        v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("adaptive_quality").and_then(|v| v.as_bool()) {
                    core.config.adaptive_quality = v;
                }
                if let Some(v) = config_obj
                    .get("popup_blocker_enabled")
                    .and_then(|v| v.as_bool())
                {
                    core.config.popup_blocker_enabled = v;
                }
                if let Some(v) = config_obj
                    .get("adblock_update_interval_hours")
                    .and_then(|v| v.as_u64())
                {
                    core.config.adblock_update_interval_hours = v;
                }
                if let Some(v) = config_obj.get("theme").and_then(|v| v.as_str()) {
                    core.config.theme = v.to_string();
                }
                if let Some(v) = config_obj
                    .get("adblock_cosmetic_filtering")
                    .and_then(|v| v.as_bool())
                {
                    core.config.adblock_cosmetic_filtering = v;
                }
                if let Some(v) = config_obj.get("auto_save").and_then(|v| v.as_bool()) {
                    core.config.auto_save = v;
                }
                if let Some(v) = config_obj.get("sync_target").and_then(|v| v.as_str()) {
                    core.config.sync_target = v.to_string();
                }
                if let Some(v) = config_obj.get("sync_encrypted").and_then(|v| v.as_bool()) {
                    core.config.sync_encrypted = v;
                }
                // sync_passphrase is stored in keyring, not config — handled below
                #[cfg(feature = "passwords")]
                if let Some(v) = config_obj
                    .get("sync_passphrase")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    match aileron_core::passwords::keyring::store_credential("aileron-sync", v) {
                        Ok(()) => {
                            core.config.sync_passphrase = v.to_string();
                            info!("Sync passphrase stored in system keyring");
                        }
                        Err(e) => {
                            warn!("Failed to store sync passphrase in keyring: {}", e);
                            core.ui.status_message = format!("Failed to store passphrase: {e}");
                        }
                    }
                }
                if let Some(v) = config_obj.get("sync_auto").and_then(|v| v.as_bool()) {
                    core.config.sync_auto = v;
                }
                if let Some(v) = config_obj
                    .get("sync_auto_interval_sec")
                    .and_then(|v| v.as_u64())
                {
                    core.config.sync_auto_interval_sec = v;
                }
                if let Err(e) = aileron_core::config::Config::save(&core.config) {
                    warn!("Failed to save config: {}", e);
                }
                if let Some(pane) = panes.get_mut(&pane_id) {
                    pane.execute_js_code("window._onConfigSaved && window._onConfigSaved();");
                    pane.mark_dirty();
                }
                core.ui.status_message = "Settings saved".into();
                core.cache.config_json_dirty = true;
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
                match aileron_core::passwords::keyring::store_credential(&key, password) {
                    Ok(()) => {
                        info!("Saved credential for {}", username);
                        core.ui.status_message = format!("Credential saved for {username}");
                    }
                    Err(e) => {
                        warn!("Failed to store credential: {}", e);
                        core.ui.status_message = format!("Credential save failed: {e}");
                    }
                }
            } else {
                core.ui.status_message = "No pending credentials to save".into();
            }
        }
        Some("scroll-fraction") => {
            if let Some(frac) = msg.get("frac").and_then(|v| v.as_f64())
                && let Some(mark_char) = core.session.pending_mark_set.take()
            {
                let frac = frac.clamp(0.0, 1.0);
                core.session
                    .marks
                    .entry(pane_id)
                    .or_default()
                    .insert(mark_char, frac);
                tracing::debug!("Mark '{}' set at fraction {}", mark_char, frac);
                // Persist to database keyed by URL
                if let Some(pane) = panes.get(&pane_id) {
                    let url = pane.url().to_string();
                    if let Some(ref conn) = core.db
                        && let Err(e) = aileron_core::db::scroll_marks::set_scroll_mark(
                            conn, &url, mark_char, frac,
                        )
                    {
                        tracing::warn!("Failed to persist scroll mark: {}", e);
                    }
                }
            }
        }
        Some("hint-clicked") => {
            core.ui.hint_mode = false;
            core.ui.hint_buffer.clear();
            core.ui.status_message.clear();
        }
        #[cfg(feature = "passwords")]
        Some("login-form-detected") => {
            if msg
                .get("has_login")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                && pane_id == core.wm.active_pane_id()
                && core.bitwarden.is_unlocked()
                && let Some(pane) = panes.get_mut(&pane_id)
            {
                let url = pane.url().to_string();
                if let Ok(items) = core.bitwarden.search_for_url(&url)
                    && !items.is_empty()
                {
                    core.autofill.available = true;
                    if let Some(uid) = msg.get("username_id").and_then(|v| v.as_str()) {
                        core.autofill.username_id = uid.to_string();
                    }
                    if let Some(pid) = msg.get("password_id").and_then(|v| v.as_str()) {
                        core.autofill.password_id = pid.to_string();
                    }
                    if let Ok(cred) = core.bitwarden.get_credential(&items[0].id) {
                        let domain = url::Url::parse(&url)
                            .ok()
                            .and_then(|u| u.domain().map(String::from))
                            .unwrap_or_else(|| "unknown".into());
                        let (username_id, password_id) = {
                            let autofill = &core.autofill;
                            (autofill.username_id.clone(), autofill.password_id.clone())
                        };
                        let js =
                            core.bitwarden
                                .autofill_by_id_js(&username_id, &password_id, &cred);
                        core.autofill.js = Some(js);
                        core.autofill.status_msg = format!("Auto-filled credentials for {domain}");
                    }
                }
            } else {
                core.autofill.available = false;
                core.autofill.js = None;
            }
        }
        Some("get-newtab-data") => {
            let bookmarks: Vec<serde_json::Value> = if let Some(db) = core.db.as_ref() {
                aileron_core::db::bookmarks::all_bookmarks(db)
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
            let history: Vec<serde_json::Value> = if let Some(db) = core.db.as_ref() {
                aileron_core::db::history::recent_entries(db, 8)
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

            let mgr = core.extension_manager.read();
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
        Some("reader-save-content") => {
            // Handle reader mode content save from JavaScript
            let title = msg
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled");
            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let word_count = msg.get("word_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let read_time = msg.get("read_time").and_then(|v| v.as_u64()).unwrap_or(0);

            // Save to file if a path was provided via the pending reader-save command
            if let Some(ref path) = core.pending_reader_save_path {
                let markdown = format!(
                    "# {}\n\n{}\n\n---\n*{} words, ~{} min read*\n",
                    title, content, word_count, read_time
                );
                match std::fs::write(path, &markdown) {
                    Ok(()) => {
                        core.ui.status_message = format!("Saved to: {}", path.display());
                    }
                    Err(e) => {
                        core.ui.status_message = format!("Failed to save: {}", e);
                    }
                }
                core.pending_reader_save_path = None;
            } else {
                core.ui.status_message = format!(
                    "Content extracted ({} words, ~{} min read)",
                    word_count, read_time
                );
            }
        }
        _ => {}
    }
}

pub fn handle_ipc_message_offscreen(
    core: &mut aileron_core::state::CoreState,
    offscreen_panes: &mut OffscreenWebViewManager,
    pane_id: Uuid,
    message: &str,
) {
    handle_ipc_message_generic(core, offscreen_panes, pane_id, message);
}

pub trait ExecuteJs {
    fn execute_js_code(&self, js: &str);
}

pub fn inject_extension_shim_and_script<T>(
    ext_script: &aileron_core::extensions::scripting::ExtensionContentScriptEntry,
    pane: &T,
    pane_id: Uuid,
    core: &mut aileron_core::state::CoreState,
    use_idle_delay: bool,
) where
    T: ExecuteJs,
{
    let ext_id = ExtensionId(ext_script.extension_id.clone());
    let is_loaded = core.extension_manager.read().get(&ext_id).is_some();
    if !is_loaded {
        warn!(
            "Extension '{}' is not loaded, skipping content script '{}'",
            ext_script.extension_id, ext_script.script_id
        );
        return;
    }

    if !core.is_script_injected(pane_id, &format!("shim:{}", ext_script.extension_id)) {
        pane.execute_js_code(EXTENSION_RUNTIME_SHIM_JS);
        pane.execute_js_code(&format!(
            "window.__aileron_extension_id = {};",
            serde_json::to_string(&ext_script.extension_id).unwrap_or_default()
        ));
        core.mark_script_injected(pane_id, &format!("shim:{}", ext_script.extension_id));
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
