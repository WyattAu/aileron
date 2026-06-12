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
                let (url, title) = wry_panes
                    .get(&active_id)
                    .map(|p| (p.url().as_str().to_string(), p.title().to_string()))
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
                let pane_ids: Vec<Uuid> =
                    app_state.wm.panes_ref().iter().map(|(id, _)| *id).collect();
                let lines: Vec<String> = pane_ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        let marker = if *id == active { " [active]" } else { "" };
                        let (url, title) = wry_panes
                            .get(id)
                            .map(|p| (p.url().to_string(), p.title().to_string()))
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
