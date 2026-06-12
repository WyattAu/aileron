//! Shared event processing logic for both Wry and Offscreen webview managers.
//!
//! This module eliminates the ~660 lines of duplication between
//! `process_wry_events_inner` and `process_offscreen_events_inner` by
//! abstracting over the pane manager type via traits.

use std::sync::Arc;

use tracing::{info, warn};

use crate::app::AppState;
use crate::extensions::web_request::WebRequestInterceptorRegistry;
use crate::scripts::{ContentScriptManager, RunAt};
use crate::servo::{WryEvent, wry_engine::WryPane};
use uuid::Uuid;

use super::ipc;
use super::ipc::ExecuteJs;

/// Trait abstracting over pane types for event processing.
///
/// Both `WryPane` and `OffscreenWebView` implement this trait.
/// The key difference: `mark_dirty()` is a no-op for WryPane (Wry re-renders
/// automatically) but real for OffscreenWebView (must signal frame invalidation).
pub trait EventPane: ipc::ExecuteJs {
    fn navigate(&mut self, url: &url::Url);
    fn url(&self) -> &url::Url;
    fn title(&self) -> &str;
    fn mark_dirty(&mut self);
    fn suppress_context_menu(&self);
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
    app_state: &mut AppState,
    manager: &mut M,
    content_scripts: &ContentScriptManager,
    adblocker: &crate::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
    events: Vec<WryEvent>,
) -> Vec<(Uuid, String)> {
    let mut ipc_messages = Vec::new();
    for event in events {
        let pane_id = pane_id_from_event(&event);

        match event {
            WryEvent::LoadComplete { url, .. } => {
                app_state.session.session_dirty = true;
                app_state.tabs.tab_display_dirty = true;
                app_state.cache.pane_count_dirty = true;
                if let Ok(parsed) = url::Url::parse(&url) {
                    app_state.record_visit(&parsed, &url);
                }

                // Sync tab URL/title
                let title = manager
                    .get(&pane_id)
                    .map(|p| p.title().to_string())
                    .unwrap_or_default();
                super::sync_tab_url_title(app_state, pane_id, &url, &title);

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

                if let Some(pane) = manager.get_mut(&pane_id) {
                    pane.mark_dirty();
                }

                if !url.starts_with("aileron://") {
                    app_state.load_scroll_marks_for_pane(pane_id, &url);

                    // Inject DocumentEnd scripts
                    let end_matching = content_scripts.scripts_for_url(&url, RunAt::DocumentEnd);
                    for script in end_matching {
                        let key = format!("userscript:{}", script.name);
                        if app_state.is_script_injected(pane_id, &key) {
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
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            ipc::inject_extension_shim_and_script(
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
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            info!(
                                "Injecting content script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            pane.execute_js_code(&script.js_code);
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
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            ipc::inject_extension_shim_and_script(
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
                    if let Some(pane) = manager.get_mut(&pane_id) {
                        pane.execute_js_code(crate::servo::NETWORK_MONITOR_JS);
                        pane.execute_js_code(crate::servo::CONSOLE_CAPTURE_JS);
                        #[cfg(feature = "passwords")]
                        pane.execute_js_code(
                            crate::passwords::bitwarden::BitwardenClient::form_submit_observer_js(),
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
                            crate::passwords::bitwarden::BitwardenClient::form_detect_report_js()
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

                    if let Some(ref css) = app_state.config.custom_css
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
                    if let Some(ref db) = app_state.db
                        && let Ok(settings) =
                            crate::db::site_settings::get_site_settings_for_url(db, &url)
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
                        if app_state.is_script_injected(pane_id, &key) {
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
                        if let Some(pane) = manager.get_mut(&pane_id) {
                            ipc::inject_extension_shim_and_script(
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
                let url = manager
                    .get(&pane_id)
                    .map(|p| p.url().to_string())
                    .unwrap_or_default();
                super::sync_tab_url_title(app_state, pane_id, &url, &title);
            }
            WryEvent::DownloadStarted { url, filename, .. } => {
                let dl_id = app_state
                    .download_manager
                    .start(url.as_str(), Some(filename.as_str()));
                let short_url = if url.len() > 40 { &url[..37] } else { &url };
                app_state.ui.status_message =
                    format!("Download #{dl_id}: {filename} ({short_url})");
                info!("Download #{} started: {} from {}", dl_id, filename, url);
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
                let _ = open::that(&path);
                app_state.ui.status_message = format!("Opened: {path}");
            }
            WryEvent::HttpsUpgraded { to, .. } => {
                app_state.ui.status_message = format!("HTTPS upgrade: {to}");
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
