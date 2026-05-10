use super::super::AppState;

pub fn save_workspace_with_urls(
    state: &AppState,
    name: &str,
    pane_urls: &std::collections::HashMap<uuid::Uuid, String>,
) -> anyhow::Result<()> {
    let db = state
        .db
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No database connection"))?;

    let url_resolver = |pane_id: uuid::Uuid| -> Option<String> { pane_urls.get(&pane_id).cloned() };

    let data = state.wm.to_workspace_data(url_resolver)?;
    crate::db::workspaces::save_workspace(db, name, &data)?;
    Ok(())
}

pub fn list_workspaces(state: &AppState) -> Vec<crate::db::workspaces::Workspace> {
    state
        .db
        .as_ref()
        .and_then(|conn| crate::db::workspaces::list_workspaces(conn).ok())
        .unwrap_or_default()
}

pub fn swap_panes(state: &mut AppState) {
    if let Some(last_id) = state.tabs.last_active_pane_id {
        let active_id = state.wm.active_pane_id();
        if last_id != active_id && state.wm.panes().iter().any(|(id, _)| *id == last_id) {
            let active_url = state
                .engines
                .get(&active_id)
                .and_then(|e| e.current_url().cloned());
            let last_url = state
                .engines
                .get(&last_id)
                .and_then(|e| e.current_url().cloned());
            if let (Some(a_url), Some(l_url)) = (active_url, last_url) {
                if let Some(engine) = state.engines.get_mut(&active_id) {
                    engine.navigate(&l_url);
                }
                if let Some(engine) = state.engines.get_mut(&last_id) {
                    engine.navigate(&a_url);
                }
                state.ui.status_message = "Panes swapped".into();
            }
        } else {
            state.ui.status_message = "No previous pane to swap with".into();
        }
    } else {
        state.ui.status_message = "No previous pane".into();
    }
}

#[must_use = "ignoring this value may lead to unexpected behavior"]
pub fn handle_workspace_commands(state: &mut AppState, query: &str) -> Option<()> {
    if let Some(name) = query.strip_prefix("ws-save ") {
        let name = name.trim();
        if name.is_empty() {
            state.ui.status_message = "Usage: ws-save <name>".into();
            return Some(());
        }
        state
            .pending_wry_actions
            .push_back(crate::app::WryAction::SaveWorkspace {
                name: name.to_string(),
                pane_urls: std::collections::HashMap::new(),
            });
        state.ui.status_message = format!("Saving workspace: {name}...");
        state.current_workspace_name = name.to_string();
        return Some(());
    }

    if query == "ws-list" {
        let workspaces = list_workspaces(state);
        if workspaces.is_empty() {
            state.ui.status_message = "No saved workspaces.".into();
        } else {
            let names: Vec<&str> = workspaces
                .iter()
                .filter(|w| w.name != "_autosave")
                .map(|w| w.name.as_str())
                .collect();
            state.ui.status_message = format!("Workspaces: {}", names.join(", "));
        }
        return Some(());
    }

    if let Some(name) = query.strip_prefix("ws-load ") {
        let name = name.trim();
        if name.is_empty() {
            state.ui.status_message = "Usage: ws-load <name>".into();
            return Some(());
        }
        state.pending_workspace_restore = Some(name.to_string());
        state.current_workspace_name = name.to_string();
        state.ui.status_message = format!("Restoring workspace: {name}...");
        return Some(());
    }

    if let Some(name) = query.strip_prefix("ws-delete ") {
        let name = name.trim();
        if name.is_empty() {
            state.ui.status_message = "Usage: ws-delete <name>".into();
            return Some(());
        }
        if let Some(db) = state.db.as_ref() {
            match crate::db::workspaces::delete_workspace(db, name) {
                Ok(true) => {
                    state.ui.status_message = format!("Workspace deleted: {name}");
                    if name == state.current_workspace_name {
                        state.current_workspace_name = "default".into();
                    }
                }
                Ok(false) => {
                    state.ui.status_message = format!("Workspace not found: {name}");
                }
                Err(e) => {
                    state.ui.status_message = format!("Delete failed: {e}");
                }
            }
        } else {
            state.ui.status_message = "No database connection".into();
        }
        return Some(());
    }

    if query == "ws-panel" || query == "workspaces" {
        if state.panels.workspace_panel_open {
            state.panels.workspace_panel_open = false;
            state.panels.workspace_entries.clear();
        } else {
            let workspaces = list_workspaces(state);
            state.panels.workspace_entries = workspaces;
            state.panels.workspace_selected = 0;
            state.panels.workspace_panel_open = true;
        }
        return Some(());
    }

    if query == "ws-next" || query == "ws-prev" {
        let workspaces: Vec<String> = list_workspaces(state)
            .into_iter()
            .filter(|w| w.name != "_autosave")
            .map(|w| w.name)
            .collect();
        if workspaces.is_empty() {
            state.ui.status_message = "No saved workspaces.".into();
            return Some(());
        }
        let current_idx = workspaces
            .iter()
            .position(|w| w == &state.current_workspace_name);
        let target = match (query, current_idx) {
            ("ws-next", Some(idx)) => workspaces
                .get((idx + 1) % workspaces.len())
                .cloned()
                .unwrap_or_default(),
            ("ws-prev", Some(idx)) => {
                let prev = if idx == 0 {
                    workspaces.len() - 1
                } else {
                    idx - 1
                };
                workspaces.get(prev).cloned().unwrap_or_default()
            }
            _ => workspaces.first().cloned().unwrap_or_default(),
        };
        state.ui.status_message = format!("Switching to workspace: {target}...");
        state.current_workspace_name = target.clone();
        state.pending_workspace_restore = Some(target);
        return Some(());
    }

    if query == "swap" || query == "tab-swap" {
        swap_panes(state);
        return Some(());
    }

    None
}
