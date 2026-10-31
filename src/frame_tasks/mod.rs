use std::sync::Arc;

use tracing::{info, warn};
use uuid::Uuid;

#[cfg(feature = "mcp")]
use image::ImageEncoder;

use crate::app::{AppState, WryAction};
#[cfg(feature = "arp")]
use crate::arp::ArpCommand;
use crate::extensions::web_request::WebRequestInterceptorRegistry;
use crate::git::GitStatus;
#[cfg(feature = "mcp")]
use crate::mcp::{McpBridge, McpCommand};
use crate::offscreen_webview::OffscreenWebViewManager;
use crate::scripts::ContentScriptManager;
use crate::servo::{WryPaneManager, pump_gtk};
#[cfg(feature = "terminal")]
use crate::terminal::NativeTerminalManager;

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
    if !app_state.tabs.tab_display_dirty {
        return;
    }

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
    let active_id = app_state.wm.active_pane_id();
    if let Some(wry_pane) = wry_panes.get(&active_id) {
        mcp_bridge.update_state(wry_pane.url().as_str(), wry_pane.title());
        mcp_bridge.update_active_meta(
            app_state
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
    let active_id = app_state.wm.active_pane_id();
    if let Some(pane) = offscreen_panes.get(&active_id) {
        mcp_bridge.update_state(pane.url().as_str(), pane.title());
        mcp_bridge.update_active_meta(
            app_state
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
    #[cfg(feature = "lua")]
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

/// Wait for a oneshot JS result while pumping the GTK main loop.
///
/// Offscreen (Wayland) webviews deliver script callbacks on the GTK main
/// loop; a plain `blocking_recv()` would deadlock the very loop that must
/// run for the callback to fire. Pumping also covers wry (X11) delivery.
fn wait_for_js_result(
    mut rx: tokio::sync::oneshot::Receiver<String>,
    timeout: std::time::Duration,
) -> Option<String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match rx.try_recv() {
            Ok(v) => {
                // wry delivers evaluate_script results JSON-encoded
                // (js_value().to_json()): a JS string arrives as "\"...\"".
                // Decode the outer wrapper once for every callback tool.
                let decoded = serde_json::from_str::<serde_json::Value>(&v)
                    .ok()
                    .and_then(|val| val.as_str().map(str::to_string))
                    .unwrap_or(v);
                return Some(decoded);
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => return None,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        pump_gtk_loop();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Dispatch JS execution on a pane regardless of rendering backend: native
/// wry pane (X11) or offscreen pane (Wayland). Returns false when the pane
/// exists in neither map.
///
/// Before 2026-10-25 the MCP browsing tools only consulted `wry_panes`,
/// which left every read/interaction tool dead on Wayland ("no active
/// pane") — the default Linux session type.
fn dispatch_active_js_with_callback(
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

/// Fire-and-forget variant of [`dispatch_active_js_with_callback`].
fn dispatch_active_js(
    wry_panes: &WryPaneManager,
    offscreen_panes: &mut OffscreenWebViewManager,
    pane_id: &uuid::Uuid,
    js: &str,
) -> bool {
    dispatch_active_js_with_callback(wry_panes, offscreen_panes, pane_id, js, |_| {})
}

/// YP-MCP-NAV-EPOCH-001 §5: hold pump-waiting commands until every
/// navigation issued for the pane has finished loading (read-your-writes).
/// The settled epoch promotes when the pane stops reporting loading - the
/// same signal the frame loop uses to clear the loading flag. Bounded wait;
/// on timeout the command runs anyway (best effort, never a wedge).
fn wait_mcp_nav_settled(
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
    pane: &uuid::Uuid,
    budget: std::time::Duration,
) {
    let deadline = std::time::Instant::now() + budget;
    let mut saw_loading = false;
    let trace = crate::offscreen_webview::mcp_trace_enabled();
    if trace {
        eprintln!(
            "[mcp-trace] wait enter nav={} settled={}",
            app_state.mcp_nav_epoch.get(pane).copied().unwrap_or(0),
            app_state.mcp_settled_epoch.get(pane).copied().unwrap_or(0)
        );
    }
    loop {
        let nav = *app_state.mcp_nav_epoch.get(pane).unwrap_or(&0);
        let settled = *app_state.mcp_settled_epoch.get(pane).unwrap_or(&0);
        if nav <= settled {
            if trace {
                eprintln!("[mcp-trace] wait exit: settled (load committed)");
            }
            return;
        }
        // Live WebKitGTK load-state: accurate inside frame callbacks, unlike
        // the `loading` flag which lags until the frame loop drains events.
        #[cfg(target_os = "linux")]
        {
            let loading = offscreen_panes
                .get(pane)
                .map(|p| p.webview_is_loading())
                .unwrap_or(false);
            if loading {
                saw_loading = true;
            } else if saw_loading {
                if trace {
                    eprintln!("[mcp-trace] wait exit: observed commit -> promote");
                }
                app_state.mcp_settled_epoch.insert(*pane, nav);
                return;
            }
            // !loading && !saw: the navigation has not started yet (or the
            // pane fires no load events) - keep polling; the deadline
            // promotes best-effort so reads never wedge.
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = offscreen_panes;
            let nav = *app_state.mcp_nav_epoch.get(pane).unwrap_or(&0);
            app_state.mcp_settled_epoch.insert(*pane, nav);
            return;
        }
        if std::time::Instant::now() >= deadline {
            let nav = *app_state.mcp_nav_epoch.get(pane).unwrap_or(&0);
            app_state.mcp_settled_epoch.insert(*pane, nav);
            return;
        }
        pump_gtk_loop();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// Append one entry to the enterprise audit ring (Era-3): provenance per
/// dispatched or denied command. Cap 1000, oldest dropped.
#[cfg(feature = "mcp")]
fn audit_record(
    app_state: &mut AppState,
    session: Option<Uuid>,
    command: &McpCommand,
    pane: Uuid,
    denied: bool,
) {
    let epoch = app_state.mcp_settled_epoch.get(&pane).copied().unwrap_or(0);
    let entry = format!(
        "{} session={} tool={} class={} pane={} epoch={} outcome={}",
        chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f"),
        session
            .map(|s| s.to_string())
            .unwrap_or_else(|| "default".into()),
        command.tool_name(),
        crate::mcp::bridge::tool_class(command.tool_name()),
        &pane.to_string()[..8],
        epoch,
        if denied { "DENIED" } else { "ok" }
    );
    let mut log = app_state.audit_log.borrow_mut();
    if log.len() >= 1000 {
        log.pop_front();
    }
    log.push_back(entry);
}

#[cfg(feature = "mcp")]
pub fn process_mcp_commands(
    mcp_bridge: &McpBridge,
    wry_panes: &mut WryPaneManager,
    active_id: Uuid,
    app_state: &mut AppState,
    offscreen_panes: &mut OffscreenWebViewManager,
) {
    // MCP tools target web content, not the browser chrome overlay. When the
    // wm reports the chrome pane (nil UUID) as active - which happens on
    // Wayland during page loads - fall back to the first real content pane,
    // otherwise read/click tools silently operate on the chrome UI.
    let original_id = active_id;
    let active_id = if active_id == uuid::Uuid::nil() {
        offscreen_panes
            .iter()
            .map(|(id, _)| *id)
            .find(|id| *id != uuid::Uuid::nil())
            .unwrap_or(active_id)
    } else {
        active_id
    };
    let _ = original_id;
    // Workspace-restore guard (YP-WORKSPACE-002): a pending restore rebuilds
    // every pane (wry_panes.remove_all + fresh offscreen manager), so
    // commands must not execute against the dying pane set. Check BEFORE
    // polling: commands stay queued in the bridge channel and process once
    // the restore completes (the flag is consumed within a frame or two,
    // so this deferral is bounded).
    if app_state.pending_workspace_restore.is_some() {
        return;
    }
    let mcp_commands: Vec<(Option<Uuid>, McpCommand)> = mcp_bridge.poll_commands().collect();
    if mcp_commands.is_empty() {
        return;
    }

    for (session, command) in mcp_commands {
        // YP-MCP-SESSIONS-002 (Era-2 S27): capability gate. A scoped session
        // may only act on panes within its focus set (phi); violations fail
        // closed - ExecuteJs gets an explicit FORBIDDEN response, others are
        // dropped with a warn. Unscoped (default) sessions are unrestricted.
        if let Some(sid) = session {
            let caps = app_state
                .session_registry
                .lock()
                .ok()
                .and_then(|r| r.get(&sid).cloned());
            if let Some(caps) = caps
                && let Some(allowed) = &caps.panes
                && !allowed.contains(&active_id)
            {
                warn!(
                    "MCP: session {} FORBIDDEN on pane {} (phi scope)",
                    &sid.to_string()[..8],
                    &active_id.to_string()[..8]
                );
                audit_record(app_state, session, &command, active_id, true);
                if let McpCommand::ExecuteJs { response_tx, .. } = command {
                    let _ = response_tx.send(format!(
                        "FORBIDDEN: pane {} is outside this session's scope (session_scope to adjust)",
                        &active_id.to_string()[..8]
                    ));
                }
                continue;
            }
        }
        // YP-MCP-NAV-EPOCH-001 read-your-writes: every command settles behind
        // the pane's outstanding navigation before executing. Navigations
        // pass through instantly (nav <= settled when nothing is outstanding
        // from before) and advance the epoch themselves.
        audit_record(app_state, session, &command, active_id, false);
        wait_mcp_nav_settled(
            app_state,
            offscreen_panes,
            &active_id,
            std::time::Duration::from_secs(90),
        );
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
                                // Queue real webview creation: engines.create_pane
                                // builds a wry-engine pane which never renders on
                                // Wayland (phantom pane, all tools dead on it).
                                app_state
                                    .pending_mcp_pane_creations
                                    .push_back((new_id, parsed.clone()));
                                app_state
                                    .pending_wry_actions
                                    .push_back(WryAction::Navigate(parsed));
                                app_state.wm.set_active_pane(new_id);
                                // YP-MCP-NAV-EPOCH-001: advance the pane epoch.
                                *app_state.mcp_nav_epoch.entry(new_id).or_insert(0) += 1;
                                app_state.session.session_dirty = true;
                            }
                            Err(e) => {
                                warn!("MCP: failed to create new tab: {}", e);
                            }
                        }
                    } else if wry_panes.get(&active_id).is_none()
                        && offscreen_panes.get(&active_id).is_none()
                    {
                        // Phantom pane (wm entry without a webview - e.g. an
                        // MCP-created tab from a previous session or a crashed
                        // webview): a navigate cannot reach it. Attach a real
                        // webview pointed at the target URL instead.
                        warn!(
                            "MCP: navigating phantom pane {} - creating webview",
                            &active_id.to_string()[..8]
                        );
                        app_state
                            .pending_mcp_pane_creations
                            .push_back((active_id, parsed));
                        app_state.session.session_dirty = true;
                    } else if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
                        // X11 / GTK-window-embedded native pane.
                        info!("MCP: navigating to {}", url);
                        wry_pane.navigate(&parsed);
                        // Settle: pump until the webview's URL property
                        // reflects the target (navigation committed), so
                        // same-batch reads do not evaluate the outgoing
                        // document. Bounded at 15s.
                        let nav_deadline =
                            std::time::Instant::now() + std::time::Duration::from_secs(15);
                        while std::time::Instant::now() < nav_deadline {
                            crate::frame_tasks::pump_gtk_loop();
                            if wry_pane.url().as_str() == parsed.as_str() {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        }
                        // YP-MCP-NAV-EPOCH-001: advance the pane epoch.
                        *app_state.mcp_nav_epoch.entry(active_id).or_insert(0) += 1;
                        app_state.session.session_dirty = true;
                    } else if let Some(pane) = offscreen_panes.get_mut(&active_id) {
                        // Wayland: navigate the offscreen webview directly.
                        // Queueing a WryAction would execute it AFTER this
                        // batch, so reads issued in the same batch as the
                        // navigate raced the navigation and returned the
                        // outgoing document. Direct navigation + the
                        // dispatch settle-wait (pane.is_loading()) keeps
                        // same-batch reads correct.
                        if crate::offscreen_webview::mcp_trace_enabled() {
                            eprintln!(
                                "[mcp-trace] nav arm: offscreen pane {} -> {}",
                                &active_id.to_string()[..8],
                                parsed.as_str()
                            );
                        }
                        info!("MCP: navigating offscreen pane to {}", url);
                        // YP-MCP-NAV-EPOCH-001: navigate + deterministic
                        // settle (flush stale load events, then wait for
                        // THIS load's Started->Complete pair while pumping
                        // GTK). The flush is the same-batch staleness fix:
                        // a buffered LoadComplete from an earlier load used
                        // to satisfy the hold instantly, letting reads
                        // evaluate on the outgoing document. Bounded at 60s;
                        // slow pages fall through best-effort.
                        pane.navigate_settled(&parsed, std::time::Duration::from_secs(60));
                        // YP-MCP-NAV-EPOCH-001: advance the pane epoch and
                        // promote settled (the load committed or the budget
                        // expired - reads must never wedge on either path).
                        *app_state.mcp_nav_epoch.entry(active_id).or_insert(0) += 1;
                        let epoch_now = app_state
                            .mcp_nav_epoch
                            .get(&active_id)
                            .copied()
                            .unwrap_or(0);
                        app_state.mcp_settled_epoch.insert(active_id, epoch_now);
                        app_state.session.session_dirty = true;
                    } else {
                        warn!("MCP: navigate target pane vanished");
                    }
                } else {
                    warn!("MCP: invalid navigate URL: {}", url);
                }
            }
            McpCommand::ExecuteJs { code, response_tx } => {
                info!("MCP: executing JS ({} chars)", code.len());
                let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(response_tx)));
                let tx_for_cb = std::sync::Arc::clone(&tx);
                let dispatched = dispatch_active_js_with_callback(
                    wry_panes,
                    offscreen_panes,
                    &active_id,
                    &code,
                    move |result| {
                        // wry delivers evaluate_script results JSON-encoded
                        // (js_value().to_json()): a JS string arrives as
                        // "\"...\"". Decode that outer wrapper so consumers
                        // (get_text, wait_for, ...) see the raw value.
                        let decoded = serde_json::from_str::<serde_json::Value>(&result)
                            .ok()
                            .and_then(|v| v.as_str().map(str::to_string))
                            .unwrap_or(result);
                        if let Ok(mut guard) = tx_for_cb.lock()
                            && let Some(sender) = guard.take()
                        {
                            let _ = sender.send(decoded);
                        }
                    },
                );
                if !dispatched
                    && let Ok(mut guard) = tx.lock()
                    && let Some(sender) = guard.take()
                {
                    let _ = sender.send("Error: No active pane".to_string());
                }
            }
            McpCommand::GetActivePane { response_tx } => {
                let from_wry = wry_panes
                    .get(&active_id)
                    .map(|p| (p.url().as_str().to_string(), p.title().to_string()));
                let (url, title) = from_wry.unwrap_or_else(|| {
                    offscreen_panes
                        .get(&active_id)
                        .map(|p| (p.url().as_str().to_string(), p.title().to_string()))
                        .unwrap_or_default()
                });
                let _ = response_tx.send((url, title));
            }
            McpCommand::SessionScope {
                panes,
                tool_classes,
                response_tx,
            } => {
                // Era-2 S28 (YP-MCP-SESSIONS-002 / YP-SEC-CONFINEMENT-001):
                // narrowing-only capability update. The command's tag is the
                // authoritative session identity (assigned inside the TCP
                // accept loop) - a page cannot forge it, so the only paths
                // into this arm carry the caller's own tag and every write
                // must be a subset (structural taint: grants only shrink).
                if let Some(sid) = session {
                    let narrowed = crate::mcp::bridge::SessionCaps {
                        panes: panes.map(|v| v.into_iter().collect()),
                        tool_classes: tool_classes.map(|v| v.into_iter().collect()),
                    };
                    let result = app_state
                        .session_registry
                        .lock()
                        .map(|mut reg| {
                            let current = reg.get(&sid).cloned().unwrap_or_default();
                            if current.allows_narrowing_to(&narrowed) {
                                reg.insert(sid, narrowed.clone());
                                Ok::<_, String>(narrowed)
                            } else {
                                Err("REJECTED: session_scope can only narrow (monotone)".into())
                            }
                        })
                        .unwrap_or_else(|_| Err("registry poisoned".into()));
                    let _ = response_tx.send(match result {
                        Ok(caps) => format!(
                            "{{\"session\":\"{sid}\",\"panes\":{:?},\"tool_classes\":{:?}}}",
                            caps.panes.map(|p| p.into_iter().collect::<Vec<_>>()),
                            caps.tool_classes.map(|c| c.into_iter().collect::<Vec<_>>())
                        ),
                        Err(e) => format!("Error: {e}"),
                    });
                }
            }
            McpCommand::SessionInfo { response_tx } => {
                let caps = session
                    .and_then(|sid| {
                        app_state
                            .session_registry
                            .lock()
                            .ok()
                            .and_then(|r| r.get(&sid).cloned())
                    })
                    .unwrap_or_default();
                let _ = response_tx.send(format!(
                    "{{\"session\":\"{}\",\"scoped\":{},\"panes\":{:?},\"tool_classes\":{:?}}}",
                    session
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "default".into()),
                    session.is_some(),
                    caps.panes.map(|p| p.into_iter().collect::<Vec<_>>()),
                    caps.tool_classes.map(|c| c.into_iter().collect::<Vec<_>>())
                ));
            }
            McpCommand::WorkspaceCheckpoint { name, response_tx } => {
                // Era-2 S41 (AgentRewind environment-memory pattern): save the
                // live pane tree as a named checkpoint over the existing
                // workspace persistence.
                let url_of = |id: Uuid| -> Option<String> {
                    offscreen_panes
                        .get(&id)
                        .map(|p| p.url().to_string())
                        .or_else(|| wry_panes.get(&id).map(|p| p.url().to_string()))
                };
                let result = app_state
                    .wm
                    .to_workspace_data(url_of)
                    .map_err(|e| e.to_string())
                    .and_then(|data| {
                        app_state
                            .db
                            .as_ref()
                            .ok_or_else(|| "no database".to_string())
                            .and_then(|db| {
                                crate::db::workspaces::save_workspace(db, &name, &data)
                                    .map(|_| format!("checkpoint '{name}' saved"))
                                    .map_err(|e| e.to_string())
                            })
                    });
                let _ = response_tx.send(match result {
                    Ok(msg) => msg,
                    Err(e) => format!("Error: {e}"),
                });
            }
            McpCommand::WorkspaceDiff { name, response_tx } => {
                // Era-2 S43: saved checkpoint vs live tree, as flat URL sets.
                let url_of = |id: Uuid| -> Option<String> {
                    offscreen_panes
                        .get(&id)
                        .map(|p| p.url().to_string())
                        .or_else(|| wry_panes.get(&id).map(|p| p.url().to_string()))
                };
                let result = app_state
                    .wm
                    .to_workspace_data(url_of)
                    .map_err(|e| e.to_string())
                    .and_then(|live| {
                        app_state
                            .db
                            .as_ref()
                            .ok_or_else(|| "no database".to_string())
                            .and_then(|db| {
                                crate::db::workspaces::load_workspace(db, &name)
                                    .map_err(|e| e.to_string())
                                    .and_then(|saved| {
                                        match saved {
                                            None => Err(format!("no checkpoint '{name}'")),
                                            Some((_, data)) => {
                                                let saved_urls: std::collections::HashSet<String> =
                                                    crate::db::workspaces::collect_urls(
                                                        &data.tree,
                                                    )
                                                    .into_iter()
                                                    .collect();
                                                let live_urls: std::collections::HashSet<String> =
                                                    crate::db::workspaces::collect_urls(&live.tree)
                                                        .into_iter()
                                                        .collect();
                                                let added: Vec<&String> =
                                                    live_urls.difference(&saved_urls).collect();
                                                let removed: Vec<&String> =
                                                    saved_urls.difference(&live_urls).collect();
                                                Ok(format!(
                                                    "{{\"checkpoint\":\"{name}\",\"added\":{:?},\"removed\":{:?},\"same\":{}}}",
                                                    added, removed,
                                                    added.is_empty() && removed.is_empty()
                                                ))
                                            }
                                        }
                                    })
                            })
                    });
                let _ = response_tx.send(match result {
                    Ok(msg) => msg,
                    Err(e) => format!("Error: {e}"),
                });
            }
            McpCommand::WorkspaceList { response_tx } => {
                let result = app_state
                    .db
                    .as_ref()
                    .ok_or_else(|| "no database".to_string())
                    .and_then(|db| {
                        crate::db::workspaces::list_workspaces(db).map_err(|e| e.to_string())
                    })
                    .map(|list| {
                        list.iter()
                            .map(|w| format!("{} ({})", w.name, w.updated_at))
                            .collect::<Vec<_>>()
                            .join("\n")
                    });
                let _ = response_tx.send(match result {
                    Ok(msg) => {
                        if msg.is_empty() {
                            "No checkpoints.".into()
                        } else {
                            msg
                        }
                    }
                    Err(e) => format!("Error: {e}"),
                });
            }
            McpCommand::SetUserAgent { ua, response_tx } => {
                // Era-3 mobile lane: write the WebKit settings property, then
                // re-load the current page through the settle hold so the
                // epoch/read-your-writes discipline applies to the reload.
                let reloaded = offscreen_panes
                    .get_mut(&active_id)
                    .map(|p| {
                        let ok = p.set_user_agent(&ua);
                        if ok {
                            let current = p.url().clone();
                            p.navigate_settled(&current, std::time::Duration::from_secs(60));
                        }
                        ok
                    })
                    .unwrap_or(false);
                if reloaded {
                    *app_state.mcp_nav_epoch.entry(active_id).or_insert(0) += 1;
                    let epoch_now = app_state
                        .mcp_nav_epoch
                        .get(&active_id)
                        .copied()
                        .unwrap_or(0);
                    app_state.mcp_settled_epoch.insert(active_id, epoch_now);
                    app_state.session.session_dirty = true;
                }
                let _ = response_tx.send(if reloaded {
                    format!("User agent set and page reloaded: {ua}")
                } else {
                    "Error: could not set user agent (no webkit child)".into()
                });
            }
            McpCommand::ReadAudit { limit, response_tx } => {
                let log = app_state.audit_log.borrow();
                let skip = log.len().saturating_sub(limit);
                let lines: Vec<String> = log.iter().skip(skip).cloned().collect();
                let _ = response_tx.send(if lines.is_empty() {
                    "Audit log empty.".into()
                } else {
                    lines.join("\n")
                });
            }
            McpCommand::ListDownloads { response_tx } => {
                let rows: Vec<String> = app_state
                    .download_manager
                    .progress_all()
                    .iter()
                    .map(|d| {
                        format!(
                            "#{} {} {} {}/{} ({:.0}%) {:?}",
                            d.id,
                            d.state,
                            d.filename,
                            crate::downloads::DownloadProgress::format_bytes(d.received_bytes),
                            crate::downloads::DownloadProgress::format_bytes(d.total_bytes),
                            d.fraction * 100.0,
                            d.url
                        )
                    })
                    .collect();
                let _ = response_tx.send(if rows.is_empty() {
                    "No downloads.".into()
                } else {
                    rows.join("\n")
                });
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
                let pane_ids: Vec<Uuid> =
                    app_state.wm.panes_ref().iter().map(|(id, _)| *id).collect();
                let lines: Vec<String> = pane_ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        let marker = if *id == active { " [active]" } else { "" };
                        // wry pane (X11) first, offscreen pane (Wayland)
                        // fallback — offscreen panes report real state instead
                        // of a permanent "about:blank /(untitled)".
                        let (url, title) = wry_panes
                            .get(id)
                            .map(|p| (p.url().to_string(), p.title().to_string()))
                            .or_else(|| {
                                offscreen_panes
                                    .get(id)
                                    .map(|p| (p.url().as_str().to_string(), p.title().to_string()))
                            })
                            .unwrap_or_else(|| ("about:blank".into(), "(untitled)".into()));
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
                } else if wry_panes.get(&active_id).is_some() {
                    // Native wry pane (X11 / non-composited Wayland): Canvas
                    // API capture. The result arrives via the GTK main loop,
                    // so wait with the same pump used for JS results.
                    let buffer = std::sync::Arc::new(std::sync::Mutex::new(None));
                    if let Some(pane) = wry_panes.get(&active_id) {
                        pane.start_capture_screenshot(std::sync::Arc::clone(&buffer));
                    }
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    let mut out = "Error: screenshot capture timed out".into();
                    while std::time::Instant::now() < deadline {
                        if let Some(data) =
                            crate::servo::wry_engine::WryPane::pending_screenshot_result(&buffer)
                        {
                            out = data;
                            break;
                        }
                        pump_gtk_loop();
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                    if out.starts_with("ERROR:") {
                        warn!("Screenshot canvas capture failed: {out}");
                    }
                    out
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
                    #[cfg(feature = "terminal")]
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
            McpCommand::Scroll {
                delta_x,
                delta_y,
                response_tx,
            } => {
                let js = format!(
                    "window.scrollBy({{top: {}, left: {}, behavior: 'instant'}})",
                    delta_y, delta_x
                );
                let result = if dispatch_active_js(wry_panes, offscreen_panes, &active_id, &js) {
                    format!("Scrolled by ({delta_x}, {delta_y}) pixels")
                } else {
                    "Error: no active pane".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::NavigateBack { response_tx } => {
                let result = if dispatch_active_js(
                    wry_panes,
                    offscreen_panes,
                    &active_id,
                    "window.history.back()",
                ) {
                    "Navigated back".into()
                } else {
                    "Error: no active pane".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::NavigateForward { response_tx } => {
                let result = if dispatch_active_js(
                    wry_panes,
                    offscreen_panes,
                    &active_id,
                    "window.history.forward()",
                ) {
                    "Navigated forward".into()
                } else {
                    "Error: no active pane".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::SelectOption {
                selector,
                value,
                response_tx,
            } => {
                let js = format!(
                    r#"(() => {{
                        const sel = document.querySelector('{}');
                            if (!sel) return 'Error: select element not found';
                            sel.value = '{}';
                            sel.dispatchEvent(new Event('change', {{bubbles: true}}));
                            return 'Selected option: {}';
                        }})()"#,
                    selector.replace('\'', "\\'"),
                    value.replace('\'', "\\'"),
                    value.replace('\'', "\\'")
                );
                let (response_tx_inner, response_rx) = tokio::sync::oneshot::channel();
                let tx = std::sync::Mutex::new(Some(response_tx_inner));
                let dispatched = dispatch_active_js_with_callback(
                    wry_panes,
                    offscreen_panes,
                    &active_id,
                    &js,
                    move |result| {
                        if let Ok(mut guard) = tx.lock()
                            && let Some(sender) = guard.take()
                        {
                            let _ = sender.send(result);
                        }
                    },
                );
                let result = if dispatched {
                    match wait_for_js_result(response_rx, std::time::Duration::from_secs(10)) {
                        Some(r) => r,
                        None => "Error: select option timed out".into(),
                    }
                } else {
                    "Error: no active pane".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::ReadPageStructure { response_tx } => {
                let js = r#"(() => {
                    const tree = {headings: [], landmarks: [], links: [], buttons: [], forms: []};
                        document.querySelectorAll('h1,h2,h3,h4,h5,h6').forEach(h => {
                            tree.headings.push({level: parseInt(h.tagName[1]), text: h.innerText.trim().slice(0, 200)});
                        });
                        document.querySelectorAll('nav,main,aside,header,footer,section[aria-label],section[aria-labelledby]').forEach(el => {
                            tree.landmarks.push({role: el.tagName.toLowerCase(), label: (el.getAttribute('aria-label') || el.getAttribute('aria-labelledby') || '').trim()});
                        });
                        document.querySelectorAll('a[href]').forEach(a => {
                            const text = a.innerText.trim().slice(0, 100);
                            if (text) tree.links.push({text, href: a.href});
                        });
                        document.querySelectorAll('button,[role="button"]').forEach(b => {
                            const text = b.innerText.trim().slice(0, 100);
                            if (text) tree.buttons.push({text});
                        });
                        document.querySelectorAll('form').forEach(f => {
                            const inputs = [];
                            f.querySelectorAll('input,select,textarea').forEach(inp => {
                                inputs.push({type: inp.type || inp.tagName.toLowerCase(), name: inp.name || inp.id || ''});
                            });
                            tree.forms.push({action: f.action, inputs});
                        });
                        return JSON.stringify(tree);
                    })()"#;
                let (response_tx_inner, response_rx) = tokio::sync::oneshot::channel();
                let tx = std::sync::Mutex::new(Some(response_tx_inner));
                let dispatched = dispatch_active_js_with_callback(
                    wry_panes,
                    offscreen_panes,
                    &active_id,
                    js,
                    move |result| {
                        if let Ok(mut guard) = tx.lock()
                            && let Some(sender) = guard.take()
                        {
                            let _ = sender.send(result);
                        }
                    },
                );
                let result = if dispatched {
                    match wait_for_js_result(response_rx, std::time::Duration::from_secs(10)) {
                        Some(r) => r,
                        None => "Error: page structure read timed out".into(),
                    }
                } else {
                    "Error: no active pane".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::ExtractStructuredData { response_tx } => {
                let js = r#"(() => {
                    const data = {jsonLd: [], openGraph: {}, tables: []};
                        document.querySelectorAll('script[type="application/ld+json"]').forEach(s => {
                            try { data.jsonLd.push(JSON.parse(s.textContent)); } catch(e) {}
                        });
                        document.querySelectorAll('meta[property^="og:"]').forEach(m => {
                            const key = m.getAttribute('property').replace('og:', '');
                            data.openGraph[key] = m.getAttribute('content');
                        });
                        document.querySelectorAll('table').forEach((t, i) => {
                            if (i >= 5) return;
                            const headers = [];
                            t.querySelectorAll('th').forEach(th => headers.push(th.innerText.trim()));
                            const rows = [];
                            t.querySelectorAll('tbody tr').forEach(tr => {
                                const cells = [];
                                tr.querySelectorAll('td').forEach(td => cells.push(td.innerText.trim()));
                                if (cells.length) rows.push(cells);
                            });
                            data.tables.push({headers, rows: rows.slice(0, 50)});
                        });
                        return JSON.stringify(data);
                    })()"#;
                let (response_tx_inner, response_rx) = tokio::sync::oneshot::channel();
                let tx = std::sync::Mutex::new(Some(response_tx_inner));
                let dispatched = dispatch_active_js_with_callback(
                    wry_panes,
                    offscreen_panes,
                    &active_id,
                    js,
                    move |result| {
                        if let Ok(mut guard) = tx.lock()
                            && let Some(sender) = guard.take()
                        {
                            let _ = sender.send(result);
                        }
                    },
                );
                let result = if dispatched {
                    match wait_for_js_result(response_rx, std::time::Duration::from_secs(10)) {
                        Some(r) => r,
                        None => "Error: structured data extraction timed out".into(),
                    }
                } else {
                    "Error: no active pane".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::SummarizePage { response_tx } => {
                let result = {
                    let (text_tx, text_rx) = tokio::sync::oneshot::channel();
                    let text_tx_mutex = std::sync::Mutex::new(Some(text_tx));
                    let dispatched = dispatch_active_js_with_callback(
                        wry_panes,
                        offscreen_panes,
                        &active_id,
                        "document.body.innerText",
                        move |result| {
                            if let Ok(mut guard) = text_tx_mutex.lock()
                                && let Some(sender) = guard.take()
                            {
                                let _ = sender.send(result);
                            }
                        },
                    );

                    if dispatched {
                        match wait_for_js_result(text_rx, std::time::Duration::from_secs(120)) {
                            Some(text) if !text.is_empty() && text != "null" => {
                                let config = crate::config::Config::load();
                                if config.llm_base_url.is_empty() {
                                    "Error: LLM not configured (llm_base_url is empty)".into()
                                } else {
                                    let client =
                                        crate::llm::OllamaClient::new(crate::llm::OllamaConfig {
                                            base_url: config.llm_base_url,
                                            default_model: config.llm_model,
                                            timeout_secs: config.llm_timeout_secs,
                                            temperature: config.llm_temperature,
                                            max_tokens: config.llm_max_tokens,
                                        });
                                    match client.summarize(&text) {
                                        Ok(summary) => format!("## Summary\n\n{summary}"),
                                        Err(e) => format!("Error: LLM summarization failed: {e}"),
                                    }
                                }
                            }
                            _ => "Error: could not extract page text".into(),
                        }
                    } else {
                        "Error: no active pane".into()
                    }
                };
                let _ = response_tx.send(result);
            }
            McpCommand::TranslatePage {
                target_lang,
                response_tx,
            } => {
                let result = {
                    let (text_tx, text_rx) = tokio::sync::oneshot::channel();
                    let text_tx_mutex = std::sync::Mutex::new(Some(text_tx));
                    let dispatched = dispatch_active_js_with_callback(
                        wry_panes,
                        offscreen_panes,
                        &active_id,
                        "document.body.innerText",
                        move |result| {
                            if let Ok(mut guard) = text_tx_mutex.lock()
                                && let Some(sender) = guard.take()
                            {
                                let _ = sender.send(result);
                            }
                        },
                    );

                    if dispatched {
                        match wait_for_js_result(text_rx, std::time::Duration::from_secs(120)) {
                            Some(text) if !text.is_empty() && text != "null" => {
                                let config = crate::config::Config::load();
                                if config.llm_base_url.is_empty() {
                                    "Error: LLM not configured (llm_base_url is empty)".into()
                                } else {
                                    let client =
                                        crate::llm::OllamaClient::new(crate::llm::OllamaConfig {
                                            base_url: config.llm_base_url,
                                            default_model: config.llm_model,
                                            timeout_secs: config.llm_timeout_secs,
                                            temperature: config.llm_temperature,
                                            max_tokens: config.llm_max_tokens,
                                        });
                                    match client.translate(&text, &target_lang) {
                                        Ok(translated) => {
                                            format!(
                                                "## Translation ({target_lang})\n\n{translated}"
                                            )
                                        }
                                        Err(e) => format!("Error: LLM translation failed: {e}"),
                                    }
                                }
                            }
                            _ => "Error: could not extract page text".into(),
                        }
                    } else {
                        "Error: no active pane".into()
                    }
                };
                let _ = response_tx.send(result);
            }
            McpCommand::AnalyzePage { response_tx } => {
                let result = {
                    let (text_tx, text_rx) = tokio::sync::oneshot::channel();
                    let text_tx_mutex = std::sync::Mutex::new(Some(text_tx));
                    let dispatched = dispatch_active_js_with_callback(
                        wry_panes,
                        offscreen_panes,
                        &active_id,
                        "document.body.innerText",
                        move |result| {
                            if let Ok(mut guard) = text_tx_mutex.lock()
                                && let Some(sender) = guard.take()
                            {
                                let _ = sender.send(result);
                            }
                        },
                    );

                    if dispatched {
                        match wait_for_js_result(text_rx, std::time::Duration::from_secs(120)) {
                            Some(text) if !text.is_empty() && text != "null" => {
                                let config = crate::config::Config::load();
                                if config.llm_base_url.is_empty() {
                                    "Error: LLM not configured (llm_base_url is empty)".into()
                                } else {
                                    let client =
                                        crate::llm::OllamaClient::new(crate::llm::OllamaConfig {
                                            base_url: config.llm_base_url,
                                            default_model: config.llm_model,
                                            timeout_secs: config.llm_timeout_secs,
                                            temperature: config.llm_temperature,
                                            max_tokens: config.llm_max_tokens,
                                        });
                                    match client.analyze(&text) {
                                        Ok(analysis) => format!("## Analysis\n\n{analysis}"),
                                        Err(e) => format!("Error: LLM analysis failed: {e}"),
                                    }
                                }
                            }
                            _ => "Error: could not extract page text".into(),
                        }
                    } else {
                        "Error: no active pane".into()
                    }
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
    #[cfg(feature = "terminal")]
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
#[cfg(feature = "terminal")]
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
pub fn spawn_mcp_server(
    mcp_bridge: &McpBridge,
    registry: crate::mcp::bridge::SessionRegistry,
    policy: &str,
) {
    use crate::mcp::tools;
    let mcp_state = mcp_bridge.state.clone();
    let mcp_command_tx = mcp_bridge.command_tx.clone();

    // Policy profile (Era-3 S38): the default ceiling for NEW sessions.
    let initial_caps = match policy {
        "strict" => Some(crate::mcp::bridge::SessionCaps {
            panes: None,
            tool_classes: Some(std::collections::HashSet::from(["read".to_string()])),
        }),
        "balanced" => Some(crate::mcp::bridge::SessionCaps {
            panes: None,
            tool_classes: Some(std::collections::HashSet::from([
                "read".to_string(),
                "mutate".to_string(),
            ])),
        }),
        _ => None, // permissive (default)
    };

    // TCP bridge (Era-2 S27): per-connection session tool sets are built
    // inside the accept loop from this shared state; the registry carries
    // each session's capability set.
    crate::mcp::tcp_bridge::start_tcp_mcp_server(
        mcp_state.clone(),
        mcp_command_tx.clone(),
        registry,
        initial_caps,
    );

    // A separate tool set backs the in-process stdio transport; both transports
    // therefore behave identically without sharing mutable state.
    let stdio_tools = tools::create_tools(mcp_state, mcp_bridge.command_tx.clone());
    let mut mcp_server = crate::mcp::McpServer::new();
    for tool in stdio_tools {
        mcp_server.register_tool(tool);
    }

    let transport = crate::mcp::McpTransport::new(mcp_server);
    info!(
        "MCP server starting (stdio + TCP bridge on port {})",
        crate::mcp::MCP_TCP_PORT
    );
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
