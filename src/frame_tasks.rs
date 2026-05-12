use open::that as open_that;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

#[cfg(feature = "mcp")]
use image::ImageEncoder;

use crate::app::{AppState, WryAction};
#[cfg(feature = "arp")]
use crate::arp::ArpCommand;
use crate::extensions::web_request::WebRequestInterceptorRegistry;
use crate::extensions::{ExtensionId, MessageBus};
use crate::git::GitStatus;
#[cfg(feature = "mcp")]
use crate::mcp::{McpBridge, McpCommand};
use crate::offscreen_webview::OffscreenWebViewManager;
use crate::scripts::{ContentScriptManager, RunAt};
use crate::servo::{WryEvent, WryPaneManager, pump_gtk};
use crate::terminal::NativeTerminalManager;

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

pub fn poll_git_status(git_status: &mut GitStatus, git_poller: &Option<crate::git::GitPoller>) {
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
    if !app_state.session.session_dirty {
        return;
    }
    let interval = std::time::Duration::from_secs(app_state.config.auto_save_interval);
    if app_state.session.last_auto_save.elapsed() < interval {
        return;
    }
    app_state.session.last_auto_save = std::time::Instant::now();

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
#[cfg(feature = "arp")]
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
                "muted": app_state.tabs.muted_pane_ids.contains(id),
                "pinned": app_state.tabs.pinned_pane_ids.contains(id),
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
#[cfg(feature = "arp")]
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
                    .split(active, crate::wm::SplitDirection::Vertical, 0.5)
                {
                    Ok(new_id) => {
                        let target_url = url
                            .and_then(|u| url::Url::parse(&u).ok())
                            .unwrap_or_else(|| url::Url::parse("aileron://newtab").unwrap());
                        app_state.engines.create_pane(new_id, target_url, None);
                        app_state.session.session_dirty = true;
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
                    app_state.session.session_dirty = true;
                }
                Err(e) => {
                    warn!(target: "arp", "Tab navigate invalid URL: {}", e);
                }
            },
            ArpCommand::TabClose { tab_id } => {
                let target = tab_id.unwrap_or_else(|| app_state.wm.active_pane_id());
                match app_state.wm.close(target) {
                    Ok(_next) => {
                        app_state.session.session_dirty = true;
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
                let contents = crate::platform::platform()
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

fn process_wry_events_inner(
    app_state: &mut AppState,
    wry_panes: &mut WryPaneManager,
    content_scripts: &ContentScriptManager,
    adblocker: &crate::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
) {
    let wry_events = wry_panes.poll_all_events();
    for event in wry_events {
        match event {
            WryEvent::LoadComplete { pane_id, url, .. } => {
                app_state.session.session_dirty = true;
                app_state.tabs.tab_display_dirty = true;
                app_state.cache.pane_count_dirty = true;
                if let Ok(parsed) = url::Url::parse(&url) {
                    app_state.record_visit(&parsed, &url);
                }
                app_state.update_a11y(&format!("Loaded: {}", &url[..url.len().min(60)]));

                // Fire extension onCompleted lifecycle event
                if interceptor_registry.has_interceptors()
                    && let Ok(parsed_url) = url::Url::parse(&url)
                {
                    let details = crate::extensions::web_request::CompletedDetails {
                        request_id: crate::extensions::types::RequestId(0),
                        url: parsed_url,
                        frame_id: crate::extensions::types::FrameId(0),
                        tab_id: None,
                        type_: crate::extensions::web_request::ResourceType::MainFrame,
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
                        wry_pane.execute_js(crate::servo::NETWORK_MONITOR_JS);
                        wry_pane.execute_js(crate::servo::CONSOLE_CAPTURE_JS);
                        #[cfg(feature = "passwords")]
                        wry_pane.execute_js(
                            crate::passwords::bitwarden::BitwardenClient::form_submit_observer_js(),
                        );
                        wry_pane.execute_js(
                            "setTimeout(function() { \
                                if (window._aileron_scroll_pos) { \
                                    window.scrollTo(window._aileron_scroll_pos.x, window._aileron_scroll_pos.y); \
                                } \
                            }, 100);"
                        );
                        #[cfg(feature = "passwords")]
                        wry_pane.execute_js(&format!(
                            "setTimeout(function() {{ {} }}, 500);",
                            crate::passwords::bitwarden::BitwardenClient::form_detect_report_js()
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
                                s.textContent = `{escaped}`; \
                                (document.head || document.documentElement).appendChild(s); \
                            }}, 0);"
                        ));
                    }

                    let csp_headers = adblocker.get_csp_headers(&url);
                    if !csp_headers.is_empty() {
                        let csp = csp_headers.join("; ");
                        let escaped = csp.replace('\\', "\\\\").replace('\'', "\\'");
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            wry_pane.execute_js(&format!(
                                "var meta = document.createElement('meta'); meta['http-equiv'] = 'Content-Security-Policy'; meta.content = '{escaped}'; document.head.appendChild(meta);"
                            ));
                        }
                    }

                    // Apply per-site zoom if configured
                    if let Some(ref db) = app_state.db
                        && let Ok(settings) =
                            crate::db::site_settings::get_site_settings_for_url(db, &url)
                        && let Some(zoom) = settings.iter().find_map(|s| s.zoom_level)
                        && let Some(wry_pane) = wry_panes.get_mut(&pane_id)
                    {
                        wry_pane.execute_js(&format!(
                            "if(document.body) document.body.style.zoom = '{zoom:.2}';"
                        ));
                    }
                }
            }
            WryEvent::LoadStarted { url, pane_id, .. } => {
                app_state.autofill.available = false;
                app_state.autofill.username_id.clear();
                app_state.autofill.password_id.clear();
                app_state.autofill.js = None;
                app_state.autofill.status_msg.clear();
                app_state.clear_injected_scripts(pane_id);
                app_state.update_a11y(&format!("Loading: {}...", &url[..url.len().min(40)]));

                // Fire extension onBeforeRequest lifecycle event
                if interceptor_registry.has_interceptors()
                    && let Ok(parsed_url) = url::Url::parse(&url)
                {
                    let details = crate::extensions::web_request::RequestDetails {
                        request_id: crate::extensions::types::RequestId(0),
                        url: parsed_url,
                        method: "GET".into(),
                        frame_id: crate::extensions::types::FrameId(0),
                        parent_frame_id: crate::extensions::types::FrameId(u32::MAX),
                        tab_id: None,
                        type_: crate::extensions::web_request::ResourceType::MainFrame,
                        origin_url: None,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs_f64() * 1000.0)
                            .unwrap_or(0.0),
                        request_headers: None,
                    };
                    let response = interceptor_registry.fire_on_before_request(&details);
                    if response.cancel == Some(true) {
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            wry_pane.navigate(&url::Url::parse("about:blank").unwrap());
                        }
                        continue;
                    }
                    if let Some(redirect_url) = response.redirect_url {
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            wry_pane.navigate(&redirect_url);
                        }
                        continue;
                    }
                }

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
                app_state.tabs.tab_display_dirty = true;
            }
            WryEvent::DownloadStarted { url, filename, .. } => {
                // Use the download manager for actual downloading with progress
                let dl_id = app_state
                    .download_manager
                    .start(url.as_str(), Some(filename.as_str()));
                let short_url = if url.len() > 40 { &url[..37] } else { &url };
                app_state.ui.status_message =
                    format!("Download #{dl_id}: {filename} ({short_url})");
                info!("Download #{} started: {} from {}", dl_id, filename, url);
                // Record in database for history
                if let Some(db) = app_state.db.as_ref() {
                    let dest = app_state
                        .download_manager
                        .downloads_dir()
                        .join(filename.as_str());
                    if let Err(e) = crate::db::downloads::record_download(
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
                app_state.ui.status_message = format!("Opened: {path}");
            }
            WryEvent::HttpsUpgraded { to, .. } => {
                app_state.ui.status_message = format!("HTTPS upgrade: {to}");
            }
            WryEvent::IpcMessage { pane_id, message } => {
                handle_ipc_message(app_state, wry_panes, pane_id, &message);
            }
        }
    }
}

#[cfg(feature = "mcp")]
pub fn process_wry_events(
    app_state: &mut AppState,
    wry_panes: &mut WryPaneManager,
    content_scripts: &ContentScriptManager,
    mcp_bridge: &mut McpBridge,
    adblocker: &crate::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
) {
    process_wry_events_inner(
        app_state,
        wry_panes,
        content_scripts,
        adblocker,
        interceptor_registry,
    );
    let active_id = app_state.wm.active_pane_id();
    if let Some(wry_pane) = wry_panes.get(&active_id) {
        mcp_bridge.update_state(wry_pane.url().as_str(), wry_pane.title());
    }
}

#[cfg(not(feature = "mcp"))]
pub fn process_wry_events(
    app_state: &mut AppState,
    wry_panes: &mut WryPaneManager,
    content_scripts: &ContentScriptManager,
    adblocker: &crate::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
) {
    process_wry_events_inner(
        app_state,
        wry_panes,
        content_scripts,
        adblocker,
        interceptor_registry,
    );
}

fn process_offscreen_events_inner(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
    content_scripts: &ContentScriptManager,
    adblocker: &crate::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
) {
    let events = offscreen_panes.drain_all_events();
    for (_pane_id, event) in events {
        match event {
            WryEvent::LoadComplete { pane_id, url, .. } => {
                app_state.session.session_dirty = true;
                app_state.tabs.tab_display_dirty = true;
                app_state.cache.pane_count_dirty = true;
                if let Ok(parsed) = url::Url::parse(&url) {
                    app_state.record_visit(&parsed, &url);
                }
                app_state.update_a11y(&format!("Loaded: {}", &url[..url.len().min(60)]));

                // Fire extension onCompleted lifecycle event
                if interceptor_registry.has_interceptors()
                    && let Ok(parsed_url) = url::Url::parse(&url)
                {
                    let details = crate::extensions::web_request::CompletedDetails {
                        request_id: crate::extensions::types::RequestId(0),
                        url: parsed_url,
                        frame_id: crate::extensions::types::FrameId(0),
                        tab_id: None,
                        type_: crate::extensions::web_request::ResourceType::MainFrame,
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
                        pane.execute_js(crate::servo::NETWORK_MONITOR_JS);
                        pane.execute_js(crate::servo::CONSOLE_CAPTURE_JS);
                        #[cfg(feature = "passwords")]
                        pane.execute_js(
                            crate::passwords::bitwarden::BitwardenClient::form_submit_observer_js(),
                        );
                        pane.suppress_context_menu();
                        pane.execute_js(
                            "setTimeout(function() { \
                                if (window._aileron_scroll_pos) { \
                                    window.scrollTo(window._aileron_scroll_pos.x, window._aileron_scroll_pos.y); \
                                } \
                            }, 100);"
                        );
                        #[cfg(feature = "passwords")]
                        pane.execute_js(&format!(
                            "setTimeout(function() {{ {} }}, 500);",
                            crate::passwords::bitwarden::BitwardenClient::form_detect_report_js()
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
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            pane.execute_js(&format!(
                                "var meta = document.createElement('meta'); meta['http-equiv'] = 'Content-Security-Policy'; meta.content = '{escaped}'; document.head.appendChild(meta);"
                            ));
                        }
                    }

                    // Apply per-site zoom if configured
                    if let Some(ref db) = app_state.db
                        && let Ok(settings) =
                            crate::db::site_settings::get_site_settings_for_url(db, &url)
                        && let Some(zoom) = settings.iter().find_map(|s| s.zoom_level)
                        && let Some(pane) = offscreen_panes.get_mut(&pane_id)
                    {
                        pane.execute_js(&format!(
                            "if(document.body) document.body.style.zoom = '{zoom:.2}';"
                        ));
                    }
                }
            }
            WryEvent::LoadStarted { url, pane_id, .. } => {
                app_state.autofill.available = false;
                app_state.autofill.username_id.clear();
                app_state.autofill.password_id.clear();
                app_state.autofill.js = None;
                app_state.autofill.status_msg.clear();
                app_state.clear_injected_scripts(pane_id);
                app_state.update_a11y(&format!("Loading: {}...", &url[..url.len().min(40)]));

                // Fire extension onBeforeRequest lifecycle event
                if interceptor_registry.has_interceptors()
                    && let Ok(parsed_url) = url::Url::parse(&url)
                {
                    let details = crate::extensions::web_request::RequestDetails {
                        request_id: crate::extensions::types::RequestId(0),
                        url: parsed_url,
                        method: "GET".into(),
                        frame_id: crate::extensions::types::FrameId(0),
                        parent_frame_id: crate::extensions::types::FrameId(u32::MAX),
                        tab_id: None,
                        type_: crate::extensions::web_request::ResourceType::MainFrame,
                        origin_url: None,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs_f64() * 1000.0)
                            .unwrap_or(0.0),
                        request_headers: None,
                    };
                    let response = interceptor_registry.fire_on_before_request(&details);
                    if response.cancel == Some(true) {
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            pane.navigate(&url::Url::parse("about:blank").unwrap());
                            pane.mark_dirty();
                        }
                        continue;
                    }
                    if let Some(redirect_url) = response.redirect_url {
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
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
                app_state.tabs.tab_display_dirty = true;
            }
            WryEvent::DownloadStarted { url, filename, .. } => {
                // Use the download manager for actual downloading with progress
                let dl_id = app_state
                    .download_manager
                    .start(url.as_str(), Some(filename.as_str()));
                let short_url = if url.len() > 40 { &url[..37] } else { &url };
                app_state.ui.status_message =
                    format!("Download #{dl_id}: {filename} ({short_url})");
                info!("Download #{} started: {} from {}", dl_id, filename, url);
                // Record in database for history
                if let Some(db) = app_state.db.as_ref() {
                    let dest = app_state
                        .download_manager
                        .downloads_dir()
                        .join(filename.as_str());
                    if let Err(e) = crate::db::downloads::record_download(
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
                app_state.ui.status_message = format!("Opened: {path}");
            }
            WryEvent::HttpsUpgraded { to, .. } => {
                app_state.ui.status_message = format!("HTTPS upgrade: {to}");
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

#[cfg(feature = "mcp")]
pub fn process_offscreen_events(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
    content_scripts: &ContentScriptManager,
    _mcp_bridge: &mut McpBridge,
    adblocker: &crate::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
) {
    process_offscreen_events_inner(
        app_state,
        offscreen_panes,
        content_scripts,
        adblocker,
        interceptor_registry,
    );
}

#[cfg(not(feature = "mcp"))]
pub fn process_offscreen_events(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
    content_scripts: &ContentScriptManager,
    adblocker: &crate::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
) {
    process_offscreen_events_inner(
        app_state,
        offscreen_panes,
        content_scripts,
        adblocker,
        interceptor_registry,
    );
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
        if pane.is_crashed(crash_timeout) && !app_state.crash.webview_crash_detected {
            let url = pane.url().to_string();
            warn!(
                "WebView crash detected in pane {}: stalled while loading {}",
                &pane_id.to_string()[..8],
                &url[..url.len().min(80)]
            );
            app_state.crash.webview_crash_detected = true;
            app_state.crash.crashed_pane_url = Some(url);
            app_state.crash.crashed_pane_id = Some(*pane_id);
            app_state.ui.status_message =
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
    // Drain any navigations queued by the Lua engine (aileron.navigate from hooks/init.lua)
    if let Some(state) = app_state {
        state.drain_lua_navigations();
    }
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
        if let Err(e) = crate::wry_actions::process_wry_action(
            action,
            active_id,
            wry_panes,
            offscreen_panes,
            app_state,
            content_scripts,
        ) {
            warn!("WryAction error: {}", e);
            if let Some(app_state) = app_state {
                app_state.ui.status_message = format!("Action failed: {e}");
            }
        }
    }
}

#[cfg(feature = "mcp")]
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
                            crate::wm::SplitDirection::Vertical,
                            0.5,
                        ) {
                            Ok(new_id) => {
                                info!("MCP: opening in new tab {}", url);
                                app_state.engines.create_pane(new_id, parsed, None);
                                app_state.wm.set_active_pane(new_id);
                                app_state.session.session_dirty = true;
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
                    match crate::db::bookmarks::all_bookmarks(db) {
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
                        Err(e) => format!("Error: {e}"),
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
                    match crate::db::bookmarks::add_bookmark_with_folder(db, &url, &title, &folder)
                    {
                        Ok(id) => format!("Bookmarked (id={id}) {url}"),
                        Err(e) => format!("Error: {e}"),
                    }
                } else {
                    "Error: No database".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::RemoveBookmark { url, response_tx } => {
                let result = if let Some(db) = app_state.db.as_ref() {
                    match crate::db::bookmarks::remove_bookmark(db, &url) {
                        Ok(true) => format!("Removed bookmark: {url}"),
                        Ok(false) => format!("Not bookmarked: {url}"),
                        Err(e) => format!("Error: {e}"),
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
                    match crate::db::history::search(db, &query, limit) {
                        Ok(entries) => {
                            let lines: Vec<String> = entries
                                .iter()
                                .map(|h| {
                                    format!("{} - {} ({} visits)", h.title, h.url, h.visit_count)
                                })
                                .collect();
                            lines.join("\n")
                        }
                        Err(e) => format!("Error: {e}"),
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
                    if pane.capture_frame().is_none() {
                        warn!("Screenshot frame capture returned no data");
                    }
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
                                        format!("data:image/png;base64,{b64}")
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
                    if let Err(e) = app_state.wm.close(close_id) {
                        warn!(%e, "Failed to close tab via MCP");
                    }
                    app_state.engines.remove_pane(&close_id);
                    app_state.terminal_pane_ids.remove(&close_id);
                    if active_before == close_id
                        && let Some(&next) = pane_ids.iter().find(|&&id| id != close_id)
                    {
                        app_state.wm.set_active_pane(next);
                    }
                    app_state.session.session_dirty = true;
                    format!("Closed tab at index {index}.")
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
    if let Err(e) = app_state.wm.close(close_id) {
        warn!(%e, "Failed to close pending tab");
    }
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
            "firefox" => crate::app::cmd::import::import_firefox(db),
            "chrome" => crate::app::cmd::import::import_chrome(db),
            _ => {
                app_state.ui.status_message = format!("Unknown import source: {source}");
                return;
            }
        };
        app_state.ui.status_message = msg;
    } else {
        app_state.ui.status_message = "No database available for import.".into();
    }
}

/// Poll native terminals for new output and feed VT parser.
pub fn poll_terminal_output(terminal_manager: &mut NativeTerminalManager) {
    terminal_manager.tick_all();
}

pub fn pump_gtk_loop() {
    pump_gtk();
}

pub fn load_default_adblock_rules(adblocker: &mut crate::net::adblock::AdBlocker) {
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
        if let Err(e) = adblocker.load_filter_list(filter) {
            warn!(%e, "Failed to load adblock filter");
        }
    }
}

#[cfg(feature = "mcp")]
pub fn spawn_mcp_server(mcp_bridge: &McpBridge) {
    use crate::mcp::tools;
    let mcp_state = mcp_bridge.state.clone();
    let mcp_command_tx = mcp_bridge.command_tx.clone();
    let tool_list = tools::create_tools(mcp_state, mcp_command_tx);
    let mut mcp_server = crate::mcp::McpServer::new();
    for tool in tool_list {
        mcp_server.register_tool(tool);
    }
    let transport = crate::mcp::McpTransport::new(mcp_server);
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
            let display_msg = format!("Failed to load: {failed_url}\n\n{error_detail}");
            let encoded = urlencoding::encode(&display_msg);
            if let Ok(error_url) = url::Url::parse(&format!("aileron://error?msg={encoded}")) {
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
                if let Some(pane) = wry_panes.get_mut(&pane_id) {
                    pane.execute_js("window._onConfigSaved && window._onConfigSaved();");
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
                if let Some(pane) = wry_panes.get(&pane_id) {
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
                && let Some(pane) = wry_panes.get(&pane_id)
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

            let mgr = app_state.extension_manager.read();
            let bus: &Arc<MessageBus> = mgr.message_bus();
            let response = bus.send_message(source_id.as_ref(), target_id.as_ref(), message);
            let response_json = serde_json::to_string(&response).unwrap_or_else(|_| "null".into());
            if let Some(pane) = wry_panes.get_mut(&pane_id) {
                pane.execute_js(&format!(
                    "if (window.__aileron_ext_response) \
                     window.__aileron_ext_response({}, {});",
                    serde_json::to_string(&req_id).unwrap_or_else(|_| "\"\"".into()),
                    response_json
                ));
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
                if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                    pane.execute_js("window._onConfigSaved && window._onConfigSaved();");
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
                if let Some(pane) = offscreen_panes.get(&pane_id) {
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
            if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                pane.execute_js(&js);
                pane.mark_dirty();
            }
        }
        #[cfg(feature = "passwords")]
        Some("login-form-detected") => {
            if msg
                .get("has_login")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                && pane_id == app_state.wm.active_pane_id()
                && app_state.bitwarden.is_unlocked()
                && let Some(pane) = offscreen_panes.get_mut(&pane_id)
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
        _ => {}
    }
}

// End of file

trait ExecuteJs {
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

fn inject_extension_shim_and_script<T>(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_viewport() -> crate::wm::Rect {
        crate::wm::Rect::new(0.0, 0.0, 1920.0, 1080.0)
    }

    fn test_app_state() -> AppState {
        AppState::new(test_viewport(), crate::config::Config::default()).unwrap()
    }

    // ─── poll_git_status ────────────────────────────────────────────────

    #[test]
    fn poll_git_status_none_poller_leaves_status_unchanged() {
        let mut status = GitStatus {
            branch: Some("main".into()),
            modified_count: 3,
            untracked_count: 1,
            is_dirty: true,
        };
        poll_git_status(&mut status, &None);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.modified_count, 3);
        assert!(status.is_dirty);
    }

    #[test]
    fn poll_git_status_with_empty_channel_leaves_status_unchanged() {
        let tmp = std::env::temp_dir().join("aileron_test_git_poller_none");
        let _ = std::fs::create_dir_all(&tmp);
        let poller = crate::git::GitPoller::new(tmp.clone(), std::time::Duration::from_secs(3600));
        let mut status = GitStatus {
            branch: Some("feature".into()),
            modified_count: 1,
            untracked_count: 0,
            is_dirty: true,
        };
        poll_git_status(&mut status, &Some(poller));
        assert_eq!(status.branch.as_deref(), Some("feature"));
        assert_eq!(status.modified_count, 1);
    }

    #[test]
    fn poll_git_status_with_new_poller_receives_initial_status() {
        let tmp = std::env::temp_dir().join("aileron_test_git_poller_recv");
        let _ = std::fs::create_dir_all(&tmp);
        let poller = crate::git::GitPoller::new(tmp.clone(), std::time::Duration::from_secs(3600));
        let mut status = GitStatus::default();
        poll_git_status(&mut status, &Some(poller));
    }

    // ─── auto_save_workspace ────────────────────────────────────────────

    #[test]
    fn auto_save_disabled_does_not_save() {
        let mut app_state = test_app_state();
        app_state.config.auto_save = false;
        app_state.session.session_dirty = true;
        app_state.session.last_auto_save = std::time::Instant::now()
            - std::time::Duration::from_secs(app_state.config.auto_save_interval + 10);
        let wry_panes = WryPaneManager::new();
        auto_save_workspace(&mut app_state, &wry_panes);
        assert!(app_state.session.session_dirty);
    }

    #[test]
    fn auto_save_session_not_dirty_does_not_save() {
        let mut app_state = test_app_state();
        app_state.config.auto_save = true;
        app_state.session.session_dirty = false;
        app_state.session.last_auto_save = std::time::Instant::now()
            - std::time::Duration::from_secs(app_state.config.auto_save_interval + 10);
        let wry_panes = WryPaneManager::new();
        auto_save_workspace(&mut app_state, &wry_panes);
    }

    #[test]
    fn auto_save_interval_not_elapsed_does_not_save() {
        let mut app_state = test_app_state();
        app_state.config.auto_save = true;
        app_state.session.session_dirty = true;
        app_state.session.last_auto_save = std::time::Instant::now();
        let wry_panes = WryPaneManager::new();
        auto_save_workspace(&mut app_state, &wry_panes);
    }

    // ─── push_tabs_to_arp ───────────────────────────────────────────────

    #[cfg(feature = "arp")]
    #[test]
    fn push_tabs_to_arp_no_server_does_nothing() {
        let app_state = test_app_state();
        assert!(app_state.arp_server.is_none());
        let wry_panes = WryPaneManager::new();
        push_tabs_to_arp(&app_state, &wry_panes);
    }

    #[cfg(feature = "arp")]
    #[test]
    fn push_tabs_to_arp_stopped_server_does_nothing() {
        let mut app_state = test_app_state();
        let Ok((server, _receiver)) = crate::arp::ArpServer::new(crate::arp::ArpConfig::default())
        else {
            return;
        };
        assert!(!server.is_running());
        app_state.arp_server = Some(server);
        let wry_panes = WryPaneManager::new();
        push_tabs_to_arp(&app_state, &wry_panes);
    }

    // ─── process_arp_commands ───────────────────────────────────────────

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_no_receiver_does_nothing() {
        let mut app_state = test_app_state();
        assert!(app_state.arp_cmd_receiver.is_none());
        process_arp_commands(&mut app_state);
    }

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_tab_navigate_pushes_action() {
        let mut app_state = test_app_state();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
        let _ = tx.send(ArpCommand::TabNavigate {
            tab_id: None,
            url: "https://example.com".into(),
        });
        process_arp_commands(&mut app_state);
        assert!(!app_state.pending_wry_actions.is_empty());
    }

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_clipboard_set_pushes_action() {
        let mut app_state = test_app_state();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
        let _ = tx.send(ArpCommand::ClipboardSet {
            text: "test clipboard".into(),
        });
        process_arp_commands(&mut app_state);
        assert!(!app_state.pending_wry_actions.is_empty());
    }

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_quickmark_open_no_match_does_nothing() {
        let mut app_state = test_app_state();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
        let _ = tx.send(ArpCommand::QuickmarkOpen {
            key: "nonexistent".into(),
        });
        process_arp_commands(&mut app_state);
        assert!(app_state.pending_wry_actions.is_empty());
    }

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_quickmark_open_with_default_quickmark_pushes_navigate() {
        let mut app_state = test_app_state();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
        let _ = tx.send(ArpCommand::QuickmarkOpen { key: "gh".into() });
        process_arp_commands(&mut app_state);
        assert_eq!(app_state.pending_wry_actions.len(), 1);
    }

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_tab_create_with_no_url() {
        let mut app_state = test_app_state();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
        let _ = tx.send(ArpCommand::TabCreate { url: None });
        process_arp_commands(&mut app_state);
        assert!(app_state.session.session_dirty);
    }

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_tab_close_with_none_target() {
        let mut app_state = test_app_state();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
        let _ = tx.send(ArpCommand::TabClose { tab_id: None });
        process_arp_commands(&mut app_state);
    }

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_tab_activate() {
        let mut app_state = test_app_state();
        let active_id = app_state.wm.active_pane_id();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
        let _ = tx.send(ArpCommand::TabActivate { tab_id: active_id });
        process_arp_commands(&mut app_state);
        assert_eq!(app_state.wm.active_pane_id(), active_id);
    }

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_tab_go_back() {
        let mut app_state = test_app_state();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
        let _ = tx.send(ArpCommand::TabGoBack { tab_id: None });
        process_arp_commands(&mut app_state);
        assert_eq!(app_state.pending_wry_actions.len(), 1);
    }

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_tab_go_forward() {
        let mut app_state = test_app_state();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
        let _ = tx.send(ArpCommand::TabGoForward { tab_id: None });
        process_arp_commands(&mut app_state);
        assert_eq!(app_state.pending_wry_actions.len(), 1);
    }

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_tab_reload() {
        let mut app_state = test_app_state();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
        let _ = tx.send(ArpCommand::TabReload { tab_id: None });
        process_arp_commands(&mut app_state);
        assert_eq!(app_state.pending_wry_actions.len(), 1);
    }

    #[cfg(feature = "arp")]
    #[test]
    fn process_arp_commands_clipboard_get() {
        let mut app_state = test_app_state();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        app_state.arp_cmd_receiver = Some(std::sync::Mutex::new(rx));
        let _ = tx.send(ArpCommand::ClipboardGet { request_id: 42 });
        process_arp_commands(&mut app_state);
    }

    // ─── load_default_adblock_rules ─────────────────────────────────────

    #[test]
    fn load_default_adblock_rules_loads_without_panic() {
        let mut adblocker = crate::net::adblock::AdBlocker::new();
        load_default_adblock_rules(&mut adblocker);
        assert!(adblocker.is_enabled());
    }

    #[test]
    fn load_default_adblock_rules_adds_blocked_domains() {
        let mut adblocker = crate::net::adblock::AdBlocker::new();
        load_default_adblock_rules(&mut adblocker);
        let test_url = url::Url::parse("https://doubleclick.net/track").unwrap();
        assert!(
            adblocker.should_block(&test_url, None, None),
            "doubleclick.net should be blocked after loading default rules"
        );
    }

    // ─── handle_pending_import ──────────────────────────────────────────

    #[test]
    fn handle_pending_import_none_does_nothing() {
        let mut app_state = test_app_state();
        assert!(app_state.pending_import.is_none());
        handle_pending_import(&mut app_state);
        assert!(app_state.ui.status_message.is_empty());
    }

    #[test]
    fn handle_pending_import_no_database_sets_message() {
        let mut app_state = test_app_state();
        app_state.pending_import = Some("firefox".into());
        app_state.db = None;
        handle_pending_import(&mut app_state);
        assert!(app_state.pending_import.is_none());
        assert!(app_state.ui.status_message.contains("No database"));
    }

    #[test]
    fn handle_pending_import_unknown_source_sets_message() {
        let mut app_state = test_app_state();
        app_state.pending_import = Some("safari".into());
        handle_pending_import(&mut app_state);
        assert!(app_state.pending_import.is_none());
        assert!(
            app_state
                .ui
                .status_message
                .contains("Unknown import source")
        );
    }

    // ─── poll_terminal_output ───────────────────────────────────────────

    #[test]
    fn poll_terminal_output_calls_tick_all_without_panic() {
        let mut terminal_manager = NativeTerminalManager::new();
        poll_terminal_output(&mut terminal_manager);
    }

    // ─── process_pending_wry_actions ────────────────────────────────────

    #[test]
    fn process_pending_wry_actions_none_app_state_does_nothing() {
        let mut app_state: Option<AppState> = None;
        let mut wry_panes = WryPaneManager::new();
        let mut offscreen_panes = OffscreenWebViewManager::new();
        let content_scripts = ContentScriptManager::new();
        process_pending_wry_actions(
            &mut app_state,
            &mut wry_panes,
            &mut offscreen_panes,
            &content_scripts,
        );
    }
}
