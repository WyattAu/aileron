use super::super::AppState;

/// U1-04: Session manager commands.
///
/// Sessions save and restore workspace states with metadata including
/// BSP tree structure, pane URLs, scroll positions, and timestamps.
///
/// Sessions reuse the workspaces table with a "session:" prefix to
/// maintain compatibility with existing workspace infrastructure.
pub fn handle_session_commands(state: &mut AppState, query: &str) -> Option<()> {
    if let Some(name) = query.strip_prefix("session-save ") {
        let name = name.trim();
        if name.is_empty() {
            state.ui.status_message = "Usage: :session-save <name>".into();
            return Some(());
        }
        let session_name = format!("session:{name}");
        state
            .pending_wry_actions
            .push_back(crate::app::WryAction::SaveWorkspace {
                name: session_name,
                pane_urls: std::collections::HashMap::new(),
            });
        state.ui.status_message = format!("Saving session: {name}...");
        return Some(());
    }

    if let Some(name) = query.strip_prefix("session-load ") {
        let name = name.trim();
        if name.is_empty() {
            state.ui.status_message = "Usage: :session-load <name>".into();
            return Some(());
        }
        let session_name = format!("session:{name}");
        state.pending_workspace_restore = Some(session_name);
        state.ui.status_message = format!("Loading session: {name}...");
        return Some(());
    }

    if query == "session-list" {
        let workspaces = list_sessions(state);
        if workspaces.is_empty() {
            state.ui.status_message = "No saved sessions.".into();
        } else {
            let names: Vec<String> = workspaces
                .iter()
                .map(|w| {
                    let display_name = w.name.strip_prefix("session:").unwrap_or(&w.name);
                    format!("{display_name} ({})", w.updated_at)
                })
                .collect();
            state.ui.status_message = format!("Sessions: {}", names.join(", "));
        }
        return Some(());
    }

    if let Some(name) = query.strip_prefix("session-delete ") {
        let name = name.trim();
        if name.is_empty() {
            state.ui.status_message = "Usage: :session-delete <name>".into();
            return Some(());
        }
        let session_name = format!("session:{name}");
        if let Some(db) = state.db.as_ref() {
            match crate::db::workspaces::delete_workspace(db, &session_name) {
                Ok(true) => {
                    state.ui.status_message = format!("Session deleted: {name}");
                }
                Ok(false) => {
                    state.ui.status_message = format!("Session not found: {name}");
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

    None
}

/// List all saved sessions from the database.
fn list_sessions(state: &AppState) -> Vec<crate::db::workspaces::Workspace> {
    state
        .db
        .as_ref()
        .and_then(|conn| crate::db::workspaces::list_workspaces(conn).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|w| w.name.starts_with("session:"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::wm::Rect;

    fn make_state() -> AppState {
        let viewport = Rect::new(0.0, 0.0, 800.0, 600.0);
        AppState::new(viewport, Config::default()).unwrap()
    }

    #[test]
    fn test_session_save_empty_name() {
        let mut state = make_state();
        handle_session_commands(&mut state, "session-save ");
        assert_eq!(state.ui.status_message, "Usage: :session-save <name>");
    }

    #[test]
    fn test_session_load_empty_name() {
        let mut state = make_state();
        handle_session_commands(&mut state, "session-load ");
        assert_eq!(state.ui.status_message, "Usage: :session-load <name>");
    }

    #[test]
    fn test_session_delete_empty_name() {
        let mut state = make_state();
        handle_session_commands(&mut state, "session-delete ");
        assert_eq!(state.ui.status_message, "Usage: :session-delete <name>");
    }

    #[test]
    fn test_session_list_empty() {
        let mut state = make_state();
        handle_session_commands(&mut state, "session-list");
        assert_eq!(state.ui.status_message, "No saved sessions.");
    }

    #[test]
    fn test_session_save_queues_action() {
        let mut state = make_state();
        handle_session_commands(&mut state, "session-save my-session");
        assert!(
            state
                .pending_wry_actions
                .iter()
                .any(|a| matches!(a, crate::app::WryAction::SaveWorkspace { name, .. } if name == "session:my-session"))
        );
    }

    #[test]
    fn test_session_load_sets_pending_restore() {
        let mut state = make_state();
        handle_session_commands(&mut state, "session-load my-session");
        assert_eq!(
            state.pending_workspace_restore.as_deref(),
            Some("session:my-session")
        );
    }

    #[test]
    fn test_session_delete_no_db() {
        let mut state = make_state();
        state.db = None;
        handle_session_commands(&mut state, "session-delete my-session");
        assert_eq!(state.ui.status_message, "No database connection");
    }

    #[test]
    fn test_unknown_session_command_returns_none() {
        let mut state = make_state();
        let result = handle_session_commands(&mut state, "unknown-command");
        assert!(result.is_none());
    }
}
