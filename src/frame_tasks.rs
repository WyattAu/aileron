use tracing::{info, warn};
use uuid::Uuid;

use aileron::app::{AppState, WryAction};
use aileron::git::GitStatus;
use aileron::mcp::{McpBridge, McpCommand};
use aileron::scripts::{ContentScriptManager, RunAt};
use aileron::offscreen_webview::OffscreenWebViewManager;
use aileron::servo::{pump_gtk, WryEvent, WryPaneManager};
use aileron::terminal::NativeTerminalManager;

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
    adblocker: &aileron::net::adblock::AdBlocker,
) {
    let wry_events = wry_panes.poll_all_events();
    for event in wry_events {
        match event {
            WryEvent::LoadComplete { pane_id, url, .. } => {
                app_state.session_dirty = true;
                if let Ok(parsed) = url::Url::parse(&url) {
                    app_state.record_visit(&parsed, &url);
                }
                app_state.status_message = format!("Loaded: {}", &url[..url.len().min(60)]);

                if !url.starts_with("aileron://") {
                    let matching = content_scripts.scripts_for_url(&url, RunAt::DocumentIdle);
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
                    let ext_scripts = content_scripts.extension_scripts_for_url(&url, RunAt::DocumentIdle);
                    for ext_script in ext_scripts {
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            if !ext_script.css_code.is_empty() {
                                let escaped = ext_script.css_code.replace('\\', "\\\\").replace('`', "\\`").replace('$', "\\$");
                                wry_pane.execute_js(&format!(
                                    "setTimeout(function() {{ \
                                        var s = document.createElement('style'); \
                                        s.textContent = `{}`; \
                                        (document.head || document.documentElement).appendChild(s); \
                                    }}, 0);",
                                    escaped
                                ));
                            }
                            if !ext_script.js_code.is_empty() {
                                info!(
                                    "Injecting extension content script '{}' into {}",
                                    ext_script.script_id,
                                    &url[..url.len().min(40)]
                                );
                                wry_pane.execute_js(&ext_script.js_code);
                            }
                        }
                    }
                    if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                        wry_pane.execute_js(aileron::servo::NETWORK_MONITOR_JS);
                        wry_pane.execute_js(aileron::servo::CONSOLE_CAPTURE_JS);
                        wry_pane.execute_js(
                            aileron::passwords::bitwarden::BitwardenClient::form_submit_observer_js()
                        );
                        wry_pane.execute_js(
                            "setTimeout(function() { \
                                if (window._aileron_scroll_pos) { \
                                    window.scrollTo(window._aileron_scroll_pos.x, window._aileron_scroll_pos.y); \
                                } \
                            }, 100);"
                        );
                    }

                    if let Some(ref css) = app_state.config.custom_css
                        && !css.trim().is_empty()
                        && let Some(wry_pane) = wry_panes.get_mut(&pane_id)
                    {
                        let escaped = css.replace('\\', "\\\\").replace('`', "\\`").replace('$', "\\$");
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
                        && let Ok(settings) = aileron::db::site_settings::get_site_settings_for_url(db, &url)
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
                app_state.status_message = format!("Loading: {}...", &url[..url.len().min(40)]);
                if !url.starts_with("aileron://") {
                    let start_scripts = content_scripts.scripts_for_url(&url, RunAt::DocumentStart);
                    for script in start_scripts {
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            info!(
                                "Injecting document-start script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            wry_pane.execute_js(&script.js_code);
                        }
                    }
                    let ext_scripts = content_scripts.extension_scripts_for_url(&url, RunAt::DocumentStart);
                    for ext_script in ext_scripts {
                        if let Some(wry_pane) = wry_panes.get_mut(&pane_id) {
                            if !ext_script.css_code.is_empty() {
                                let escaped = ext_script.css_code.replace('\\', "\\\\").replace('`', "\\`").replace('$', "\\$");
                                wry_pane.execute_js(&format!(
                                    "var s = document.createElement('style'); \
                                     s.textContent = `{}`; \
                                     (document.documentElement || document.head).appendChild(s);",
                                    escaped
                                ));
                            }
                            if !ext_script.js_code.is_empty() {
                                info!(
                                    "Injecting extension document-start script '{}' into {}",
                                    ext_script.script_id,
                                    &url[..url.len().min(40)]
                                );
                                wry_pane.execute_js(&ext_script.js_code);
                            }
                        }
                    }
                }
            }
            WryEvent::TitleChanged { title, .. } => {
                app_state.status_message = title[..title.len().min(60)].to_string();
            }
            WryEvent::DownloadStarted { url, filename, .. } => {
                // Use the download manager for actual downloading with progress
                let dl_id = app_state.download_manager.start(url.as_str(), Some(filename.as_str()));
                let short_url = if url.len() > 40 { &url[..37] } else { &url };
                app_state.status_message = format!("Download #{}: {} ({})", dl_id, filename, short_url);
                info!("Download #{} started: {} from {}", dl_id, filename, url);
                // Record in database for history
                if let Some(db) = app_state.db.as_ref() {
                    let dest = app_state.download_manager.downloads_dir().join(filename.as_str());
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
                let _ = std::process::Command::new("xdg-open")
                    .arg(&path)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
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
) {
    let events = offscreen_panes.drain_all_events();
    for (_pane_id, event) in events {
        match event {
            WryEvent::LoadComplete { pane_id, url, .. } => {
                app_state.session_dirty = true;
                if let Ok(parsed) = url::Url::parse(&url) {
                    app_state.record_visit(&parsed, &url);
                }
                app_state.status_message = format!("Loaded: {}", &url[..url.len().min(60)]);

                if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                    pane.mark_dirty();
                }

                if !url.starts_with("aileron://") {
                    let matching = content_scripts.scripts_for_url(&url, RunAt::DocumentIdle);
                    for script in matching {
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            info!(
                                "Injecting content script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            pane.execute_js(&script.js_code);
                        }
                    }
                    let ext_scripts = content_scripts.extension_scripts_for_url(&url, RunAt::DocumentIdle);
                    for ext_script in ext_scripts {
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            if !ext_script.css_code.is_empty() {
                                let escaped = ext_script.css_code.replace('\\', "\\\\").replace('`', "\\`").replace('$', "\\$");
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
                            if !ext_script.js_code.is_empty() {
                                info!(
                                    "Injecting extension content script '{}' into {}",
                                    ext_script.script_id,
                                    &url[..url.len().min(40)]
                                );
                                pane.execute_js(&ext_script.js_code);
                                pane.mark_dirty();
                            }
                        }
                    }
                    if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                        pane.execute_js(aileron::servo::NETWORK_MONITOR_JS);
                        pane.execute_js(aileron::servo::CONSOLE_CAPTURE_JS);
                        pane.execute_js(
                            aileron::passwords::bitwarden::BitwardenClient::form_submit_observer_js()
                        );
                        pane.suppress_context_menu();
                        pane.execute_js(
                            "setTimeout(function() { \
                                if (window._aileron_scroll_pos) { \
                                    window.scrollTo(window._aileron_scroll_pos.x, window._aileron_scroll_pos.y); \
                                } \
                            }, 100);"
                        );
                    }

                    if let Some(ref css) = app_state.config.custom_css
                        && !css.trim().is_empty()
                        && let Some(pane) = offscreen_panes.get_mut(&pane_id)
                    {
                        let escaped = css.replace('\\', "\\\\").replace('`', "\\`").replace('$', "\\$");
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
                        && let Ok(settings) = aileron::db::site_settings::get_site_settings_for_url(db, &url)
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
                app_state.status_message = format!("Loading: {}...", &url[..url.len().min(40)]);
                if !url.starts_with("aileron://") {
                    let start_scripts = content_scripts.scripts_for_url(&url, RunAt::DocumentStart);
                    for script in start_scripts {
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            info!(
                                "Injecting document-start script '{}' into {}",
                                script.name,
                                &url[..url.len().min(40)]
                            );
                            pane.execute_js(&script.js_code);
                            pane.mark_dirty();
                        }
                    }
                    let ext_scripts = content_scripts.extension_scripts_for_url(&url, RunAt::DocumentStart);
                    for ext_script in ext_scripts {
                        if let Some(pane) = offscreen_panes.get_mut(&pane_id) {
                            if !ext_script.css_code.is_empty() {
                                let escaped = ext_script.css_code.replace('\\', "\\\\").replace('`', "\\`").replace('$', "\\$");
                                pane.execute_js(&format!(
                                    "var s = document.createElement('style'); \
                                     s.textContent = `{}`; \
                                     (document.documentElement || document.head).appendChild(s);",
                                    escaped
                                ));
                            }
                            if !ext_script.js_code.is_empty() {
                                info!(
                                    "Injecting extension document-start script '{}' into {}",
                                    ext_script.script_id,
                                    &url[..url.len().min(40)]
                                );
                                pane.execute_js(&ext_script.js_code);
                            }
                            pane.mark_dirty();
                        }
                    }
                }
            }
            WryEvent::TitleChanged { title, .. } => {
                app_state.status_message = title[..title.len().min(60)].to_string();
            }
            WryEvent::DownloadStarted { url, filename, .. } => {
                // Use the download manager for actual downloading with progress
                let dl_id = app_state.download_manager.start(url.as_str(), Some(filename.as_str()));
                let short_url = if url.len() > 40 { &url[..37] } else { &url };
                app_state.status_message = format!("Download #{}: {} ({})", dl_id, filename, short_url);
                info!("Download #{} started: {} from {}", dl_id, filename, url);
                // Record in database for history
                if let Some(db) = app_state.db.as_ref() {
                    let dest = app_state.download_manager.downloads_dir().join(filename.as_str());
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
                let _ = std::process::Command::new("xdg-open")
                    .arg(&path)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
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
        if let Err(e) = transport.run_stdio() {
            warn!("MCP server error: {}", e);
        }
    });
}

fn handle_ipc_message(
    app_state: &mut AppState,
    wry_panes: &mut WryPaneManager,
    pane_id: Uuid,
    message: &str,
) {
    let msg: serde_json::Value = match serde_json::from_str(message) {
        Ok(m) => m,
        Err(_) => return,
    };
    match msg.get("t").and_then(|v| v.as_str()) {
        Some("get-config") => {
            let config_json = serde_json::to_string(&app_state.config).unwrap_or_default();
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
                if let Some(v) = config_obj.get("tab_sidebar_right").and_then(|v| v.as_bool()) {
                    app_state.config.tab_sidebar_right = v;
                }
                if let Some(v) = config_obj.get("adblock_enabled").and_then(|v| v.as_bool()) {
                    app_state.config.adblock_enabled = v;
                }
                if let Some(v) = config_obj.get("https_upgrade_enabled").and_then(|v| v.as_bool()) {
                    app_state.config.https_upgrade_enabled = v;
                }
                if let Some(v) = config_obj.get("tracking_protection_enabled").and_then(|v| v.as_bool()) {
                    app_state.config.tracking_protection_enabled = v;
                }
                if let Some(v) = config_obj.get("devtools").and_then(|v| v.as_bool()) {
                    app_state.config.devtools = v;
                }
                if let Some(v) = config_obj.get("proxy") {
                    app_state.config.proxy = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("custom_css") {
                    app_state.config.custom_css = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("engine_selection").and_then(|v| v.as_str()) {
                    app_state.config.engine_selection = v.to_string();
                }
                if let Some(v) = config_obj.get("language") {
                    app_state.config.language = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("adaptive_quality").and_then(|v| v.as_bool()) {
                    app_state.config.adaptive_quality = v;
                }
                if let Some(v) = config_obj.get("popup_blocker_enabled").and_then(|v| v.as_bool()) {
                    app_state.config.popup_blocker_enabled = v;
                }
                if let Some(v) = config_obj.get("adblock_update_interval_hours").and_then(|v| v.as_u64()) {
                    app_state.config.adblock_update_interval_hours = v;
                }
                if let Some(v) = config_obj.get("theme").and_then(|v| v.as_str()) {
                    app_state.config.theme = v.to_string();
                }
                if let Some(v) = config_obj.get("adblock_cosmetic_filtering").and_then(|v| v.as_bool()) {
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
                if let Some(v) = config_obj.get("sync_passphrase").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
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
                if let Some(v) = config_obj.get("sync_auto_interval_sec").and_then(|v| v.as_u64()) {
                    app_state.config.sync_auto_interval_sec = v;
                }
                if let Err(e) = aileron::config::Config::save(&app_state.config) {
                    warn!("Failed to save config: {}", e);
                }
                if let Some(pane) = wry_panes.get_mut(&pane_id) {
                    pane.execute_js("window._onConfigSaved && window._onConfigSaved();");
                }
                app_state.status_message = "Settings saved".into();
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
                        warn!("Failed to save credential: {}", e);
                        app_state.status_message = format!("Credential save failed: {}", e);
                    }
                }
            } else {
                app_state.status_message = "No pending credentials to save".into();
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
    let msg: serde_json::Value = match serde_json::from_str(message) {
        Ok(m) => m,
        Err(_) => return,
    };
    match msg.get("t").and_then(|v| v.as_str()) {
        Some("get-config") => {
            let config_json = serde_json::to_string(&app_state.config).unwrap_or_default();
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
                if let Some(v) = config_obj.get("tab_sidebar_right").and_then(|v| v.as_bool()) {
                    app_state.config.tab_sidebar_right = v;
                }
                if let Some(v) = config_obj.get("adblock_enabled").and_then(|v| v.as_bool()) {
                    app_state.config.adblock_enabled = v;
                }
                if let Some(v) = config_obj.get("https_upgrade_enabled").and_then(|v| v.as_bool()) {
                    app_state.config.https_upgrade_enabled = v;
                }
                if let Some(v) = config_obj.get("tracking_protection_enabled").and_then(|v| v.as_bool()) {
                    app_state.config.tracking_protection_enabled = v;
                }
                if let Some(v) = config_obj.get("devtools").and_then(|v| v.as_bool()) {
                    app_state.config.devtools = v;
                }
                if let Some(v) = config_obj.get("proxy") {
                    app_state.config.proxy = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("custom_css") {
                    app_state.config.custom_css = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("engine_selection").and_then(|v| v.as_str()) {
                    app_state.config.engine_selection = v.to_string();
                }
                if let Some(v) = config_obj.get("language") {
                    app_state.config.language = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
                }
                if let Some(v) = config_obj.get("adaptive_quality").and_then(|v| v.as_bool()) {
                    app_state.config.adaptive_quality = v;
                }
                if let Some(v) = config_obj.get("popup_blocker_enabled").and_then(|v| v.as_bool()) {
                    app_state.config.popup_blocker_enabled = v;
                }
                if let Some(v) = config_obj.get("adblock_update_interval_hours").and_then(|v| v.as_u64()) {
                    app_state.config.adblock_update_interval_hours = v;
                }
                if let Some(v) = config_obj.get("theme").and_then(|v| v.as_str()) {
                    app_state.config.theme = v.to_string();
                }
                if let Some(v) = config_obj.get("adblock_cosmetic_filtering").and_then(|v| v.as_bool()) {
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
                if let Some(v) = config_obj.get("sync_passphrase").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
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
                if let Some(v) = config_obj.get("sync_auto_interval_sec").and_then(|v| v.as_u64()) {
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
                        warn!("Failed to save credential: {}", e);
                        app_state.status_message = format!("Credential save failed: {}", e);
                    }
                }
            } else {
                app_state.status_message = "No pending credentials to save".into();
            }
        }
        _ => {}
    }
}
