//! Workspace restore logic — rebuild panes from saved workspace data.

use std::collections::HashSet;

use tracing::{info, warn};
use url::Url;

use crate::db::workspaces;
use crate::servo::PaneStateManager;
use crate::terminal::NativeTerminalManager;
use crate::wm::{BspTree, Rect};

pub struct RestoreResult {
    pub pane_count: usize,
    pub panes_to_create: Vec<(uuid::Uuid, Url)>,
}

impl RestoreResult {
    pub fn empty() -> Self {
        Self {
            pane_count: 0,
            panes_to_create: Vec::new(),
        }
    }
}

pub enum RestoreOutcome {
    Restored(RestoreResult),
    NotFound,
    NoDatabase,
    TreeError(String),
}

#[allow(clippy::too_many_arguments)]
pub fn restore_workspace(
    workspace_name: &str,
    viewport: Rect,
    db: Option<&rusqlite::Connection>,
    terminal_pane_ids: &mut HashSet<uuid::Uuid>,
    engines: &mut PaneStateManager,
    wm: &mut BspTree,
    terminal_manager: &mut NativeTerminalManager,
) -> RestoreOutcome {
    let load_result = db.and_then(|conn| workspaces::load_workspace(conn, workspace_name).ok());

    let data = match load_result {
        Some(Some((_ws, data))) => data,
        Some(None) => return RestoreOutcome::NotFound,
        None => return RestoreOutcome::NoDatabase,
    };

    for tid in terminal_manager.pane_ids() {
        terminal_manager.remove(&tid);
    }

    for pid in engines.pane_ids() {
        engines.remove_pane(&pid);
        terminal_pane_ids.remove(&pid);
    }

    let new_tree = match BspTree::from_workspace_data(&data, viewport) {
        Ok(tree) => tree,
        Err(e) => {
            warn!("Workspace tree rebuild failed: {}", e);
            return RestoreOutcome::TreeError(e.to_string());
        }
    };

    let count = new_tree.leaf_count();
    info!(
        "Workspace restored: {} panes, active={}",
        count,
        new_tree.active_pane_id()
    );

    let urls = workspaces::collect_urls(&data.tree);
    let mut panes_to_create = Vec::new();
    for (i, (pid, _rect)) in new_tree.panes().iter().enumerate() {
        let url_str = urls
            .get(i)
            .cloned()
            .unwrap_or_else(|| "aileron://new".into());
        let url = Url::parse(&url_str).unwrap_or_else(|_| Url::parse("aileron://new").unwrap());
        engines.create_pane(*pid, url.clone());

        if url_str == "aileron://terminal" {
            terminal_pane_ids.insert(*pid);
        }

        panes_to_create.push((*pid, url));
    }

    *wm = new_tree;

    RestoreOutcome::Restored(RestoreResult {
        pane_count: count,
        panes_to_create,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_restore_result_empty() {
        let result = RestoreResult::empty();
        assert_eq!(result.pane_count, 0);
        assert!(result.panes_to_create.is_empty());
    }

    #[test]
    fn test_restore_result_with_panes() {
        let id = uuid::Uuid::new_v4();
        let url = Url::parse("https://example.com").unwrap();
        let result = RestoreResult {
            pane_count: 1,
            panes_to_create: vec![(id, url)],
        };
        assert_eq!(result.pane_count, 1);
        assert_eq!(result.panes_to_create.len(), 1);
        assert_eq!(result.panes_to_create[0].0, id);
        assert_eq!(result.panes_to_create[0].1.as_str(), "https://example.com/");
    }

    #[test]
    fn test_restore_outcome_restored() {
        let result = RestoreResult::empty();
        match RestoreOutcome::Restored(result) {
            RestoreOutcome::Restored(r) => assert_eq!(r.pane_count, 0),
            _ => panic!("Expected Restored variant"),
        }
    }

    #[test]
    fn test_restore_outcome_not_found() {
        matches!(RestoreOutcome::NotFound, RestoreOutcome::NotFound);
    }

    #[test]
    fn test_restore_outcome_no_database() {
        matches!(RestoreOutcome::NoDatabase, RestoreOutcome::NoDatabase);
    }

    #[test]
    fn test_restore_outcome_tree_error() {
        match RestoreOutcome::TreeError("bad tree".into()) {
            RestoreOutcome::TreeError(msg) => assert_eq!(msg, "bad tree"),
            _ => panic!("Expected TreeError variant"),
        }
    }

    #[test]
    fn test_restore_result_pane_urls() {
        let id1 = uuid::Uuid::new_v4();
        let id2 = uuid::Uuid::new_v4();
        let url1 = Url::parse("https://example.com").unwrap();
        let url2 = Url::parse("aileron://terminal").unwrap();
        let result = RestoreResult {
            pane_count: 2,
            panes_to_create: vec![(id1, url1), (id2, url2)],
        };
        assert_eq!(result.panes_to_create[0].1.scheme(), "https");
        assert_eq!(result.panes_to_create[1].1.scheme(), "aileron");
    }
}
