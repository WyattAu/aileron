use super::super::AppState;

#[must_use = "ignoring this value may lead to unexpected behavior"]
pub fn handle_tab_commands(state: &mut AppState, query: &str) -> Option<()> {
    if query == "tab-restore" {
        if let Some((url, _title)) = state.tabs.closed_tab_stack.pop_back() {
            if let Ok(parsed) = url::Url::parse(&url) {
                state
                    .pending_wry_actions
                    .push_back(crate::app::WryAction::Navigate(parsed));
                state.ui.status_message = format!("Restored: {url}");
            }
        } else {
            state.ui.status_message = "No closed tabs to restore".into();
        }
        return Some(());
    }

    if let Some(rest) = query.strip_prefix("tab-restore ")
        && let Ok(n) = rest.trim().parse::<usize>()
    {
        if n == 0 {
            if let Some((url, _title)) = state.tabs.closed_tab_stack.pop_back() {
                if let Ok(parsed) = url::Url::parse(&url) {
                    state
                        .pending_wry_actions
                        .push_back(crate::app::WryAction::Navigate(parsed));
                    state.ui.status_message = format!("Restored: {url}");
                }
            } else {
                state.ui.status_message = "No closed tabs to restore".into();
            }
        } else if let Some((url, _title)) = state.tabs.closed_tab_stack.get(n.saturating_sub(1)) {
            let url_clone = url.clone();
            if let Ok(parsed) = url::Url::parse(&url_clone) {
                state
                    .pending_wry_actions
                    .push_back(crate::app::WryAction::Navigate(parsed));
                state.ui.status_message = format!("Restored: {url_clone}");
            }
        } else {
            state.ui.status_message = format!("No closed tab at index {n}");
        }
        return Some(());
    }

    if query == "tab-unload" {
        if let Some(lru_id) = state.find_lru_pane() {
            let panes = state.wm.panes();
            if let Some((_, _)) = panes.iter().find(|(id, _)| *id == lru_id) {
                state.pending_tab_close = Some(lru_id);
                state.ui.status_message = "Unloading least-recently-used pane".into();
            }
        } else {
            state.ui.status_message = "Only one pane open, nothing to unload".into();
        }
        return Some(());
    }

    if query == "tab-rename" || query.starts_with("tab-rename ") {
        let active_id = state.wm.active_pane_id().to_string();
        let name = query.strip_prefix("tab-rename ").unwrap_or("").trim();
        if name.is_empty() {
            state.tabs.tab_names.remove(&active_id);
            if let Some(ref conn) = state.db
                && let Err(e) = crate::db::tab_names::remove_tab_name(conn, &active_id)
            {
                tracing::warn!("Failed to remove tab name: {}", e);
            }
            state.ui.status_message = "Tab name cleared".into();
        } else {
            state
                .tabs
                .tab_names
                .insert(active_id.clone(), name.to_string());
            if let Some(ref conn) = state.db
                && let Err(e) = crate::db::tab_names::set_tab_name(conn, &active_id, name)
            {
                tracing::warn!("Failed to persist tab name: {}", e);
            }
            state.ui.status_message = format!("Tab renamed: {name}");
        }
        return Some(());
    }

    if let Some(dir) = query.strip_prefix("tab-move ") {
        let panes = state.wm.panes();
        if panes.len() < 2 {
            state.ui.status_message = "Only one pane, nowhere to move.".into();
            return Some(());
        }
        let active_id = state.wm.active_pane_id();
        let positions: Vec<_> = panes.iter().map(|(id, _)| *id).collect();
        let current_idx = positions
            .iter()
            .position(|&id| id == active_id)
            .unwrap_or(0);
        let new_idx = match dir.trim() {
            "left" | "prev" => current_idx.saturating_sub(1),
            "right" | "next" => (current_idx + 1) % positions.len(),
            "first" | "start" => 0,
            "last" | "end" => positions.len() - 1,
            n => {
                if let Ok(idx) = n.parse::<usize>() {
                    idx.saturating_sub(1).min(positions.len() - 1)
                } else {
                    state.ui.status_message = "Usage: :tab-move <left|right|first|last|N>".into();
                    return Some(());
                }
            }
        };
        if new_idx != current_idx {
            let target_id = positions[new_idx];
            if state.wm.swap_pane_ids(active_id, target_id) {
                state.ui.status_message = format!(
                    "Swapped pane positions: {} → {}",
                    current_idx + 1,
                    new_idx + 1
                );
            } else {
                state.ui.status_message = "Failed to swap panes.".into();
            }
        } else {
            state.ui.status_message = "Already at target position.".into();
        }
        return Some(());
    }

    None
}
