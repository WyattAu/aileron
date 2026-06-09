use super::super::AppState;
use crate::ui::search::{FuzzySearch, SearchCategory, SearchItem};

#[must_use = "ignoring this value may lead to unexpected behavior"]
pub fn handle_tab_commands(state: &mut AppState, query: &str) -> Option<()> {
    // U1-03: Tab search across all open tabs
    if let Some(search_query) = query.strip_prefix("tabs-search ") {
        let search_query = search_query.trim();
        if search_query.is_empty() {
            state.ui.status_message = "Usage: :tabs-search <query>".into();
            return Some(());
        }

        // Collect all open tabs from all panes
        let mut search = FuzzySearch::new();
        for (pane_id, _) in state.wm.panes_ref().iter() {
            if let Some(pane_data) = state.wm.find_pane(*pane_id) {
                for tab in pane_data.tabs.iter() {
                    let pane_short = &pane_id.to_string()[..8];
                    search.upsert(SearchItem {
                        id: tab.id.to_string(),
                        label: tab.title.clone(),
                        description: format!("{} - Pane {}", tab.url, pane_short),
                        category: SearchCategory::OpenTab,
                    });
                }
            }
        }

        let results = search.search(search_query, 10);
        if results.is_empty() {
            state.ui.status_message = format!("No tabs matching: {search_query}");
        } else {
            let mut messages: Vec<String> = results
                .iter()
                .take(5)
                .map(|item| format!("{} ({})", item.label, item.description))
                .collect();
            if results.len() > 5 {
                messages.push(format!("+{} more", results.len() - 5));
            }
            state.ui.status_message = messages.join(" │ ");
        }
        return Some(());
    }

    // U1-01: Tab-within-pane commands
    if query == "tab-new-in-pane" {
        let active_id = state.wm.active_pane_id();
        if let Some(root) = state.wm.root_mut() {
            if let Some(pane) = crate::wm::BspTree::find_pane_mut(root, active_id) {
                let new_url = url::Url::parse("aileron://new").unwrap();
                let tab_id = pane.tabs.add(new_url.clone());
                state
                    .pending_wry_actions
                    .push_back(crate::app::WryAction::Navigate(new_url));
                state.ui.status_message = format!(
                    "New tab {} in pane {}",
                    &tab_id.to_string()[..8],
                    &active_id.to_string()[..8]
                );
            } else {
                state.ui.status_message = "No active pane".into();
            }
        }
        return Some(());
    }

    if query == "tab-close-in-pane" {
        let active_id = state.wm.active_pane_id();
        if let Some(root) = state.wm.root_mut() {
            if let Some(pane) = crate::wm::BspTree::find_pane_mut(root, active_id) {
                if pane.tabs.is_single() {
                    state.ui.status_message =
                        "Cannot close last tab in pane (use :q to close pane)".into();
                } else {
                    let closed_tab = pane.tabs.close_active();
                    if let Some(tab) = closed_tab {
                        // Navigate to the new active tab
                        let new_active_url = pane.url().clone();
                        state
                            .pending_wry_actions
                            .push_back(crate::app::WryAction::Navigate(new_active_url.clone()));
                        state.ui.status_message = format!("Closed tab: {}", tab.title);
                    }
                }
            } else {
                state.ui.status_message = "No active pane".into();
            }
        }
        return Some(());
    }

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
        let active_id = state.wm.active_pane_id();
        let name = query.strip_prefix("tab-rename ").unwrap_or("").trim();
        if name.is_empty() {
            state.tabs.tab_names.remove(&active_id);
            if let Some(ref conn) = state.db
                && let Err(e) = crate::db::tab_names::remove_tab_name(conn, &active_id.to_string())
            {
                tracing::warn!("Failed to remove tab name: {}", e);
            }
            state.ui.status_message = "Tab name cleared".into();
        } else {
            state.tabs.tab_names.insert(active_id, name.to_string());
            if let Some(ref conn) = state.db
                && let Err(e) =
                    crate::db::tab_names::set_tab_name(conn, &active_id.to_string(), name)
            {
                tracing::warn!("Failed to persist tab name: {}", e);
            }
            state.ui.status_message = format!("Tab renamed: {name}");
        }
        return Some(());
    }

    if query == "tab-move-left" {
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
        if current_idx == 0 {
            state.ui.status_message = "Already at first position.".into();
        } else {
            let target_id = positions[current_idx - 1];
            if state.wm.swap_pane_ids(active_id, target_id) {
                state.ui.status_message =
                    format!("Moved left: {} → {}", current_idx + 1, current_idx);
            } else {
                state.ui.status_message = "Failed to move pane.".into();
            }
        }
        return Some(());
    }

    if query == "tab-move-right" {
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
        if current_idx >= positions.len() - 1 {
            state.ui.status_message = "Already at last position.".into();
        } else {
            let target_id = positions[current_idx + 1];
            if state.wm.swap_pane_ids(active_id, target_id) {
                state.ui.status_message =
                    format!("Moved right: {} → {}", current_idx + 1, current_idx + 2);
            } else {
                state.ui.status_message = "Failed to move pane.".into();
            }
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
