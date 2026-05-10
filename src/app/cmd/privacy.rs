use super::super::AppState;

#[must_use = "ignoring this value may lead to unexpected behavior"]
pub fn cmd_clear_privacy(state: &mut AppState, query: &str) -> Option<()> {
    if let Some(subcmd) = query.strip_prefix("clear ") {
        let subcmd = subcmd.trim();
        match subcmd {
            "history" => {
                if let Some(db) = state.db.as_ref() {
                    match crate::db::history::clear_history(db) {
                        Ok(count) => {
                            state.ui.status_message = format!("Cleared {count} history entries")
                        }
                        Err(e) => state.ui.status_message = format!("Failed: {e}"),
                    }
                }
            }
            "bookmarks" => {
                if let Some(db) = state.db.as_ref() {
                    match crate::db::bookmarks::clear_bookmarks(db) {
                        Ok(count) => state.ui.status_message = format!("Cleared {count} bookmarks"),
                        Err(e) => state.ui.status_message = format!("Failed: {e}"),
                    }
                }
            }
            "workspaces" => {
                let workspaces = state.list_workspaces();
                if let Some(db) = state.db.as_ref() {
                    for ws in &workspaces {
                        if let Err(e) = crate::db::workspaces::delete_workspace(db, &ws.name) {
                            tracing::warn!("Failed to delete workspace '{}': {}", ws.name, e);
                        }
                    }
                }
                state.ui.status_message = format!("Cleared {} workspaces", workspaces.len());
            }
            "cookies" => {
                state.pending_wry_actions.push_back(crate::app::WryAction::RunJs(
                    "document.cookie.split(';').forEach(function(c) { document.cookie = c.trim().split('=')[0] + '=;expires=Thu, 01 Jan 1970 00:00:00 GMT;path=/'; }); 'Cookies cleared'".into(),
                ));
                state.ui.status_message = "Cookies cleared for current pane".into();
            }
            "all" => {
                let mut parts = Vec::new();
                if let Some(db) = state.db.as_ref() {
                    if let Ok(c) = crate::db::history::clear_history(db) {
                        parts.push(format!("{c} history"));
                    }
                    if let Ok(c) = crate::db::bookmarks::clear_bookmarks(db) {
                        parts.push(format!("{c} bookmarks"));
                    }
                    let ws = state.list_workspaces();
                    for w in &ws {
                        if let Err(e) = crate::db::workspaces::delete_workspace(db, &w.name) {
                            tracing::warn!("Failed to delete workspace '{}': {}", w.name, e);
                        }
                    }
                    parts.push(format!("{} workspaces", ws.len()));
                }
                state.ui.status_message = format!("Cleared: {}", parts.join(", "));
            }
            _ => {
                state.ui.status_message =
                    "Usage: :clear history|bookmarks|workspaces|cookies|all".into();
            }
        }
        return Some(());
    }

    if query == "privacy" {
        let https = state.config.https_upgrade_enabled;
        let tracking = state.config.tracking_protection_enabled;
        let adblock = state.config.adblock_enabled;
        state.ui.status_message = format!(
            "HTTPS upgrade: {} | Tracking protection: {} | Adblock: {}",
            if https { "ON" } else { "OFF" },
            if tracking { "ON" } else { "OFF" },
            if adblock { "ON" } else { "OFF" },
        );
        return Some(());
    }

    if query == "https-toggle" {
        let active_id = state.wm.active_pane_id();
        if let Some(engine) = state.engines.get(&active_id)
            && let Some(url) = engine.current_url()
            && let Some(host) = url.host_str()
        {
            let host_lower = host.to_lowercase();
            let safe_list = state.get_cached_https_safe_list();
            if crate::net::privacy::is_https_safe(&host_lower, &safe_list) {
                state.ui.status_message =
                    format!("HTTPS upgrade: {host_lower} is already in the safe list");
            } else {
                state.ui.status_message = format!(
                    "HTTPS upgrade: {} is not in the safe list ({} domains loaded)",
                    host_lower,
                    safe_list.len()
                );
            }
        } else {
            state.ui.status_message = "No active page URL".into();
        }
        return Some(());
    }

    if query == "cookies-clear" {
        state.pending_wry_actions.push_back(crate::app::WryAction::RunJs(
            "document.cookie.split(';').forEach(function(c) { document.cookie = c.trim().split('=')[0] + '=;expires=Thu, 01 Jan 1970 00:00:00 GMT;path=/'; }); 'Cookies cleared'".into(),
        ));
        state.ui.status_message = "Cookies cleared for current pane".into();
        return Some(());
    }

    None
}
