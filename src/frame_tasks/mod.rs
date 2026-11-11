use std::sync::Arc;

use tracing::warn;

use uuid::Uuid;

#[cfg(feature = "mcp")]
use crate::app::{AppState, WryAction};
#[cfg(feature = "arp")]
use crate::arp::ArpCommand;
use crate::extensions::web_request::WebRequestInterceptorRegistry;
use crate::git::GitStatus;
#[cfg(feature = "mcp")]
use crate::mcp::McpBridge;
use crate::offscreen_webview::OffscreenWebViewManager;
use crate::scripts::ContentScriptManager;
#[cfg(feature = "terminal")]
#[cfg(feature = "terminal")]
use crate::terminal::NativeTerminalManager;
use aileron_engine::servo::WryPaneManager;

mod event_proc;
mod ipc;
#[cfg(test)]
mod tests;

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

    if !app_state.core.config.auto_save {
        return;
    }
    if !app_state.core.session.session_dirty {
        return;
    }
    let interval = std::time::Duration::from_secs(app_state.core.config.auto_save_interval);
    if app_state.core.session.last_auto_save.elapsed() < interval {
        return;
    }
    app_state.core.session.last_auto_save = std::time::Instant::now();

    let pane_urls: std::collections::HashMap<Uuid, String> = wry_panes
        .pane_ids()
        .into_iter()
        .filter_map(|id| wry_panes.url_for(&id).map(|url| (id, url.to_string())))
        .collect();

    if !pane_urls.is_empty() {
        match app_state.save_workspace_with_urls("_autosave", &pane_urls) {
            Ok(()) => {
                tracing::info!("Auto-saved workspace ({} panes)", pane_urls.len());
                if let Some(ref conn) = app_state.core.db {
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
/// Skips serialization entirely when tab state has not changed since the last push
/// (tracked via `tab_display_dirty`). This eliminates per-frame JSON allocation
/// and async mutex contention on the ~99.9% of frames where nothing changed.
#[cfg(feature = "arp")]
pub fn push_tabs_to_arp(app_state: &AppState, wry_panes: &WryPaneManager) {
    let server = match &app_state.arp_server {
        Some(s) if s.is_running() => s,
        _ => return,
    };

    // Tab state changes only on user actions (navigate, create, close, pin, mute).
    // The tab_display_dirty flag is set by the same events that change ARP state.
    // On clean frames, skip the entire JSON construction + async lock.
    if !app_state.headed().tabs.tab_display_dirty {
        return;
    }

    let active_id = app_state.core.wm.active_pane_id();
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
                "muted": app_state.headed().tabs.muted_pane_ids.contains(id),
                "pinned": app_state.headed().tabs.pinned_pane_ids.contains(id),
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
                let active = app_state.core.wm.active_pane_id();
                match app_state
                    .core
                    .wm
                    .split(active, crate::wm::SplitDirection::Vertical, 0.5)
                {
                    Ok(new_id) => {
                        let target_url = url
                            .and_then(|u| url::Url::parse(&u).ok())
                            .unwrap_or_else(|| url::Url::parse("aileron://newtab").unwrap());
                        app_state.core.engines.create_pane(new_id, target_url, None);
                        app_state.core.session.session_dirty = true;
                    }
                    Err(e) => {
                        warn!(target: "arp", "Tab create failed: {}", e);
                    }
                }
            }
            ArpCommand::TabNavigate { tab_id: _, url } => match url::Url::parse(&url) {
                Ok(parsed) => {
                    app_state
                        .core
                        .pending_wry_actions
                        .push_back(WryAction::Navigate(parsed));
                    app_state.core.session.session_dirty = true;
                }
                Err(e) => {
                    warn!(target: "arp", "Tab navigate invalid URL: {}", e);
                }
            },
            ArpCommand::TabClose { tab_id } => {
                let target = tab_id.unwrap_or_else(|| app_state.core.wm.active_pane_id());
                match app_state.core.wm.close(target) {
                    Ok(_next) => {
                        app_state.core.session.session_dirty = true;
                    }
                    Err(e) => {
                        warn!(target: "arp", "Tab close failed: {}", e);
                    }
                }
            }
            ArpCommand::TabActivate { tab_id } => {
                app_state.core.wm.set_active_pane(tab_id);
            }
            ArpCommand::TabGoBack { tab_id: _ } => {
                app_state
                    .core
                    .pending_wry_actions
                    .push_back(WryAction::Back);
            }
            ArpCommand::TabGoForward { tab_id: _ } => {
                app_state
                    .core
                    .pending_wry_actions
                    .push_back(WryAction::Forward);
            }
            ArpCommand::TabReload { tab_id: _ } => {
                app_state
                    .core
                    .pending_wry_actions
                    .push_back(WryAction::Reload);
            }
            ArpCommand::ClipboardSet { text } => {
                app_state
                    .core
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
                        .core
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
    let ipc_messages = event_proc::process_events(
        app_state,
        wry_panes,
        content_scripts,
        adblocker,
        interceptor_registry,
        wry_events,
    );
    for (pane_id, message) in ipc_messages {
        ipc::handle_ipc_message(app_state, wry_panes, pane_id, &message);
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
    let active_id = app_state.core.wm.active_pane_id();
    if let Some(wry_pane) = wry_panes.get(&active_id) {
        mcp_bridge.update_state(wry_pane.url().as_str(), wry_pane.title());
        mcp_bridge.update_active_meta(
            app_state
                .core
                .mcp_settled_epoch
                .get(&active_id)
                .copied()
                .unwrap_or(0),
            false,
        );
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

/// Update the active tab's URL/title in the BSP tree pane's TabList
/// to match the webview's current state. This keeps tab data in sync
/// so that tab switching navigates to the correct URL.
fn sync_tab_url_title(app_state: &mut AppState, pane_id: uuid::Uuid, url: &str, title: &str) {
    if let Some(pane) = app_state
        .core
        .wm
        .root_mut()
        .and_then(|root| crate::wm::BspTree::find_pane_mut(root, pane_id))
    {
        let active_tab = pane.tabs.active_mut();
        if let Ok(parsed) = url::Url::parse(url) {
            active_tab.url = parsed;
        }
        active_tab.title = title.to_string();
    }
}

fn process_offscreen_events_inner(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
    content_scripts: &ContentScriptManager,
    adblocker: &crate::net::adblock::AdBlocker,
    interceptor_registry: &Arc<WebRequestInterceptorRegistry>,
) {
    let events = offscreen_panes.drain_all_events_flat();
    let ipc_messages = event_proc::process_events(
        app_state,
        offscreen_panes,
        content_scripts,
        adblocker,
        interceptor_registry,
        events,
    );
    for (pane_id, message) in ipc_messages {
        ipc::handle_ipc_message_offscreen(app_state, offscreen_panes, pane_id, &message);
    }
}

#[cfg(feature = "mcp")]
pub fn process_offscreen_events(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
    content_scripts: &ContentScriptManager,
    mcp_bridge: &McpBridge,
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
    // Mirror the wry-pane path (process_wry_events): push the active pane's
    // url/title into the shared MCP state so read_active_pane and friends see
    // offscreen panes. The headed runtime masked this via its wry panes; the
    // agent runtime (offscreen-only) has no wry panes to mask it.
    let active_id = app_state.core.wm.active_pane_id();
    if let Some(pane) = offscreen_panes.get(&active_id) {
        mcp_bridge.update_state(pane.url().as_str(), pane.title());
        mcp_bridge.update_active_meta(
            app_state
                .core
                .mcp_settled_epoch
                .get(&active_id)
                .copied()
                .unwrap_or(0),
            pane.is_loading(),
        );
    }
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
/// A pane is considered crashed if it has been loading for >60 seconds
/// with no activity (no events, no frame captures). HTTPS pages with DNS,
/// TLS, and redirects can take 15-20 seconds, so the timeout must be
/// generous. Successful frame capture resets the activity timer.
pub fn check_offscreen_crashes(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
) {
    let crash_timeout = std::time::Duration::from_secs(60);

    for (pane_id, pane) in offscreen_panes.iter_mut() {
        if pane.is_crashed(crash_timeout) && !app_state.headed_mut().crash.webview_crash_detected {
            let url = pane.url().to_string();
            warn!(
                "WebView crash detected in pane {}: stalled while loading {}",
                &pane_id.to_string()[..8],
                &url[..url.len().min(80)]
            );
            app_state.headed_mut().crash.webview_crash_detected = true;
            app_state.headed_mut().crash.crashed_pane_url = Some(url);
            app_state.headed_mut().crash.crashed_pane_id = Some(*pane_id);
            app_state.headed_mut().ui.status_message =
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
    #[cfg(feature = "lua")]
    if let Some(state) = app_state {
        state.drain_lua_navigations();
    }
    let (pending_actions, active_id) = {
        let app_state = match app_state {
            Some(s) => s,
            None => return,
        };
        let actions: Vec<WryAction> = app_state.core.pending_wry_actions.drain(..).collect();
        let id = app_state.core.wm.active_pane_id();
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
                app_state.headed_mut().ui.status_message = format!("Action failed: {e}");
            }
        }
    }
}

/// Wait for a oneshot JS result while pumping the GTK main loop.
///
/// Offscreen (Wayland) webviews deliver script callbacks on the GTK main
/// loop; a plain `blocking_recv()` would deadlock the very loop that must
/// run for the callback to fire. Pumping also covers wry (X11) delivery.
/// Dispatch JS execution on a pane regardless of rendering backend: native
/// wry pane (X11) or offscreen pane (Wayland). Returns false when the pane
/// exists in neither map.
///
/// Before 2026-10-25 the MCP browsing tools only consulted `wry_panes`,
/// which left every read/interaction tool dead on Wayland ("no active
/// pane") — the default Linux session type.
pub(crate) fn dispatch_active_js_with_callback(
    wry_panes: &WryPaneManager,
    offscreen_panes: &mut OffscreenWebViewManager,
    pane_id: &uuid::Uuid,
    js: &str,
    callback: impl Fn(String) + Send + 'static,
) -> bool {
    // Offscreen (Wayland) reads must not race a pending navigation: JS fired
    // mid-load evaluates on the outgoing document and returns stale/empty
    // content. Wait (while pumping) until the pane stops loading, plus a
    // short settle so the new document's initial layout runs.
    if wry_panes.get(pane_id).is_none() {
        // 60s: heavy pages (en.wikipedia et al) under machine load have taken
        // longer than 15s to commit; a too-short window made same-batch reads
        // return the outgoing document.
        let settle_deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let loading = offscreen_panes
                .get(pane_id)
                .map(|p| p.is_loading())
                .unwrap_or(false);
            if !loading || std::time::Instant::now() >= settle_deadline {
                break;
            }
            pump_gtk_loop();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let settle_end = std::time::Instant::now() + std::time::Duration::from_millis(250);
        while std::time::Instant::now() < settle_end {
            pump_gtk_loop();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    if crate::offscreen_webview::mcp_trace_enabled() {
        eprintln!(
            "[mcp-trace] dispatch pane {} wry={} offscreen=yes",
            &pane_id.to_string()[..8],
            wry_panes.get(pane_id).is_some()
        );
    }
    if let Some(pane) = wry_panes.get(pane_id) {
        pane.execute_js_with_callback(js, callback);
        true
    } else if let Some(pane) = offscreen_panes.get_mut(pane_id) {
        pane.execute_js_with_callback(js, callback);
        true
    } else {
        false
    }
}

pub fn handle_pending_tab_close(app_state: &mut AppState, close_id: Uuid) {
    app_state.core.wm.set_active_pane(close_id);
    if let Err(e) = app_state.core.wm.close(close_id) {
        warn!(%e, "Failed to close pending tab");
    }
    app_state.core.engines.remove_pane(&close_id);
    #[cfg(feature = "terminal")]
    app_state.core.terminal_pane_ids.remove(&close_id);
    app_state.update_a11y("Pane closed");
}

/// Handle pending bookmark import (Firefox or Chrome).
pub fn handle_pending_import(app_state: &mut AppState) {
    let source = match app_state.headed_mut().pending_import.take() {
        Some(s) => s,
        None => return,
    };
    if let Some(db) = app_state.core.db.as_ref() {
        let msg = match source.as_str() {
            "firefox" => crate::app::cmd::import::import_firefox(db),
            "chrome" => crate::app::cmd::import::import_chrome(db),
            _ => {
                app_state.headed_mut().ui.status_message =
                    format!("Unknown import source: {source}");
                return;
            }
        };
        app_state.headed_mut().ui.status_message = msg;
    } else {
        app_state.headed_mut().ui.status_message = "No database available for import.".into();
    }
}

/// Poll native terminals for new output and feed VT parser.
#[cfg(feature = "terminal")]
pub fn poll_terminal_output(terminal_manager: &mut NativeTerminalManager) {
    terminal_manager.tick_all();
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

/// S4 stage 2b: moved to aileron-core `agent` (all-core types).
pub use aileron_core::agent::spawn_mcp_server;

/// S4 stage 2c step 3: the MCP dispatch moved to aileron-core `agent`
/// (PaneOps seam). Re-exported for call-site compatibility.
pub use aileron_core::agent::{audit_record, process_mcp_commands, wait_mcp_nav_settled};

/// S4 stage 3: the GTK pump moved to aileron-engine `pump`.
pub use aileron_engine::pump::pump_gtk_loop;
