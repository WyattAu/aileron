use tracing::warn;

use super::super::AppState;

#[must_use = "ignoring this value may lead to unexpected behavior"]
pub fn cmd_history(state: &mut AppState, query: &str) -> Option<()> {
    match query {
        "history" => {
            if state.panels.history_panel_open {
                state.panels.history_panel_open = false;
                state.panels.history_entries.clear();
            } else if let Some(db) = state.db.as_ref() {
                match crate::db::history::recent_entries(db, 100) {
                    Ok(entries) => {
                        state.panels.history_entries = entries;
                        state.panels.history_selected = 0;
                        state.panels.history_panel_open = true;
                    }
                    Err(e) => {
                        state.ui.status_message = format!("History error: {e}");
                    }
                }
            }
            Some(())
        }
        "history-clear" => {
            if let Some(db) = state.db.as_ref() {
                match crate::db::history::clear_history(db) {
                    Ok(count) => {
                        state.ui.status_message = format!("Cleared {count} history entries");
                        state.panels.history_panel_open = false;
                        state.panels.history_entries.clear();
                    }
                    Err(e) => {
                        state.ui.status_message = format!("Failed to clear history: {e}");
                    }
                }
            }
            Some(())
        }
        _ => None,
    }
}

pub fn record_visit(state: &AppState, url: &url::Url, title: &str) {
    if let Some(ref conn) = state.db
        && !state
            .tabs
            .private_pane_ids
            .contains(&state.wm.active_pane_id())
        && let Err(e) = crate::db::history::record_visit(conn, url, title)
    {
        warn!("Failed to record visit: {}", e);
    }
}
