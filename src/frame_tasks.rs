use tracing::{info, warn};
use uuid::Uuid;

use aileron::app::{AppState, WryAction};
use aileron::git::GitStatus;
use aileron::mcp::{McpBridge, McpCommand};
use aileron::scripts::ContentScriptManager;
use aileron::servo::{pump_gtk, WryEvent, WryPaneManager};
use aileron::terminal::TerminalManager;

pub fn poll_git_status(git_status: &mut GitStatus, last_git_poll: &mut std::time::Instant) {
    if last_git_poll.elapsed().as_secs() >= 1 {
        *git_status = GitStatus::for_dir(std::path::Path::new("."));
        *last_git_poll = std::time::Instant::now();
    }
}

pub fn auto_save_workspace(app_state: &mut AppState, wry_panes: &WryPaneManager) {
    if !app_state.config.auto_save {
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
            }
            Err(e) => {
                tracing::warn!("Auto-save failed: {}", e);
            }
        }
    }
}

pub fn process_wry_events(
    app_state: &mut AppState,
    wry_panes: &mut WryPaneManager,
    content_scripts: &ContentScriptManager,
    mcp_bridge: &mut McpBridge,
) {
    let wry_events = wry_panes.poll_all_events();
    for event in wry_events {
        match event {
            WryEvent::LoadComplete { pane_id, url, .. } => {
                if let Ok(parsed) = url::Url::parse(&url) {
                    app_state.record_visit(&parsed, &url);
                }
                app_state.status_message = format!("Loaded: {}", &url[..url.len().min(60)]);

                if !url.starts_with("aileron://") {
                    let matching = content_scripts.scripts_for_url(&url);
                    for script in matching {
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            info!(
                                "Injecting content script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            wry_pane.execute_js(&script.js_code);
                        }
                    }
                    if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                        wry_pane.execute_js(aileron::servo::NETWORK_MONITOR_JS);
                        wry_pane.execute_js(aileron::servo::CONSOLE_CAPTURE_JS);
                        wry_pane.execute_js(
                            "setTimeout(function() { \
                                if (window._aileron_scroll_pos) { \
                                    window.scrollTo(window._aileron_scroll_pos.x, window._aileron_scroll_pos.y); \
                                } \
                            }, 100);"
                        );
                    }
                }
            }
            WryEvent::LoadStarted { url, .. } => {
                app_state.status_message = format!("Loading: {}...", &url[..url.len().min(40)]);
            }
            WryEvent::TitleChanged { title, .. } => {
                app_state.status_message = format!("{}", &title[..title.len().min(60)]);
            }
            WryEvent::DownloadStarted { url, filename, .. } => {
                let short_url = if url.len() > 40 { &url[..37] } else { &url };
                app_state.status_message = format!("Downloading: {} ({})", filename, short_url);
                info!("Download started: {} from {}", filename, url);
                if let Some(db) = app_state.db.as_ref() {
                    if let Some(downloads_dir) = directories::UserDirs::new()
                        .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
                    {
                        let dest = downloads_dir.join(&filename);
                        if let Err(e) = aileron::db::downloads::record_download(
                            db,
                            &url,
                            &filename,
                            &dest.to_string_lossy(),
                        ) {
                            warn!("Failed to record download: {}", e);
                        }
                    }
                }
            }
            WryEvent::OpenFile { path } => {
                let _ = std::process::Command::new("xdg-open")
                    .arg(&path)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                app_state.status_message = format!("Opened: {}", path);
            }
        }
    }

    let active_id = app_state.wm.active_pane_id();
    if let Some(wry_pane) = wry_panes.get(&active_id) {
        mcp_bridge.update_state(wry_pane.url().as_str(), wry_pane.title());
    }
}

pub fn process_pending_wry_actions(
    app_state: &mut Option<AppState>,
    wry_panes: &mut WryPaneManager,
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
) {
    let mcp_commands: Vec<McpCommand> = mcp_bridge.poll_commands().collect();

    for command in mcp_commands {
        match command {
            McpCommand::Navigate { url } => {
                if let Ok(parsed) = url::Url::parse(&url) {
                    if let Some(wry_pane) = wry_panes.get_mut(&active_id) {
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
                    wry_pane.execute_js_with_callback(&code, move |result| {
                        let _ = response_tx.send(result);
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
        }
    }
}

pub fn handle_pending_tab_close(app_state: &mut AppState, close_id: Uuid) {
    app_state.wm.set_active_pane(close_id);
    let _ = app_state.wm.close(close_id);
    app_state.engines.remove_pane(&close_id);
    app_state.terminal_pane_ids.remove(&close_id);
    app_state.status_message = "Pane closed".into();
}

pub fn poll_terminal_output(terminal_manager: &mut TerminalManager, wry_panes: &WryPaneManager) {
    let term_ids: Vec<Uuid> = terminal_manager.terminal_pane_ids();
    terminal_manager.poll_input();
    for tid in &term_ids {
        if let Some(encoded) = terminal_manager.flush_output(tid) {
            if let Some(wry_pane) = wry_panes.get(tid) {
                let js = format!("_terminalWrite('{}')", encoded);
                wry_pane.execute_js(&js);
            }
        }
    }
}

pub fn process_terminal_resizes(
    terminal_manager: &mut TerminalManager,
    rx: &mut std::sync::mpsc::Receiver<(Uuid, u16, u16)>,
) {
    while let Ok((pane_id, cols, rows)) = rx.try_recv() {
        terminal_manager.resize(&pane_id, cols, rows);
    }
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
        if let Err(e) = transport.run_stdio() {
            warn!("MCP server error: {}", e);
        }
    });
}
