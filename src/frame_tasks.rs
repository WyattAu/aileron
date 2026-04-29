use open::that as open_that;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use image::ImageEncoder;

use aileron::app::{AppState, WryAction};
use aileron::arp::ArpCommand;
use aileron::extensions::web_request::WebRequestInterceptorRegistry;
use aileron::extensions::{ExtensionId, MessageBus};
use aileron::git::GitStatus;
use aileron::mcp::{McpBridge, McpCommand};
use aileron::offscreen_webview::OffscreenWebViewManager;
use aileron::scripts::{ContentScriptManager, RunAt};
use aileron::servo::{WryEvent, WryPaneManager, pump_gtk};
use aileron::terminal::NativeTerminalManager;

const EXTENSION_RUNTIME_SHIM_JS: &str = r#"
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

pub fn poll_git_status(git_status: &mut GitStatus, git_poller: &Option<aileron::git::GitPoller>) {
    if let Some(poller) = git_poller
        && let Some(new_status) = poller.try_poll()
    {
        *git_status = new_status;
    }
}

pub fn auto_save_workspace(app_state: &mut AppState, wry_panes: &WryPaneManager) {
    // Track pane focus changes for LRU unloading
    app_state.update_pane_focus_tracking();

    if !app_state.config.auto_save {
        return;
    }
    if !app_state.session_dirty {
        return;
    }
    let interval = std::time::Duration::from_secs(app_state.config.auto_save_interval);
    if app_state.last_auto_save.elapsed() < interval {
        return;
    }
    app_state.last_auto_save = std::time::Instant::now();

    let pane_urls: std::collections::HashMap<Uuid, String> = wry_panes
        .pane_ids()
        .into_iter()
        .filter_map(|id| wry_panes.url_for(&id).map(|url| (id, url.to_string())))
        .collect();

    if !pane_urls.is_empty() {
        match app_state.save_workspace_with_urls("_autosave", &pane_urls) {
            Ok(()) => {
                tracing::info!("Auto-saved workspace ({} panes)", pane_urls.len());
                if let Some(ref conn) = app_state.db {
                    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").ok();
                }
            }
            Err(e) => {
                tracing::warn!("Auto-save failed: {}", e);
            }
        }
    }
}

/// Push current tab state to the ARP server (if running).
/// Called every frame but only serializes when the server is active.
pub fn push_tabs_to_arp(app_state: &AppState, wry_panes: &WryPaneManager) {
    let server = match &app_state.arp_server {
        Some(s) if s.is_running() => s,
        _ => return,
    };

    let active_id = app_state.wm.active_pane_id();
    let pane_ids = wry_panes.pane_ids();

    let tabs: Vec<serde_json::Value> = pane_ids
        .iter()
        .filter_map(|id| {
            let url = wry_panes.url_for(id)?;
            let title = wry_panes
                .get(id)
                .map(|p| p.title().to_string())
                .unwrap_or_default();
            Some(serde_json::json!({
                "id": id.to_string(),
                "url": url.as_str(),
                "title": title,
                "active": active_id == *id,
                "muted": app_state.muted_pane_ids.contains(id),
                "pinned": app_state.pinned_pane_ids.contains(id),
            }))
        })
        .collect();

    server.set_tabs(tabs);

    // Push quickmarks state
    let quickmarks: Vec<serde_json::Value> = app_state
        .quickmarks_list()
        .iter()
        .map(|(key, url)| {
            serde_json::json!({
                "key": key,
                "url": url,
            })
        })
        .collect();
    server.set_quickmarks(quickmarks);
}

/// Process pending ARP commands from mobile clients.
/// Dispatches mutations (tab create, navigate, close, etc.) to AppState/WryActions.
pub fn process_arp_commands(app_state: &mut AppState) {
    let receiver = match &app_state.arp_cmd_receiver {
        Some(r) => r,
        None => return,
    };

    let mut guard = match receiver.lock() {
        Ok(g) => g,
        Err(_) => return,
    };

    while let Ok(cmd) = guard.try_recv() {
        match cmd {
            ArpCommand::TabCreate { url } => {
                let active = app_state.wm.active_pane_id();
                match app_state
                    .wm
                    .split(active, aileron::wm::SplitDirection::Vertical, 0.5)
                {
                    Ok(new_id) => {
                        let target_url = url
                            .and_then(|u| url::Url::parse(&u).ok())
                            .unwrap_or_else(|| url::Url::parse("aileron://newtab").unwrap());
                        app_state.engines.create_pane(new_id, target_url, None);
                        app_state.session_dirty = true;
                    }
                    Err(e) => {
                        warn!(target: "arp", "Tab create failed: {}", e);
                    }
                }
            }
            ArpCommand::TabNavigate { tab_id: _, url } => match url::Url::parse(&url) {
                Ok(parsed) => {
                    app_state
                        .pending_wry_actions
                        .push_back(WryAction::Navigate(parsed));
                    app_state.session_dirty = true;
                }
                Err(e) => {
                    warn!(target: "arp", "Tab navigate invalid URL: {}", e);
                }
            },
            ArpCommand::TabClose { tab_id } => {
                let target = tab_id.unwrap_or_else(|| app_state.wm.active_pane_id());
                match app_state.wm.close(target) {
                    Ok(_next) => {
                        app_state.session_dirty = true;
                    }
                    Err(e) => {
                        warn!(target: "arp", "Tab close failed: {}", e);
                    }
                }
            }
            ArpCommand::TabActivate { tab_id } => {
                app_state.wm.set_active_pane(tab_id);
            }
            ArpCommand::TabGoBack { tab_id: _ } => {
                app_state.pending_wry_actions.push_back(WryAction::Back);
            }
            ArpCommand::TabGoForward { tab_id: _ } => {
                app_state.pending_wry_actions.push_back(WryAction::Forward);
            }
            ArpCommand::TabReload { tab_id: _ } => {
                app_state.pending_wry_actions.push_back(WryAction::Reload);
            }
            ArpCommand::ClipboardSet { text } => {
                app_state
                    .pending_wry_actions
                    .push_back(WryAction::SetClipboard(text));
            }
            ArpCommand::ClipboardGet { request_id } => {
                let contents = aileron::platform::platform()
                    .clipboard_paste()
                    .unwrap_or_default();
                if let Some(server) = &app_state.arp_server {
                    server.notify(
                        "clipboard.contents",
                        serde_json::json!({
                            "request_id": request_id,
                            "text": contents,
                        }),
                    );
                }
            }
            ArpCommand::QuickmarkOpen { key } => {
                if let Some(url) = app_state.quickmarks_get(&key) {
                    app_state
                        .pending_wry_actions
                        .push_back(WryAction::Navigate(url));
                }
            }
        }
    }
}

pub fn process_wry_events(
    app_state: &mut AppState,
    wry_panes: &mut WryPaneManager,
    content_scripts: &ContentScriptManager,
    mcp_bridge: &mut McpBridge,
    adblocker: &aileron::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
) {
    let wry_events = wry_panes.poll_all_events();
    for event in wry_events {
        match event {
            WryEvent::LoadComplete { pane_id, url, .. } => {
                app_state.session_dirty = true;
                app_state.tab_display_dirty = true;
                app_state.pane_count_dirty = true;
                if let Ok(parsed) = url::Url::parse(&url) {
                    app_state.record_visit(&parsed, &url);
                }
                app_state.update_a11y(&format!("Loaded: {}", &url[..url.len().min(60)]));

                // Fire extension onCompleted lifecycle event
                if interceptor_registry.has_interceptors()
                    && let Ok(parsed_url) = url::Url::parse(&url)
                {
                    let details = aileron::extensions::web_request::CompletedDetails {
                        request_id: aileron::extensions::types::RequestId(0),
                        url: parsed_url,
                        frame_id: aileron::extensions::types::FrameId(0),
                        tab_id: None,
                        type_: aileron::extensions::web_request::ResourceType::MainFrame,
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

                if !url.starts_with("aileron://") {
                    // Load persisted scroll marks for this URL
                    app_state.load_scroll_marks_for_pane(pane_id, &url);

                    // Inject DocumentEnd scripts
                    let end_matching = content_scripts.scripts_for_url(&url, RunAt::DocumentEnd);
                    for script in end_matching {
                        let key = format!("userscript:{}", script.name);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            info!(
                                "Injecting document-end content script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            wry_pane.execute_js(&script.js_code);
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }
                    let ext_end_scripts =
                        content_scripts.extension_scripts_for_url(&url, RunAt::DocumentEnd);
                    for ext_script in ext_end_scripts {
                        let key = format!("{}:{}", ext_script.extension_id, ext_script.script_id);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            inject_extension_shim_and_script(
                                &ext_script,
                                wry_pane,
                                pane_id,
                                app_state,
                                false,
                            );
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }

                    // Inject DocumentIdle scripts
                    let matching = content_scripts.scripts_for_url(&url, RunAt::DocumentIdle);
                    for script in matching {
                        let key = format!("userscript:{}", script.name);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            info!(
                                "Injecting content script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            wry_pane.execute_js(&script.js_code);
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }
                    let ext_scripts =
                        content_scripts.extension_scripts_for_url(&url, RunAt::DocumentIdle);
                    for ext_script in ext_scripts {
                        let key = format!("{}:{}", ext_script.extension_id, ext_script.script_id);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            inject_extension_shim_and_script(
                                &ext_script,
                                wry_pane,
                                pane_id,
                                app_state,
                                true,
                            );
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }
                    if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                        wry_pane.execute_js(aileron::servo::NETWORK_MONITOR_JS);
                        wry_pane.execute_js(aileron::servo::CONSOLE_CAPTURE_JS);
                        wry_pane.execute_js(
                            aileron::passwords::bitwarden::BitwardenClient::form_submit_observer_js(
                            ),
                        );
                        wry_pane.execute_js(
                            "setTimeout(function() { \
                                if (window._aileron_scroll_pos) { \
                                    window.scrollTo(window._aileron_scroll_pos.x, window._aileron_scroll_pos.y); \
                                } \
                            }, 100);"
                        );
                        wry_pane.execute_js(&format!(
                            "setTimeout(function() {{ {} }}, 500);",
                            aileron::passwords::bitwarden::BitwardenClient::form_detect_report_js()
                        ));
                        wry_pane.execute_js(
                            "(function(){ \
                                var el = document.documentElement; \
                                var cs = getComputedStyle(el); \
                                if (cs && cs.scrollBehavior !== 'smooth') { \
                                    el.style.scrollBehavior = 'smooth'; \
                                } \
                            })();",
                        );
                    }

                    if let Some(ref css) = app_state.config.custom_css
                        && !css.trim().is_empty()
                        && let Some(wry_pane) = wry_panes.get_mut(&pane_id)
                    {
                        let escaped = css
                            .replace('\\', "\\\\")
                            .replace('`', "\\`")
                            .replace('$', "\\$");
                        wry_pane.execute_js(&format!(
                            "setTimeout(function() {{ \
                                var s = document.createElement('style'); \
                                s.textContent = `{}`; \
                                (document.head || document.documentElement).appendChild(s); \
                            }}, 0);",
                            escaped
                        ));
                    }

                    let csp_headers = adblocker.get_csp_headers(&url);
                    if !csp_headers.is_empty() {
                        let csp = csp_headers.join("; ");
                        let escaped = csp.replace('\\', "\\\\").replace('\'', "\\'");
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            wry_pane.execute_js(&format!(
                                "var meta = document.createElement('meta'); meta['http-equiv'] = 'Content-Security-Policy'; meta.content = '{}'; document.head.appendChild(meta);",
                                escaped
                            ));
                        }
                    }

                    // Apply per-site zoom if configured
                    if let Some(ref db) = app_state.db
                        && let Ok(settings) =
                            aileron::db::site_settings::get_site_settings_for_url(db, &url)
                        && let Some(zoom) = settings.iter().find_map(|s| s.zoom_level)
                        && let Some(wry_pane) = wry_panes.get_mut(&pane_id)
                    {
                        wry_pane.execute_js(&format!(
                            "if(document.body) document.body.style.zoom = '{:.2}';",
                            zoom
                        ));
                    }
                }
            }
            WryEvent::LoadStarted { url, pane_id, .. } => {
                app_state.autofill_available = false;
                app_state.autofill_username_id.clear();
                app_state.autofill_password_id.clear();
                app_state.autofill_js = None;
                app_state.autofill_status_msg.clear();
                app_state.clear_injected_scripts(pane_id);
                app_state.update_a11y(&format!("Loading: {}...", &url[..url.len().min(40)]));
                if !url.starts_with("aileron://") {
                    let start_scripts = content_scripts.scripts_for_url(&url, RunAt::DocumentStart);
                    for script in start_scripts {
                        let key = format!("userscript:{}", script.name);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            info!(
                                "Injecting document-start script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            wry_pane.execute_js(&script.js_code);
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }
                    let ext_scripts =
                        content_scripts.extension_scripts_for_url(&url, RunAt::DocumentStart);
                    for ext_script in ext_scripts {
                        let key = format!("{}:{}", ext_script.extension_id, ext_script.script_id);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            inject_extension_shim_and_script(
                                &ext_script,
                                wry_pane,
                                pane_id,
                                app_state,
                                false,
                            );
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }
                }
            }
            WryEvent::TitleChanged { title, .. } => {
                app_state.update_a11y(&title[..title.len().min(60)]);
                app_state.tab_display_dirty = true;
            }
            WryEvent::DownloadStarted { url, filename, .. } => {
                // Use the download manager for actual downloading with progress
                let dl_id = app_state
                    .download_manager
                    .start(url.as_str(), Some(filename.as_str()));
                let short_url = if url.len() > 40 { &url[..37] } else { &url };
                app_state.status_message =
                    format!("Download #{}: {} ({})", dl_id, filename, short_url);
                info!("Download #{} started: {} from {}", dl_id, filename, url);
                // Record in database for history
                if let Some(db) = app_state.db.as_ref() {
                    let dest = app_state
                        .download_manager
                        .downloads_dir()
                        .join(filename.as_str());
                    if let Err(e) = aileron::db::downloads::record_download(
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
                let _ = open_that(&path);
                app_state.status_message = format!("Opened: {}", path);
            }
            WryEvent::HttpsUpgraded { to, .. } => {
                app_state.status_message = format!("HTTPS upgrade: {}", to);
            }
            WryEvent::IpcMessage { pane_id, message } => {
                handle_ipc_message(app_state, wry_panes, pane_id, &message);
            }
        }
    }

    let active_id = app_state.wm.active_pane_id();
    if let Some(wry_pane) = wry_panes.get(&active_id) {
        mcp_bridge.update_state(wry_pane.url().as_str(), wry_pane.title());
    }
}

pub fn process_offscreen_events(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
    content_scripts: &ContentScriptManager,
    _mcp_bridge: &mut McpBridge,
    adblocker: &aileron::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
) {
    let events = offscreen_panes.drain_all_events();
    for (_pane_id, event) in events {
        match event {
            WryEvent::LoadComplete { pane_id, url, .. } => {
                app_state.session_dirty = true;
                app_state.tab_display_dirty = true;
                app_state.pane_count_dirty = true;
                if let Ok(parsed) = url::Url::parse(&url) {
                    app_state.record_visit(&parsed, &url);
                }
                app_state.update_a11y(&format!("Loaded: {}", &url[..url.len().min(60)]));

                // Fire extension onCompleted lifecycle event
                if interceptor_registry.has_interceptors()
                    && let Ok(parsed_url) = url::Url::parse(&url)
                {
                    let details = aileron::extensions::web_request::CompletedDetails {
                        request_id: aileron::extensions::types::RequestId(0),
                        url: parsed_url,
                        frame_id: aileron::extensions::types::FrameId(0),
                        tab_id: None,
                        type_: aileron::extensions::web_request::ResourceType::MainFrame,
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

                if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                    pane.mark_dirty();
                }

                if !url.starts_with("aileron://") {
                    // Load persisted scroll marks for this URL
                    app_state.load_scroll_marks_for_pane(pane_id, &url);

                    // Inject DocumentEnd scripts
                    let end_matching = content_scripts.scripts_for_url(&url, RunAt::DocumentEnd);
                    for script in end_matching {
                        let key = format!("userscript:{}", script.name);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            info!(
                                "Injecting document-end content script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            pane.execute_js(&script.js_code);
                            pane.mark_dirty();
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }
                    let ext_end_scripts =
                        content_scripts.extension_scripts_for_url(&url, RunAt::DocumentEnd);
                    for ext_script in ext_end_scripts {
                        let key = format!("{}:{}", ext_script.extension_id, ext_script.script_id);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            inject_extension_shim_and_script(
                                &ext_script,
                                pane,
                                pane_id,
                                app_state,
                                false,
                            );
                            pane.mark_dirty();
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }

                    // Inject DocumentIdle scripts
                    let matching = content_scripts.scripts_for_url(&url, RunAt::DocumentIdle);
                    for script in matching {
                        let key = format!("userscript:{}", script.name);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            info!(
                                "Injecting content script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            pane.execute_js(&script.js_code);
                            pane.mark_dirty();
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }
                    let ext_scripts =
                        content_scripts.extension_scripts_for_url(&url, RunAt::DocumentIdle);
                    for ext_script in ext_scripts {
                        let key = format!("{}:{}", ext_script.extension_id, ext_script.script_id);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            inject_extension_shim_and_script(
                                &ext_script,
                                pane,
                                pane_id,
                                app_state,
                                true,
                            );
                            pane.mark_dirty();
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }
                    if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                        pane.execute_js(aileron::servo::NETWORK_MONITOR_JS);
                        pane.execute_js(aileron::servo::CONSOLE_CAPTURE_JS);
                        pane.execute_js(
                            aileron::passwords::bitwarden::BitwardenClient::form_submit_observer_js(
                            ),
                        );
                        pane.suppress_context_menu();
                        pane.execute_js(
                            "setTimeout(function() { \
                                if (window._aileron_scroll_pos) { \
                                    window.scrollTo(window._aileron_scroll_pos.x, window._aileron_scroll_pos.y); \
                                } \
                            }, 100);"
                        );
                        pane.execute_js(&format!(
                            "setTimeout(function() {{ {} }}, 500);",
                            aileron::passwords::bitwarden::BitwardenClient::form_detect_report_js()
                        ));
                        pane.execute_js(
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

                    if let Some(ref css) = app_state.config.custom_css
                        && !css.trim().is_empty()
                        && let Some(pane) = offscreen_panes.get_mut(&pane_id)
                    {
                        let escaped = css
                            .replace('\\', "\\\\")
                            .replace('`', "\\`")
                            .replace('$', "\\$");
                        pane.execute_js(&format!(
                            "setTimeout(function() {{ \
                                var s = document.createElement('style'); \
                                s.textContent = `{}`; \
                                (document.head || document.documentElement).appendChild(s); \
                            }}, 0);",
                            escaped
                        ));
                        pane.mark_dirty();
                    }

                    let csp_headers = adblocker.get_csp_headers(&url);
                    if !csp_headers.is_empty() {
                        let csp = csp_headers.join("; ");
                        let escaped = csp.replace('\\', "\\\\").replace('\'', "\\'");
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            pane.execute_js(&format!(
                                "var meta = document.createElement('meta'); meta['http-equiv'] = 'Content-Security-Policy'; meta.content = '{}'; document.head.appendChild(meta);",
                                escaped
                            ));
                        }
                    }

                    // Apply per-site zoom if configured
                    if let Some(ref db) = app_state.db
                        && let Ok(settings) =
                            aileron::db::site_settings::get_site_settings_for_url(db, &url)
                        && let Some(zoom) = settings.iter().find_map(|s| s.zoom_level)
                        && let Some(pane) = offscreen_panes.get_mut(&pane_id)
                    {
                        pane.execute_js(&format!(
                            "if(document.body) document.body.style.zoom = '{:.2}';",
                            zoom
                        ));
                    }
                }
            }
            WryEvent::LoadStarted { url, pane_id, .. } => {
                app_state.autofill_available = false;
                app_state.autofill_username_id.clear();
                app_state.autofill_password_id.clear();
                app_state.autofill_js = None;
                app_state.autofill_status_msg.clear();
                app_state.clear_injected_scripts(pane_id);
                app_state.update_a11y(&format!("Loading: {}...", &url[..url.len().min(40)]));
                if !url.starts_with("aileron://") {
                    let start_scripts = content_scripts.scripts_for_url(&url, RunAt::DocumentStart);
                    for script in start_scripts {
                        let key = format!("userscript:{}", script.name);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            info!(
                                "Injecting document-start script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            pane.execute_js(&script.js_code);
                            pane.mark_dirty();
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }
                    let ext_scripts =
                        content_scripts.extension_scripts_for_url(&url, RunAt::DocumentStart);
                    for ext_script in ext_scripts {
                        let key = format!("{}:{}", ext_script.extension_id, ext_script.script_id);
                        if app_state.is_script_injected(pane_id, &key) {
                            continue;
                        }
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            inject_extension_shim_and_script(
                                &ext_script,
                                pane,
                                pane_id,
                                app_state,
                                false,
                            );
                            pane.mark_dirty();
                            app_state.mark_script_injected(pane_id, &key);
                        }
                    }
                }
            }
            WryEvent::TitleChanged { title, .. } => {
                app_state.update_a11y(&title[..title.len().min(60)]);
                app_state.tab_display_dirty = true;
            }
            WryEvent::DownloadStarted { url, filename, .. } => {
                // Use the download manager for actual downloading with progress
                let dl_id = app_state
                    .download_manager
                    .start(url.as_str(), Some(filename.as_str()));
                let short_url = if url.len() > 40 { &url[..37] } else { &url };
                app_state.status_message =
                    format!("Download #{}: {} ({})", dl_id, filename, short_url);
                info!("Download #{} started: {} from {}", dl_id, filename, url);
                // Record in database for history
                if let Some(db) = app_state.db.as_ref() {
                    let dest = app_state
                        .download_manager
                        .downloads_dir()
                        .join(filename.as_str());
                    if let Err(e) = aileron::db::downloads::record_download(
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
                let _ = open_that(&path);
                app_state.status_message = format!("Opened: {}", path);
            }
            WryEvent::HttpsUpgraded { to, .. } => {
                app_state.status_message = format!("HTTPS upgrade: {}", to);
                if let Some(pane) = offscreen_panes.get_mut(&_pane_id) {
                    pane.mark_dirty();
                }
            }
            WryEvent::IpcMessage { pane_id, message } => {
                handle_ipc_message_offscreen(app_state, offscreen_panes, pane_id, &message);
            }
        }
    }
}

/// Check all offscreen panes for crash detection.
/// A pane is considered crashed if it has been loading for >15 seconds
/// with no activity (no events, no frame updates).
pub fn check_offscreen_crashes(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
) {
    let crash_timeout = std::time::Duration::from_secs(15);

    for (pane_id, pane) in offscreen_panes.iter_mut() {
        if pane.is_crashed(crash_timeout) && !app_state.webview_crash_detected {
            let url = pane.url().to_string();
            warn!(
                "WebView crash detected in pane {}: stalled while loading {}",
                &pane_id.to_string()[..8],
                &url[..url.len().min(80)]
            );
            app_state.webview_crash_detected = true;
            app_state.crashed_pane_url = Some(url);
            app_state.crashed_pane_id = Some(*pane_id);
            app_state.status_message =
                "WebView appears crashed — type :crash-reload to recover".into();
            pane.set_loading(false);
        }
    }
}

pub fn process_pending_wry_actions(
    app_state: &mut Option<AppState>,
    wry_panes: &mut WryPaneManager,
    offscreen_panes: &mut OffscreenWebViewManager,
    content_scripts: &ContentScriptManager,
) {
    let (pending_actions, active_id) = {
        let app_state = match app_state {
            Some(s) => s,
            None => return,
        };
        let actions: Vec<WryAction> = app_state.pending_wry_actions.drain(..).collect();
        let id = app_state.wm.active_pane_id();
        (actions, id)
    };
    for action in pending_actions {
        if let Err(e) = aileron::wry_actions::process_wry_action(
            action,
            active_id,
            wry_panes,
            offscreen_panes,
            app_state,
            content_scripts,
        ) {
            warn!("WryAction error: {}", e);
            if let Some(app_state) = app_state {
                app_state.status_message = format!("Action failed: {}", e);
            }
        }
    }
}

pub fn process_mcp_commands(
    mcp_bridge: &McpBridge,
    wry_panes: &mut WryPaneManager,
    active_id: Uuid,
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
) {
    let mcp_commands: Vec<McpCommand> = mcp_bridge.poll_commands().collect();

    for command in mcp_commands {
        match command {
            McpCommand::Navigate { url, new_tab } => {
                if let Ok(parsed) = url::Url::parse(&url) {
                    if new_tab {
                        let current_active = app_state.wm.active_pane_id();
                        match app_state.wm.split(
                            current_active,
                            aileron::wm::SplitDirection::Vertical,
                            0.5,
                        ) {
                            Ok(new_id) => {
                                info!("MCP: opening in new tab {}", url);
                                app_state.engines.create_pane(new_id, parsed, None);
                                app_state.wm.set_active_pane(new_id);
                                app_state.session_dirty = true;
                            }
                            Err(e) => {
                                warn!("MCP: failed to create new tab: {}", e);
                            }
                        }
                    } else if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
                        info!("MCP: navigating to {}", url);
                        wry_pane.navigate(&parsed);
                    }
                } else {
                    warn!("MCP: invalid navigate URL: {}", url);
                }
            }
            McpCommand::ExecuteJs { code, response_tx } => {
                if let Some(wry_pane) = wry_panes.get(&active_id) {
                    info!("MCP: executing JS ({} chars)", code.len());
                    let tx = std::sync::Mutex::new(Some(response_tx));
                    wry_pane.execute_js_with_callback(&code, move |result| {
                        if let Ok(mut guard) = tx.lock()
                            && let Some(sender) = guard.take()
                        {
                            let _ = sender.send(result);
                        }
                    });
                } else {
                    let _ = response_tx.send("Error: No active pane".to_string());
                }
            }
            McpCommand::GetActivePane { response_tx } => {
                let url = wry_panes
                    .get(&active_id)
                    .map(|p| p.url().as_str().to_string())
                    .unwrap_or_default();
                let title = wry_panes
                    .get(&active_id)
                    .map(|p| p.title().to_string())
                    .unwrap_or_default();
                let _ = response_tx.send((url, title));
            }
            McpCommand::ListBookmarks { response_tx } => {
                let result = if let Some(db) = app_state.db.as_ref() {
                    match aileron::db::bookmarks::all_bookmarks(db) {
                        Ok(bms) => {
                            let lines: Vec<String> = bms
                                .iter()
                                .map(|b| {
                                    let folder = if b.folder.is_empty() {
                                        "".into()
                                    } else {
                                        format!("[{}] ", b.folder)
                                    };
                                    format!("{}{} - {}", folder, b.title, b.url)
                                })
                                .collect();
                            lines.join("\n")
                        }
                        Err(e) => format!("Error: {}", e),
                    }
                } else {
                    "Error: No database".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::AddBookmark {
                url,
                title,
                folder,
                response_tx,
            } => {
                let result = if let Some(db) = app_state.db.as_ref() {
                    match aileron::db::bookmarks::add_bookmark_with_folder(
                        db, &url, &title, &folder,
                    ) {
                        Ok(id) => format!("Bookmarked (id={}) {}", id, url),
                        Err(e) => format!("Error: {}", e),
                    }
                } else {
                    "Error: No database".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::RemoveBookmark { url, response_tx } => {
                let result = if let Some(db) = app_state.db.as_ref() {
                    match aileron::db::bookmarks::remove_bookmark(db, &url) {
                        Ok(true) => format!("Removed bookmark: {}", url),
                        Ok(false) => format!("Not bookmarked: {}", url),
                        Err(e) => format!("Error: {}", e),
                    }
                } else {
                    "Error: No database".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::SearchHistory {
                query,
                limit,
                response_tx,
            } => {
                let result = if let Some(db) = app_state.db.as_ref() {
                    match aileron::db::history::search(db, &query, limit) {
                        Ok(entries) => {
                            let lines: Vec<String> = entries
                                .iter()
                                .map(|h| {
                                    format!("{} - {} ({} visits)", h.title, h.url, h.visit_count)
                                })
                                .collect();
                            lines.join("\n")
                        }
                        Err(e) => format!("Error: {}", e),
                    }
                } else {
                    "Error: No database".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::ListTabs { response_tx } => {
                let active = app_state.wm.active_pane_id();
                let pane_ids: Vec<Uuid> = app_state.wm.panes().iter().map(|(id, _)| *id).collect();
                let lines: Vec<String> = pane_ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        let marker = if *id == active { " [active]" } else { "" };
                        let url = wry_panes
                            .get(id)
                            .map(|p| p.url().to_string())
                            .unwrap_or_else(|| "about:blank".into());
                        let title = wry_panes
                            .get(id)
                            .map(|p| p.title().to_string())
                            .unwrap_or_else(|| "(untitled)".into());
                        format!("{}. {} - {}{}", i + 1, title, url, marker)
                    })
                    .collect();
                let result = if lines.is_empty() {
                    "No tabs open.".into()
                } else {
                    lines.join("\n")
                };
                let _ = response_tx.send(result);
            }
            McpCommand::Screenshot { response_tx } => {
                let result = if let Some(pane) = offscreen_panes.get_mut(&active_id) {
                    let dims = pane.frame().map(|f| (f.width, f.height));
                    pane.capture_frame();
                    let rgba = pane.frame_rgba().map(|r| r.to_vec());
                    let dims = dims.or_else(|| pane.frame().map(|f| (f.width, f.height)));
                    match (dims, rgba) {
                        (Some((w, h)), Some(rgba)) => {
                            match image::RgbaImage::from_raw(w, h, rgba) {
                                Some(img) => {
                                    let mut png_bytes = Vec::new();
                                    let encoder =
                                        image::codecs::png::PngEncoder::new(&mut png_bytes);
                                    if encoder
                                        .write_image(
                                            img.as_raw(),
                                            w,
                                            h,
                                            image::ExtendedColorType::Rgba8,
                                        )
                                        .is_ok()
                                    {
                                        use base64::Engine;
                                        let b64 = base64::engine::general_purpose::STANDARD
                                            .encode(&png_bytes);
                                        format!("data:image/png;base64,{}", b64)
                                    } else {
                                        "Error: failed to encode PNG".into()
                                    }
                                }
                                None => "Error: invalid frame dimensions".into(),
                            }
                        }
                        _ => "Error: no frame available".into(),
                    }
                } else {
                    "Error: no active pane".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::CloseTab { index, response_tx } => {
                let pane_ids: Vec<Uuid> = app_state.wm.pane_ids().into_iter().collect();
                let result = if let Some(&close_id) = pane_ids.get(index) {
                    let active_before = app_state.wm.active_pane_id();
                    app_state.wm.set_active_pane(close_id);
                    let _ = app_state.wm.close(close_id);
                    app_state.engines.remove_pane(&close_id);
                    app_state.terminal_pane_ids.remove(&close_id);
                    if active_before == close_id
                        && let Some(&next) = pane_ids.iter().find(|&&id| id != close_id)
                    {
                        app_state.wm.set_active_pane(next);
                    }
                    app_state.session_dirty = true;
                    format!("Closed tab at index {}.", index)
                } else {
                    format!(
                        "Error: tab index {} out of range ({} tabs open)",
                        index,
                        pane_ids.len()
                    )
                };
                let _ = response_tx.send(result);
            }
        }
    }
}

pub fn handle_pending_tab_close(app_state: &mut AppState, close_id: Uuid) {
    app_state.wm.set_active_pane(close_id);
    let _ = app_state.wm.close(close_id);
    app_state.engines.remove_pane(&close_id);
    app_state.terminal_pane_ids.remove(&close_id);
    app_state.update_a11y("Pane closed");
}

/// Handle pending bookmark import (Firefox or Chrome).
pub fn handle_pending_import(app_state: &mut AppState) {
    let source = match app_state.pending_import.take() {
        Some(s) => s,
        None => return,
    };
    if let Some(db) = app_state.db.as_ref() {
        let msg = match source.as_str() {
            "firefox" => aileron::app::cmd::import::import_firefox(db),
            "chrome" => aileron::app::cmd::import::import_chrome(db),
            _ => {
                app_state.status_message = format!("Unknown import source: {}", source);
                return;
            }
        };
        app_state.status_message = msg;
    } else {
        app_state.status_message = "No database available for import.".into();
    }
}

/// Poll native terminals for new output and feed VT parser.
pub fn poll_terminal_output(terminal_manager: &mut NativeTerminalManager) {
    terminal_manager.tick_all();
}

pub fn pump_gtk_loop() {
    pump_gtk();
}

pub fn load_default_adblock_rules(adblocker: &mut aileron::net::adblock::AdBlocker) {
    let default_filters = [
        "||doubleclick.net^",
        "||googlesyndication.com^",
        "||googleadservices.com^",
        "||adnxs.com^",
        "||adsrvr.org^",
        "||amazon-adsystem.com^",
        "||facebook.net^/signal",
        "||analytics.google.com^",
        "##div.ad-banner",
        "##.sponsored-content",
        "##.ad-container",
    ];
    for filter in &default_filters {
        let _ = adblocker.load_filter_list(filter);
    }
}

pub fn spawn_mcp_server(mcp_bridge: &McpBridge) {
    use aileron::mcp::tools;
    let mcp_state = mcp_bridge.state.clone();
    let mcp_command_tx = mcp_bridge.command_tx.clone();
    let tool_list = tools::create_tools(mcp_state, mcp_command_tx);
    let mut mcp_server = aileron::mcp::McpServer::new();
    for tool in tool_list {
        mcp_server.register_tool(tool);
    }
    let transport = aileron::mcp::McpTransport::new(mcp_server);
    info!("MCP server starting on background thread (stdio transport)");
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create MCP tokio runtime");
        rt.block_on(async {
            if let Err(e) = tokio::task::spawn_blocking(move || transport.run_stdio()).await {
                warn!("MCP server error: {}", e);
            }
        });
    });
}

fn handle_ipc_message(
    app_state: &mut AppState,
    wry_panes: &mut WryPaneManager,
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
        if let Some(pane) = wry_panes.get_mut(&pane_id) {
            let display_msg = format!("Failed to load: {}\n\n{}", failed_url, error_detail);
            let encoded = urlencoding::encode(&display_msg);
            if let Ok(error_url) = url::Url::parse(&format!("aileron://error?msg={}", encoded)) {
                pane.navigate(&error_url);
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
            let config_json = if app_state.config_json_dirty {
                app_state.config_json_cache =
                    serde_json::to_string(&app_state.config).unwrap_or_default();
                app_state.config_json_dirty = false;
                app_state.config_json_cache.clone()
            } else {
                app_state.config_json_cache.clone()
            };
            let js = format!(
                "window._aileron_config = {}; window._onConfigLoaded && window._onConfigLoaded(window._aileron_config);",
                config_json
            );
            if let Some(pane) = wry_panes.get_mut(&pane_id) {
                pane.execute_js(&js);
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
                if let Some(v) = config_obj
                    .get("sync_passphrase")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    match aileron::passwords::keyring::store_credential("aileron-sync", v) {
                        Ok(()) => {
                            app_state.config.sync_passphrase = v.to_string();
                            info!("Sync passphrase stored in system keyring");
                        }
                        Err(e) => {
                            warn!("Failed to store sync passphrase in keyring: {}", e);
                            app_state.status_message = format!("Failed to store passphrase: {}", e);
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
                if let Err(e) = aileron::config::Config::save(&app_state.config) {
                    warn!("Failed to save config: {}", e);
                }
                if let Some(pane) = wry_panes.get_mut(&pane_id) {
                    pane.execute_js("window._onConfigSaved && window._onConfigSaved();");
                }
                app_state.status_message = "Settings saved".into();
                app_state.config_json_dirty = true;
            }
        }
        Some("credential_save") => {
            if let (Some(username), Some(password), Some(url)) = (
                msg.get("username").and_then(|v| v.as_str()),
                msg.get("password").and_then(|v| v.as_str()),
                msg.get("url").and_then(|v| v.as_str()),
            ) {
                let key = format!("{}@{}", username, url);
                match aileron::passwords::keyring::store_credential(&key, password) {
                    Ok(()) => {
                        info!("Saved credential for {}", username);
                        app_state.status_message = format!("Credential saved for {}", username);
                    }
                    Err(e) => {
                        warn!("Failed to store credential: {}", e);
                        app_state.status_message = format!("Credential save failed: {}", e);
                    }
                }
            } else {
                app_state.status_message = "No pending credentials to save".into();
            }
        }
        Some("scroll-fraction") => {
            if let Some(frac) = msg.get("frac").and_then(|v| v.as_f64())
                && let Some(mark_char) = app_state.pending_mark_set.take()
            {
                let frac = frac.clamp(0.0, 1.0);
                app_state.store_mark_fraction(pane_id, mark_char, frac);
                tracing::debug!("Mark '{}' set at fraction {}", mark_char, frac);
                // Persist to database keyed by URL
                if let Some(pane) = wry_panes.get(&pane_id) {
                    let url = pane.url().to_string();
                    if let Some(ref conn) = app_state.db
                        && let Err(e) =
                            aileron::db::scroll_marks::set_scroll_mark(conn, &url, mark_char, frac)
                    {
                        tracing::warn!("Failed to persist scroll mark: {}", e);
                    }
                }
            }
        }
        Some("hint-clicked") => {
            app_state.hint_mode = false;
            app_state.hint_buffer.clear();
            app_state.status_message.clear();
        }
        Some("login-form-detected") => {
            if msg
                .get("has_login")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                && pane_id == app_state.wm.active_pane_id()
                && app_state.bitwarden.is_unlocked()
                && let Some(pane) = wry_panes.get(&pane_id)
            {
                let url = pane.url().to_string();
                if let Ok(items) = app_state.bitwarden.search_for_url(&url)
                    && !items.is_empty()
                {
                    app_state.autofill_available = true;
                    if let Some(uid) = msg.get("username_id").and_then(|v| v.as_str()) {
                        app_state.autofill_username_id = uid.to_string();
                    }
                    if let Some(pid) = msg.get("password_id").and_then(|v| v.as_str()) {
                        app_state.autofill_password_id = pid.to_string();
                    }
                    if let Ok(cred) = app_state.bitwarden.get_credential(&items[0].id) {
                        let domain = url::Url::parse(&url)
                            .ok()
                            .and_then(|u| u.domain().map(String::from))
                            .unwrap_or_else(|| "unknown".into());
                        let js = app_state.bitwarden.autofill_by_id_js(
                            &app_state.autofill_username_id,
                            &app_state.autofill_password_id,
                            &cred,
                        );
                        app_state.autofill_js = Some(js);
                        app_state.autofill_status_msg =
                            format!("Auto-filled credentials for {}", domain);
                    }
                }
            } else {
                app_state.autofill_available = false;
                app_state.autofill_js = None;
            }
        }
        Some("get-newtab-data") => {
            let bookmarks: Vec<serde_json::Value> = if let Some(db) = app_state.db.as_ref() {
                aileron::db::bookmarks::all_bookmarks(db)
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
                aileron::db::history::recent_entries(db, 8)
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
                "window._aileron_newtab_data = {}; if (window._onNewTabData) window._onNewTabData(window._aileron_newtab_data);",
                data
            );
            if let Some(pane) = wry_panes.get_mut(&pane_id) {
                pane.execute_js(&js);
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

            if let Ok(mgr) = app_state.extension_manager.lock() {
                let bus: &Arc<MessageBus> = mgr.message_bus();
                let response = bus.send_message(source_id.as_ref(), target_id.as_ref(), message);
                let response_json =
                    serde_json::to_string(&response).unwrap_or_else(|_| "null".into());
                if let Some(pane) = wry_panes.get_mut(&pane_id) {
                    pane.execute_js(&format!(
                        "if (window.__aileron_ext_response) \
                         window.__aileron_ext_response({}, {});",
                        serde_json::to_string(&req_id).unwrap_or_else(|_| "\"\"".into()),
                        response_json
                    ));
                }
            }
        }
        _ => {}
    }
}

fn handle_ipc_message_offscreen(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
    pane_id: Uuid,
    message: &str,
) {
    // Check for navigation error detection from ERROR_MONITOR_JS
    if let Some(error_msg) = message.strip_prefix("__aileron_nav_error__|") {
        let parts: Vec<&str> = error_msg.splitn(2, '|').collect();
        let failed_url = parts.first().copied().unwrap_or("unknown");
        let error_detail = parts.get(1).copied().unwrap_or("Unknown error");
        info!(
            "Navigation error detected in offscreen pane {}: {} — {}",
            &pane_id.to_string()[..8],
            failed_url,
            error_detail
        );
        app_state.update_a11y(&format!(
            "Load failed: {}",
            &error_detail[..error_detail.len().min(60)]
        ));
        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
            let display_msg = format!("Failed to load: {}\n\n{}", failed_url, error_detail);
            let encoded = urlencoding::encode(&display_msg);
            if let Ok(error_url) = url::Url::parse(&format!("aileron://error?msg={}", encoded)) {
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
            let config_json = if app_state.config_json_dirty {
                app_state.config_json_cache =
                    serde_json::to_string(&app_state.config).unwrap_or_default();
                app_state.config_json_dirty = false;
                app_state.config_json_cache.clone()
            } else {
                app_state.config_json_cache.clone()
            };
            let js = format!(
                "window._aileron_config = {}; window._onConfigLoaded && window._onConfigLoaded(window._aileron_config);",
                config_json
            );
            if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                pane.execute_js(&js);
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
                if let Some(v) = config_obj
                    .get("sync_passphrase")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    match aileron::passwords::keyring::store_credential("aileron-sync", v) {
                        Ok(()) => {
                            app_state.config.sync_passphrase = v.to_string();
                            info!("Sync passphrase stored in system keyring");
                        }
                        Err(e) => {
                            warn!("Failed to store sync passphrase in keyring: {}", e);
                            app_state.status_message = format!("Failed to store passphrase: {}", e);
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
                if let Err(e) = aileron::config::Config::save(&app_state.config) {
                    warn!("Failed to save config: {}", e);
                }
                if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                    pane.execute_js("window._onConfigSaved && window._onConfigSaved();");
                    pane.mark_dirty();
                }
                app_state.status_message = "Settings saved".into();
                app_state.config_json_dirty = true;
            }
        }
        Some("credential_save") => {
            if let (Some(username), Some(password), Some(url)) = (
                msg.get("username").and_then(|v| v.as_str()),
                msg.get("password").and_then(|v| v.as_str()),
                msg.get("url").and_then(|v| v.as_str()),
            ) {
                let key = format!("{}@{}", username, url);
                match aileron::passwords::keyring::store_credential(&key, password) {
                    Ok(()) => {
                        info!("Saved credential for {}", username);
                        app_state.status_message = format!("Credential saved for {}", username);
                    }
                    Err(e) => {
                        warn!("Failed to store credential: {}", e);
                        app_state.status_message = format!("Credential save failed: {}", e);
                    }
                }
            } else {
                app_state.status_message = "No pending credentials to save".into();
            }
        }
        Some("scroll-fraction") => {
            if let Some(frac) = msg.get("frac").and_then(|v| v.as_f64())
                && let Some(mark_char) = app_state.pending_mark_set.take()
            {
                let frac = frac.clamp(0.0, 1.0);
                app_state.store_mark_fraction(pane_id, mark_char, frac);
                tracing::debug!("Mark '{}' set at fraction {}", mark_char, frac);
                // Persist to database keyed by URL
                if let Some(pane) = offscreen_panes.get(&pane_id) {
                    let url = pane.url().to_string();
                    if let Some(ref conn) = app_state.db
                        && let Err(e) =
                            aileron::db::scroll_marks::set_scroll_mark(conn, &url, mark_char, frac)
                    {
                        tracing::warn!("Failed to persist scroll mark: {}", e);
                    }
                }
            }
        }
        Some("hint-clicked") => {
            app_state.hint_mode = false;
            app_state.hint_buffer.clear();
            app_state.status_message.clear();
        }
        Some("get-newtab-data") => {
            let bookmarks: Vec<serde_json::Value> = if let Some(db) = app_state.db.as_ref() {
                aileron::db::bookmarks::all_bookmarks(db)
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
                aileron::db::history::recent_entries(db, 8)
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
                "window._aileron_newtab_data = {}; if (window._onNewTabData) window._onNewTabData(window._aileron_newtab_data);",
                data
            );
            if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                pane.execute_js(&js);
                pane.mark_dirty();
            }
        }
        Some("login-form-detected") => {
            if msg
                .get("has_login")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                && pane_id == app_state.wm.active_pane_id()
                && app_state.bitwarden.is_unlocked()
                && let Some(pane) = offscreen_panes.get(&pane_id)
            {
                let url = pane.url().to_string();
                if let Ok(items) = app_state.bitwarden.search_for_url(&url)
                    && !items.is_empty()
                {
                    app_state.autofill_available = true;
                    if let Some(uid) = msg.get("username_id").and_then(|v| v.as_str()) {
                        app_state.autofill_username_id = uid.to_string();
                    }
                    if let Some(pid) = msg.get("password_id").and_then(|v| v.as_str()) {
                        app_state.autofill_password_id = pid.to_string();
                    }
                    if let Ok(cred) = app_state.bitwarden.get_credential(&items[0].id) {
                        let domain = url::Url::parse(&url)
                            .ok()
                            .and_then(|u| u.domain().map(String::from))
                            .unwrap_or_else(|| "unknown".into());
                        let js = app_state.bitwarden.autofill_by_id_js(
                            &app_state.autofill_username_id,
                            &app_state.autofill_password_id,
                            &cred,
                        );
                        app_state.autofill_js = Some(js);
                        app_state.autofill_status_msg =
                            format!("Auto-filled credentials for {}", domain);
                    }
                }
            } else {
                app_state.autofill_available = false;
                app_state.autofill_js = None;
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

            if let Ok(mgr) = app_state.extension_manager.lock() {
                let bus: &Arc<MessageBus> = mgr.message_bus();
                let response = bus.send_message(source_id.as_ref(), target_id.as_ref(), message);
                let response_json =
                    serde_json::to_string(&response).unwrap_or_else(|_| "null".into());
                if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                    pane.execute_js(&format!(
                        "if (window.__aileron_ext_response) \
                         window.__aileron_ext_response({}, {});",
                        serde_json::to_string(&req_id).unwrap_or_else(|_| "\"\"".into()),
                        response_json
                    ));
                    pane.mark_dirty();
                }
            }
        }
        _ => {}
    }
}

// End of file

trait ExecuteJs {
    fn execute_js_code(&self, js: &str);
}

impl ExecuteJs for aileron::servo::WryPane {
    fn execute_js_code(&self, js: &str) {
        self.execute_js(js);
    }
}

impl ExecuteJs for aileron::offscreen_webview::OffscreenWebView {
    fn execute_js_code(&self, js: &str) {
        self.execute_js(js);
    }
}

fn inject_extension_shim_and_script<T>(
    ext_script: &aileron::extensions::scripting::ExtensionContentScriptEntry,
    pane: &T,
    pane_id: Uuid,
    app_state: &mut AppState,
    use_idle_delay: bool,
) where
    T: ExecuteJs,
{
    let ext_id = ExtensionId(ext_script.extension_id.clone());
    let is_loaded = app_state
        .extension_manager
        .lock()
        .ok()
        .is_some_and(|mgr| mgr.get(&ext_id).is_some());
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
                    s.textContent = `{}`; \
                    (document.head || document.documentElement).appendChild(s); \
                }}, 0);",
                escaped
            )
        } else {
            format!(
                "var s = document.createElement('style'); \
                 s.textContent = `{}`; \
                 (document.head || document.documentElement).appendChild(s);",
                escaped
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
// End of file
