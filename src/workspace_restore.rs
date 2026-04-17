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
