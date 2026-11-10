//! The aileron agent runtime (S4 stage 2).
//!
//! Currently hosts the MCP server bootstrap (stdio + TCP bridge). The
//! agent event loop moves here in stage 2c, once `process_mcp_commands`
//! crosses the crate boundary via the `PaneOps` callback trait — the
//! target layout gives `aileron-agent` = thin `aileron_core::agent::run()`.

use crate::mcp::bridge::McpBridge;
use tracing::{info, warn};
use uuid::Uuid;

use crate::mcp::bridge::McpCommand;

/// Pane operations the MCP dispatch layer needs, abstracted so
/// `process_mcp_commands` can live in core while the pane mechanics
/// (GTK pumping, settle waits, frame capture) stay in the engine side.
///
/// Semantics are behavior-locked by the GATE-3 soak — implementations
/// must preserve the settle budgets (wry URL-property pump 15s,
/// offscreen is_loading settle 60s) and the two-path screenshot
/// capture exactly.
pub trait PaneOps {
    /// First non-nil content pane id (offscreen iteration order) — the
    /// fallback when the wm reports the chrome pane as active.
    fn first_content_pane(&self) -> Option<uuid::Uuid>;
    /// True when either manager owns a pane with this id.
    fn pane_exists(&self, id: &uuid::Uuid) -> bool;
    /// (url, title) for whichever manager owns the pane.
    fn pane_url_title(&self, id: &uuid::Uuid) -> Option<(String, String)>;
    /// url for whichever manager owns the pane.
    fn pane_url(&self, id: &uuid::Uuid) -> Option<String>;
    /// (id, url, title) for every offscreen pane.
    fn all_panes(&self) -> Vec<(uuid::Uuid, String, String)>;
    /// Navigate the pane owned by either manager, with that engine's
    /// deterministic settle semantics. Returns Some(is_offscreen) when
    /// handled, None when no manager owns the pane (phantom).
    fn navigate_settled(&mut self, id: &uuid::Uuid, url: &url::Url) -> Option<bool>;
    /// Set the user agent (offscreen panes only). Returns handled.
    fn set_user_agent(&mut self, id: &uuid::Uuid, ua: &str) -> bool;
    /// Set the viewport size (offscreen panes only). Returns handled.
    fn set_viewport(&mut self, id: &uuid::Uuid, w: i32, h: i32) -> bool;
    /// Capture the pane as a PNG data-URI. Offscreen: synchronous frame
    /// grab + encode. Wry: Canvas capture with the GTK pump, 10s budget.
    /// Error strings follow the established MCP response phrasing.
    fn screenshot_data_uri(&mut self, id: &uuid::Uuid) -> String;
    /// Dispatch JS against the active pane; the callback receives the
    /// decoded result (wry's JSON-encoded wrapper stripped). Returns
    /// dispatched (false = no pane owned).
    fn execute_js(
        &mut self,
        id: &uuid::Uuid,
        js: &str,
        on_result: Box<dyn Fn(String) + Send>,
    ) -> bool;
    /// Reset the offscreen manager (the workspace-restore arm's
    /// fresh-managers pattern).
    fn reset_offscreen(&mut self);
    /// Live WebKitGTK load state for the pane (the settle poll).
    fn webview_is_loading(&self, id: &uuid::Uuid) -> bool;
    /// Pump the event loop so pending load/frame events land.
    fn pump(&mut self);
}

/// Bootstrap both MCP transports (stdio + TCP). All-core types: the
/// headed/agent caller provides the shared bridge and session registry.
pub fn spawn_mcp_server(
    mcp_bridge: &McpBridge,
    registry: crate::mcp::bridge::SessionRegistry,
    policy: &str,
) {
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
    let stdio_tools = crate::mcp::tools::create_tools(mcp_state, mcp_bridge.command_tx.clone());
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

// ---------------------------------------------------------------------------
// MCP dispatch (S4 stage 2c step 3): moved from frame_tasks — the body is
// core-clean via the PaneOps seam (crate:: paths resolve identically in core:
// db, wm, mcp, downloads, workspace_restore are all core modules).
// ---------------------------------------------------------------------------

/// Environment trace flag shared with the engine layer.
pub fn mcp_trace_enabled() -> bool {
    std::env::var("AILERON_MCP_TRACE").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

pub fn process_mcp_commands(
    mcp_bridge: &McpBridge,
    active_id: Uuid,
    core: &mut crate::state::CoreState,
    panes: &mut dyn PaneOps,
) {
    // MCP tools target web content, not the browser chrome overlay. When the
    // wm reports the chrome pane (nil UUID) as active - which happens on
    // Wayland during page loads - fall back to the first real content pane,
    // otherwise read/click tools silently operate on the chrome UI.
    let original_id = active_id;
    let active_id = if active_id == uuid::Uuid::nil() {
        panes.first_content_pane().unwrap_or(active_id)
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
    if core.pending_workspace_restore.is_some() {
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
            let caps = core
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
                audit_record(core, session, &command, active_id, true);
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
        audit_record(core, session, &command, active_id, false);
        wait_mcp_nav_settled(core, panes, &active_id, std::time::Duration::from_secs(90));
        match command {
            McpCommand::Navigate { url, new_tab } => {
                if let Ok(parsed) = url::Url::parse(&url) {
                    if new_tab {
                        let current_active = core.wm.active_pane_id();
                        match core.wm.split(
                            current_active,
                            crate::wm::SplitDirection::Vertical,
                            0.5,
                        ) {
                            Ok(new_id) => {
                                info!("MCP: opening in new tab {}", url);
                                // Queue real webview creation: engines.create_pane
                                // builds a wry-engine pane which never renders on
                                // Wayland (phantom pane, all tools dead on it).
                                core.pending_mcp_pane_creations
                                    .push_back((new_id, parsed.clone()));
                                core.pending_wry_actions
                                    .push_back(crate::state::WryAction::Navigate(parsed));
                                core.wm.set_active_pane(new_id);
                                // YP-MCP-NAV-EPOCH-001: advance the pane epoch.
                                *core.mcp_nav_epoch.entry(new_id).or_insert(0) += 1;
                                core.session.session_dirty = true;
                            }
                            Err(e) => {
                                warn!("MCP: failed to create new tab: {}", e);
                            }
                        }
                    } else if !panes.pane_exists(&active_id) {
                        // Phantom pane (wm entry without a webview - e.g. an
                        // MCP-created tab from a previous session or a crashed
                        // webview): a navigate cannot reach it. Attach a real
                        // webview pointed at the target URL instead.
                        warn!(
                            "MCP: navigating phantom pane {} - creating webview",
                            &active_id.to_string()[..8]
                        );
                        core.pending_mcp_pane_creations
                            .push_back((active_id, parsed));
                        core.session.session_dirty = true;
                    } else {
                        match panes.navigate_settled(&active_id, &parsed) {
                            Some(false) => {
                                // X11 / GTK-window-embedded native pane.
                                info!("MCP: navigating to {}", url);
                                // YP-MCP-NAV-EPOCH-001: advance the pane epoch.
                                *core.mcp_nav_epoch.entry(active_id).or_insert(0) += 1;
                                core.session.session_dirty = true;
                            }
                            Some(true) => {
                                // Wayland: offscreen pane, deterministic settle.
                                // YP-MCP-NAV-EPOCH-001: advance the epoch and
                                // promote settled (the load committed or the
                                // budget expired - reads must never wedge).
                                *core.mcp_nav_epoch.entry(active_id).or_insert(0) += 1;
                                let epoch_now =
                                    core.mcp_nav_epoch.get(&active_id).copied().unwrap_or(0);
                                core.mcp_settled_epoch.insert(active_id, epoch_now);
                                core.session.session_dirty = true;
                            }
                            None => {
                                warn!("MCP: navigate target pane vanished");
                            }
                        }
                    }
                } else {
                    warn!("MCP: invalid navigate URL: {}", url);
                }
            }
            McpCommand::ExecuteJs { code, response_tx } => {
                info!("MCP: executing JS ({} chars)", code.len());
                let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(response_tx)));
                let tx_for_cb = std::sync::Arc::clone(&tx);
                let dispatched = panes.execute_js(
                    &active_id,
                    &code,
                    Box::new(move |result| {
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
                    }),
                );
                if !dispatched
                    && let Ok(mut guard) = tx.lock()
                    && let Some(sender) = guard.take()
                {
                    let _ = sender.send("Error: No active pane".to_string());
                }
            }
            McpCommand::GetActivePane { response_tx } => {
                let (url, title) = panes.pane_url_title(&active_id).unwrap_or_default();
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
                    let result = core
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
                        core.session_registry
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
                let url_of = |id: Uuid| -> Option<String> { panes.pane_url(&id) };
                let result = core
                    .wm
                    .to_workspace_data(url_of)
                    .map_err(|e| e.to_string())
                    .and_then(|data| {
                        core.db
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
                let url_of = |id: Uuid| -> Option<String> { panes.pane_url(&id) };
                let result = core
                    .wm
                    .to_workspace_data(url_of)
                    .map_err(|e| e.to_string())
                    .and_then(|live| {
                        core
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
                let result = core
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
                let reloaded = if panes.set_user_agent(&active_id, &ua) {
                    let current = panes.pane_url(&active_id);
                    if let Some(current) = current {
                        if let Ok(current) = url::Url::parse(&current) {
                            panes.navigate_settled(&active_id, &current).is_some()
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                } else {
                    false
                };
                if reloaded {
                    *core.mcp_nav_epoch.entry(active_id).or_insert(0) += 1;
                    let epoch_now = core.mcp_nav_epoch.get(&active_id).copied().unwrap_or(0);
                    core.mcp_settled_epoch.insert(active_id, epoch_now);
                    core.session.session_dirty = true;
                }
                let _ = response_tx.send(if reloaded {
                    format!("User agent set and page reloaded: {ua}")
                } else {
                    "Error: could not set user agent (no webkit child)".into()
                });
            }
            McpCommand::SetViewport {
                width,
                height,
                response_tx,
            } => {
                // Era-3 mobile lane: the layout viewport is what responsive
                // sites key on (media queries) - a mobile UA alone keeps a
                // desktop layout. Resize, then re-load under the settle hold.
                let (w, h) = (width.max(200), height.max(200));
                let reloaded = if panes.set_viewport(&active_id, w, h) {
                    let current = panes.pane_url(&active_id);
                    if let Some(current) = current {
                        if let Ok(current) = url::Url::parse(&current) {
                            panes.navigate_settled(&active_id, &current).is_some()
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                } else {
                    false
                };
                if reloaded {
                    *core.mcp_nav_epoch.entry(active_id).or_insert(0) += 1;
                    let epoch_now = core.mcp_nav_epoch.get(&active_id).copied().unwrap_or(0);
                    core.mcp_settled_epoch.insert(active_id, epoch_now);
                    core.session.session_dirty = true;
                }
                let _ = response_tx.send(if reloaded {
                    format!("Viewport set to {w}x{h} and page reloaded")
                } else {
                    "Error: no active pane".into()
                });
            }
            McpCommand::WorkspaceRestore { name, response_tx } => {
                // Era-2 S42: full restore via the shared workspace_restore
                // flow (fresh pane managers, tree rebuild, webviews queued).
                // The headed runtime's pattern: fresh managers -> restore ->
                // queue (pid, url) creations; in agent mode the tick's
                // pending_mcp_pane_creations drain performs creation.
                let viewport = crate::wm::Rect::new(0.0, 0.0, 1280.0, 800.0);
                panes.reset_offscreen();
                let outcome = crate::workspace_restore::restore_workspace(
                    &name,
                    viewport,
                    core.db.as_ref(),
                    &mut core.engines,
                    &mut core.wm,
                );
                let msg = match outcome {
                    crate::workspace_restore::RestoreOutcome::Restored(result) => {
                        // Terminal mirror is a caller concern since stage 2c.
                        #[cfg(feature = "terminal")]
                        for pid in &result.removed_pane_ids {
                            core.terminal_pane_ids.remove(pid);
                        }
                        #[cfg(feature = "terminal")]
                        for (pid, url) in &result.panes_to_create {
                            if url.as_str() == "aileron://terminal" {
                                core.terminal_pane_ids.insert(*pid);
                            }
                        }
                        for (pid, url) in &result.panes_to_create {
                            core.pending_mcp_pane_creations
                                .push_back((*pid, url.clone()));
                        }
                        format!("restored '{}' ({} panes queued)", name, result.pane_count)
                    }
                    crate::workspace_restore::RestoreOutcome::NotFound => {
                        format!("Error: checkpoint '{name}' not found")
                    }
                    crate::workspace_restore::RestoreOutcome::NoDatabase => {
                        "Error: no database".into()
                    }
                    crate::workspace_restore::RestoreOutcome::TreeError(e) => {
                        format!("Error (tree): {e}")
                    }
                };
                let _ = response_tx.send(msg);
            }
            McpCommand::WorkspaceQuery {
                contains,
                response_tx,
            } => {
                // Era-2 S44 (PRO-LONG programmatic-memory pattern): agents
                // query the live pane set instead of dumping context.
                let needle = contains.to_lowercase();
                let mut hits: Vec<String> = Vec::new();
                for (id, url, title) in panes.all_panes() {
                    if url.to_lowercase().contains(&needle)
                        || title.to_lowercase().contains(&needle)
                    {
                        hits.push(format!(
                            "{{\"pane\":\"{id}\",\"title\":{:?},\"url\":{:?}}}",
                            title, url
                        ));
                    }
                }
                let _ = response_tx.send(if hits.is_empty() {
                    format!("No panes matching '{contains}'.")
                } else {
                    format!("[{}]", hits.join(","))
                });
            }
            McpCommand::ReadAudit { limit, response_tx } => {
                let log = core.audit_log.borrow();
                let skip = log.len().saturating_sub(limit);
                let lines: Vec<String> = log.iter().skip(skip).cloned().collect();
                let _ = response_tx.send(if lines.is_empty() {
                    "Audit log empty.".into()
                } else {
                    lines.join("\n")
                });
            }
            McpCommand::ListDownloads { response_tx } => {
                let rows: Vec<String> = core
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
                let result = if let Some(db) = core.db.as_ref() {
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
                let result = if let Some(db) = core.db.as_ref() {
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
                let result = if let Some(db) = core.db.as_ref() {
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
                let result = if let Some(db) = core.db.as_ref() {
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
                let active = core.wm.active_pane_id();
                let pane_ids: Vec<Uuid> = core.wm.panes_ref().iter().map(|(id, _)| *id).collect();
                let lines: Vec<String> = pane_ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        let marker = if *id == active { " [active]" } else { "" };
                        // wry pane (X11) first, offscreen pane (Wayland)
                        // fallback — offscreen panes report real state instead
                        // of a permanent "about:blank /(untitled)".
                        let (url, title) = panes
                            .pane_url_title(id)
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
                let result = panes.screenshot_data_uri(&active_id);
                let _ = response_tx.send(result);
            }
            McpCommand::CloseTab { index, response_tx } => {
                let pane_ids: Vec<Uuid> = core.wm.pane_ids().into_iter().collect();
                let result = if let Some(&close_id) = pane_ids.get(index) {
                    let active_before = core.wm.active_pane_id();
                    core.wm.set_active_pane(close_id);
                    if let Err(e) = core.wm.close(close_id) {
                        warn!(%e, "Failed to close tab via MCP");
                    }
                    core.engines.remove_pane(&close_id);
                    #[cfg(feature = "terminal")]
                    core.terminal_pane_ids.remove(&close_id);
                    if active_before == close_id
                        && let Some(&next) = pane_ids.iter().find(|&&id| id != close_id)
                    {
                        core.wm.set_active_pane(next);
                    }
                    core.session.session_dirty = true;
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
                let result = if panes.execute_js(&active_id, &js, Box::new(|_| {})) {
                    format!("Scrolled by ({delta_x}, {delta_y}) pixels")
                } else {
                    "Error: no active pane".into()
                };
                let _ = response_tx.send(result);
            }
            McpCommand::NavigateBack { response_tx } => {
                let result =
                    if panes.execute_js(&active_id, "window.history.back()", Box::new(|_| {})) {
                        "Navigated back".into()
                    } else {
                        "Error: no active pane".into()
                    };
                let _ = response_tx.send(result);
            }
            McpCommand::NavigateForward { response_tx } => {
                let result =
                    if panes.execute_js(&active_id, "window.history.forward()", Box::new(|_| {})) {
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
                let dispatched = panes.execute_js(
                    &active_id,
                    &js,
                    Box::new(move |result| {
                        if let Ok(mut guard) = tx.lock()
                            && let Some(sender) = guard.take()
                        {
                            let _ = sender.send(result);
                        }
                    }),
                );
                let result = if dispatched {
                    match wait_for_js_result(response_rx, std::time::Duration::from_secs(10), panes)
                    {
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
                let dispatched = panes.execute_js(
                    &active_id,
                    js,
                    Box::new(move |result| {
                        if let Ok(mut guard) = tx.lock()
                            && let Some(sender) = guard.take()
                        {
                            let _ = sender.send(result);
                        }
                    }),
                );
                let result = if dispatched {
                    match wait_for_js_result(response_rx, std::time::Duration::from_secs(10), panes)
                    {
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
                let dispatched = panes.execute_js(
                    &active_id,
                    js,
                    Box::new(move |result| {
                        if let Ok(mut guard) = tx.lock()
                            && let Some(sender) = guard.take()
                        {
                            let _ = sender.send(result);
                        }
                    }),
                );
                let result = if dispatched {
                    match wait_for_js_result(response_rx, std::time::Duration::from_secs(10), panes)
                    {
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
                    let dispatched = panes.execute_js(
                        &active_id,
                        "document.body.innerText",
                        Box::new(move |result| {
                            if let Ok(mut guard) = text_tx_mutex.lock()
                                && let Some(sender) = guard.take()
                            {
                                let _ = sender.send(result);
                            }
                        }),
                    );

                    if dispatched {
                        match wait_for_js_result(
                            text_rx,
                            std::time::Duration::from_secs(120),
                            panes,
                        ) {
                            Some(text) if !text.is_empty() && text != "null" => {
                                llm_summarize(&text)
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
                    let dispatched = panes.execute_js(
                        &active_id,
                        "document.body.innerText",
                        Box::new(move |result| {
                            if let Ok(mut guard) = text_tx_mutex.lock()
                                && let Some(sender) = guard.take()
                            {
                                let _ = sender.send(result);
                            }
                        }),
                    );

                    if dispatched {
                        match wait_for_js_result(
                            text_rx,
                            std::time::Duration::from_secs(120),
                            panes,
                        ) {
                            Some(text) if !text.is_empty() && text != "null" => {
                                llm_translate(&text, &target_lang)
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
                    let dispatched = panes.execute_js(
                        &active_id,
                        "document.body.innerText",
                        Box::new(move |result| {
                            if let Ok(mut guard) = text_tx_mutex.lock()
                                && let Some(sender) = guard.take()
                            {
                                let _ = sender.send(result);
                            }
                        }),
                    );

                    if dispatched {
                        match wait_for_js_result(
                            text_rx,
                            std::time::Duration::from_secs(120),
                            panes,
                        ) {
                            Some(text) if !text.is_empty() && text != "null" => llm_analyze(&text),
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

/// Fire-and-forget variant of [`dispatch_active_js_with_callback`].
/// YP-MCP-NAV-EPOCH-001 §5: hold pump-waiting commands until every
/// navigation issued for the pane has finished loading (read-your-writes).
/// The settled epoch promotes when the pane stops reporting loading - the
/// same signal the frame loop uses to clear the loading flag. Bounded wait;
/// on timeout the command runs anyway (best effort, never a wedge).
pub fn wait_mcp_nav_settled(
    core: &mut crate::state::CoreState,
    panes: &mut dyn PaneOps,
    pane: &uuid::Uuid,
    budget: std::time::Duration,
) {
    let deadline = std::time::Instant::now() + budget;
    let mut saw_loading = false;
    let trace = mcp_trace_enabled();
    if trace {
        eprintln!(
            "[mcp-trace] wait enter nav={} settled={}",
            core.mcp_nav_epoch.get(pane).copied().unwrap_or(0),
            core.mcp_settled_epoch.get(pane).copied().unwrap_or(0)
        );
    }
    loop {
        let nav = *core.mcp_nav_epoch.get(pane).unwrap_or(&0);
        let settled = *core.mcp_settled_epoch.get(pane).unwrap_or(&0);
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
            let loading = panes.webview_is_loading(pane);
            if loading {
                saw_loading = true;
            } else if saw_loading {
                if trace {
                    eprintln!("[mcp-trace] wait exit: observed commit -> promote");
                }
                core.mcp_settled_epoch.insert(*pane, nav);
                return;
            }
            // !loading && !saw: the navigation has not started yet (or the
            // pane fires no load events) - keep polling; the deadline
            // promotes best-effort so reads never wedge.
        }
        #[cfg(not(target_os = "linux"))]
        {
            let nav = *core.mcp_nav_epoch.get(pane).unwrap_or(&0);
            core.mcp_settled_epoch.insert(*pane, nav);
            return;
        }
        if std::time::Instant::now() >= deadline {
            let nav = *core.mcp_nav_epoch.get(pane).unwrap_or(&0);
            core.mcp_settled_epoch.insert(*pane, nav);
            return;
        }
        panes.pump();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// Append one entry to the enterprise audit ring (Era-3): provenance per
/// dispatched or denied command. Cap 1000, oldest dropped.
pub fn audit_record(
    core: &mut crate::state::CoreState,
    session: Option<Uuid>,
    command: &McpCommand,
    pane: Uuid,
    denied: bool,
) {
    let epoch = core.mcp_settled_epoch.get(&pane).copied().unwrap_or(0);
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
    let mut log = core.audit_log.borrow_mut();
    if log.len() >= 1000 {
        log.pop_front();
    }
    log.push_back(entry);
}

#[cfg(feature = "llm")]
fn llm_summarize(text: &str) -> String {
    let config = crate::config::Config::load();
    if config.llm_base_url.is_empty() {
        return "Error: LLM not configured (llm_base_url is empty)".into();
    }
    let client = crate::llm::OllamaClient::new(crate::llm::OllamaConfig {
        base_url: config.llm_base_url,
        default_model: config.llm_model,
        timeout_secs: config.llm_timeout_secs,
        temperature: config.llm_temperature,
        max_tokens: config.llm_max_tokens,
    });
    match client.summarize(text) {
        Ok(summary) => format!("## Summary\n\n{summary}"),
        Err(e) => format!("Error: LLM summarization failed: {e}"),
    }
}

#[cfg(not(feature = "llm"))]
fn llm_summarize(_text: &str) -> String {
    "Error: LLM feature disabled".into()
}

#[cfg(feature = "llm")]
fn llm_translate(text: &str, target_lang: &str) -> String {
    let config = crate::config::Config::load();
    if config.llm_base_url.is_empty() {
        return "Error: LLM not configured (llm_base_url is empty)".into();
    }
    let client = crate::llm::OllamaClient::new(crate::llm::OllamaConfig {
        base_url: config.llm_base_url,
        default_model: config.llm_model,
        timeout_secs: config.llm_timeout_secs,
        temperature: config.llm_temperature,
        max_tokens: config.llm_max_tokens,
    });
    match client.translate(text, target_lang) {
        Ok(translated) => format!("## Translation ({target_lang})\n\n{translated}"),
        Err(e) => format!("Error: LLM translation failed: {e}"),
    }
}

#[cfg(not(feature = "llm"))]
fn llm_translate(_text: &str, _target_lang: &str) -> String {
    "Error: LLM feature disabled".into()
}

#[cfg(feature = "llm")]
fn llm_analyze(text: &str) -> String {
    let config = crate::config::Config::load();
    if config.llm_base_url.is_empty() {
        return "Error: LLM not configured (llm_base_url is empty)".into();
    }
    let client = crate::llm::OllamaClient::new(crate::llm::OllamaConfig {
        base_url: config.llm_base_url,
        default_model: config.llm_model,
        timeout_secs: config.llm_timeout_secs,
        temperature: config.llm_temperature,
        max_tokens: config.llm_max_tokens,
    });
    match client.analyze(text) {
        Ok(analysis) => format!("## Analysis\n\n{analysis}"),
        Err(e) => format!("Error: LLM analysis failed: {e}"),
    }
}

#[cfg(not(feature = "llm"))]
fn llm_analyze(_text: &str) -> String {
    "Error: LLM feature disabled".into()
}

/// Poll a JS-result channel until it delivers (or the budget expires),
/// pumping the event loop so pane callbacks land. Engine mechanics via
/// PaneOps::pump.
pub fn wait_for_js_result(
    mut rx: tokio::sync::oneshot::Receiver<String>,
    timeout: std::time::Duration,
    panes: &mut dyn PaneOps,
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
        panes.pump();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
