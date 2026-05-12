use super::super::AppState;
use tracing::warn;

#[must_use = "ignoring this value may lead to unexpected behavior"]
pub fn cmd_bookmarks(state: &mut AppState, query: &str) -> Option<()> {
    match query {
        "bookmarks" => {
            if state.panels.bookmarks_panel_open {
                state.panels.bookmarks_panel_open = false;
                state.panels.bookmarks_entries.clear();
            } else if let Some(db) = state.db.as_ref() {
                match crate::db::bookmarks::all_bookmarks(db) {
                    Ok(entries) => {
                        state.panels.bookmarks_entries = entries;
                        state.panels.bookmarks_selected = 0;
                        state.panels.bookmarks_panel_open = true;
                    }
                    Err(e) => {
                        state.ui.status_message = format!("Bookmarks error: {e}");
                    }
                }
            }
            Some(())
        }
        "bookmark-clear" => {
            if let Some(db) = state.db.as_ref() {
                match crate::db::bookmarks::clear_bookmarks(db) {
                    Ok(count) => {
                        state.ui.status_message = format!("Cleared {count} bookmarks");
                        state.panels.bookmarks_panel_open = false;
                        state.panels.bookmarks_entries.clear();
                    }
                    Err(e) => {
                        state.ui.status_message = format!("Failed to clear bookmarks: {e}");
                    }
                }
            }
            Some(())
        }
        _ => {
            if let Some(rest) = query.strip_prefix("bookmark ") {
                let rest = rest.trim();
                if rest.is_empty() {
                    state.ui.status_message = "Usage: :bookmark <url> [folder]".into();
                    return Some(());
                }
                let parts: Vec<&str> = rest.rsplitn(2, ' ').collect();
                let (url, folder) = if parts.len() == 2 {
                    (parts[1].trim(), parts[0].trim())
                } else {
                    (parts[0].trim(), "")
                };

                if let Some(db) = state.db.as_ref() {
                    match crate::db::bookmarks::add_bookmark_with_folder(db, url, "", folder) {
                        Ok(id) => {
                            let folder_msg = if folder.is_empty() {
                                String::new()
                            } else {
                                format!(" [{folder}]")
                            };
                            state.ui.status_message =
                                format!("Bookmarked: {url}{folder_msg} (id={id})");
                        }
                        Err(e) => {
                            state.ui.status_message = format!("Bookmark failed: {e}");
                        }
                    }
                }
                return Some(());
            }
            None
        }
    }
}

#[must_use = "ignoring this value may lead to unexpected behavior"]
pub fn handle_quickmark_commands(state: &mut AppState, query: &str) -> Option<()> {
    if let Some(name) = query.strip_prefix("quickmark-add ") {
        let name = name.trim();
        if name.is_empty() {
            state.ui.status_message = "Usage: :quickmark-add <name>".into();
            return Some(());
        }
        let active_id = state.wm.active_pane_id();
        let url_str = state
            .engines
            .get(&active_id)
            .and_then(|e| e.current_url().map(|u| u.to_string()))
            .unwrap_or_default();
        if url_str.is_empty() || url_str == "aileron://welcome" {
            state.ui.status_message = "No URL to quickmark".into();
            return Some(());
        }
        state
            .session
            .quickmarks
            .insert(name.to_string(), url_str.clone());
        if let Some(ref conn) = state.db
            && let Err(e) = crate::db::quickmarks::set_quickmark(conn, name, &url_str)
        {
            tracing::warn!("Failed to persist quickmark {}: {}", name, e);
        }
        state.ui.status_message = format!("Quickmark '{name}' → {url_str}");
        return Some(());
    }

    if let Some(name) = query.strip_prefix("quickmark-del ") {
        let name = name.trim();
        if name.is_empty() {
            state.ui.status_message = "Usage: :quickmark-del <name>".into();
            return Some(());
        }
        if state.session.quickmarks.remove(name).is_some() {
            if let Some(ref conn) = state.db
                && let Err(e) = crate::db::quickmarks::remove_quickmark(conn, name)
            {
                warn!(%e, "Failed to remove quickmark");
            }
            state.ui.status_message = format!("Quickmark '{name}' deleted");
        } else {
            state.ui.status_message = format!("Quickmark '{name}' not found");
        }
        return Some(());
    }

    if query == "quickmark-list" {
        let list = state.quickmarks_list();
        if list.is_empty() {
            state.ui.status_message = "No quickmarks".into();
        } else {
            let items: Vec<String> = list.iter().map(|(k, v)| format!("{k}:{v}")).collect();
            let msg = items.join(" | ");
            let display = if msg.len() > 120 {
                format!("{}...", &msg[..117])
            } else {
                msg
            };
            state.ui.status_message = format!("Quickmarks: {display}");
        }
        return Some(());
    }

    None
}
