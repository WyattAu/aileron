//! Workspace persistence — save and restore pane layouts + URLs.
//!
//! A workspace captures the BSP tree structure (split directions + ratios)
//! and each pane's URL. Rectangles are NOT saved since they depend on
//! window size — they are recomputed from the viewport on restore.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tracing::warn;

/// A saved workspace entry in the database.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: i64,
    pub name: String,
    pub data: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Serializable workspace data — the BSP layout + pane URLs.
///
/// This is what gets stored as JSON in the `workspaces.data` column.
/// Rectangles are not stored (they're recomputed from the viewport on restore).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceData {
    /// The BSP tree structure (directions + ratios), without position info.
    pub tree: WorkspaceNode,
    /// URL of the active pane at save time.
    pub active_url: String,
}

/// A node in the workspace's BSP tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkspaceNode {
    /// A leaf pane with its URL.
    Leaf { url: String },
    /// An internal split with direction and ratio.
    Split {
        direction: SplitDir,
        ratio: f64,
        left: Box<WorkspaceNode>,
        right: Box<WorkspaceNode>,
    },
}

/// Split direction (serializable).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SplitDir {
    Horizontal,
    Vertical,
}

impl WorkspaceData {
    /// Serialize this workspace data to JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize workspace: {}", e))
    }

    /// Deserialize workspace data from JSON.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize workspace: {}", e))
    }
}

/// Save a workspace. If a workspace with the same name exists, it is updated.
pub fn save_workspace(conn: &Connection, name: &str, data: &WorkspaceData) -> Result<i64> {
    let json = data.to_json()?;
    conn.execute(
        "INSERT INTO workspaces (name, data, updated_at) VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(name) DO UPDATE SET data = excluded.data, updated_at = datetime('now')",
        params![name, json],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Load a workspace by name. Returns None if not found.
pub fn load_workspace(conn: &Connection, name: &str) -> Result<Option<(Workspace, WorkspaceData)>> {
    let mut stmt = conn
        .prepare("SELECT id, name, data, created_at, updated_at FROM workspaces WHERE name = ?1")?;

    let result = stmt.query_row(params![name], |row| {
        Ok(Workspace {
            id: row.get(0)?,
            name: row.get(1)?,
            data: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
        })
    });

    match result {
        Ok(workspace) => {
            let data = WorkspaceData::from_json(&workspace.data)?;
            Ok(Some((workspace, data)))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// List all workspaces, ordered by last updated (newest first).
pub fn list_workspaces(conn: &Connection) -> Result<Vec<Workspace>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, data, created_at, updated_at FROM workspaces ORDER BY updated_at DESC",
    )?;
    let workspaces = stmt
        .query_map([], |row| {
            Ok(Workspace {
                id: row.get(0)?,
                name: row.get(1)?,
                data: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
        .filter_map(|r| {
            if let Err(e) = &r {
                warn!("Error reading workspace: {}", e);
            }
            r.ok()
        })
        .collect();
    Ok(workspaces)
}

/// Delete a workspace by name. Returns true if it existed.
pub fn delete_workspace(conn: &Connection, name: &str) -> Result<bool> {
    let rows = conn.execute("DELETE FROM workspaces WHERE name = ?1", params![name])?;
    Ok(rows > 0)
}

/// Extract all pane URLs from a WorkspaceNode in order (depth-first left-to-right).
/// The order matches the order panes are created during restoration.
pub fn collect_urls(node: &WorkspaceNode) -> Vec<String> {
    match node {
        WorkspaceNode::Leaf { url } => vec![url.clone()],
        WorkspaceNode::Split { left, right, .. } => {
            let mut urls = collect_urls(left);
            urls.extend(collect_urls(right));
            urls
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS workspaces (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                data TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        conn
    }

    fn sample_data() -> WorkspaceData {
        WorkspaceData {
            tree: WorkspaceNode::Split {
                direction: SplitDir::Vertical,
                ratio: 0.5,
                left: Box::new(WorkspaceNode::Leaf {
                    url: "https://example.com".into(),
                }),
                right: Box::new(WorkspaceNode::Split {
                    direction: SplitDir::Horizontal,
                    ratio: 0.6,
                    left: Box::new(WorkspaceNode::Leaf {
                        url: "https://rust-lang.org".into(),
                    }),
                    right: Box::new(WorkspaceNode::Leaf {
                        url: "https://github.com".into(),
                    }),
                }),
            },
            active_url: "https://rust-lang.org".into(),
        }
    }

    #[test]
    fn test_workspace_data_serde_roundtrip() {
        let data = sample_data();
        let json = data.to_json().unwrap();
        let restored = WorkspaceData::from_json(&json).unwrap();
        assert_eq!(data.active_url, restored.active_url);
    }

    #[test]
    fn test_save_and_load() {
        let conn = test_db();
        let data = sample_data();

        save_workspace(&conn, "dev-layout", &data).unwrap();

        let (ws, loaded) = load_workspace(&conn, "dev-layout").unwrap().unwrap();
        assert_eq!(ws.name, "dev-layout");
        assert_eq!(loaded.active_url, "https://rust-lang.org");
    }

    #[test]
    fn test_save_upsert() {
        let conn = test_db();

        let data1 = WorkspaceData {
            tree: WorkspaceNode::Leaf {
                url: "https://a.com".into(),
            },
            active_url: "https://a.com".into(),
        };
        save_workspace(&conn, "test", &data1).unwrap();

        let data2 = WorkspaceData {
            tree: WorkspaceNode::Leaf {
                url: "https://b.com".into(),
            },
            active_url: "https://b.com".into(),
        };
        save_workspace(&conn, "test", &data2).unwrap();

        // Should have updated, not duplicated
        let all = list_workspaces(&conn).unwrap();
        assert_eq!(all.len(), 1);
        let (_, loaded) = load_workspace(&conn, "test").unwrap().unwrap();
        assert_eq!(loaded.active_url, "https://b.com");
    }

    #[test]
    fn test_load_nonexistent() {
        let conn = test_db();
        let result = load_workspace(&conn, "nonexistent");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_list_workspaces() {
        let conn = test_db();
        save_workspace(&conn, "ws1", &sample_data()).unwrap();
        save_workspace(&conn, "ws2", &sample_data()).unwrap();
        save_workspace(&conn, "ws3", &sample_data()).unwrap();

        let all = list_workspaces(&conn).unwrap();
        assert_eq!(all.len(), 3);
        let names: Vec<&str> = all.iter().map(|w| w.name.as_str()).collect();
        assert!(names.contains(&"ws1"));
        assert!(names.contains(&"ws2"));
        assert!(names.contains(&"ws3"));
    }

    #[test]
    fn test_delete_workspace() {
        let conn = test_db();
        save_workspace(&conn, "to-delete", &sample_data()).unwrap();
        assert!(load_workspace(&conn, "to-delete").unwrap().is_some());

        let deleted = delete_workspace(&conn, "to-delete").unwrap();
        assert!(deleted);
        assert!(load_workspace(&conn, "to-delete").unwrap().is_none());

        // Deleting non-existent returns false
        let deleted = delete_workspace(&conn, "to-delete").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_collect_urls() {
        let data = sample_data();
        let urls = collect_urls(&data.tree);
        assert_eq!(urls.len(), 3);
        assert_eq!(urls[0], "https://example.com");
        assert_eq!(urls[1], "https://rust-lang.org");
        assert_eq!(urls[2], "https://github.com");
    }

    #[test]
    fn test_single_pane_roundtrip() {
        let data = WorkspaceData {
            tree: WorkspaceNode::Leaf {
                url: "https://single.com".into(),
            },
            active_url: "https://single.com".into(),
        };
        let json = data.to_json().unwrap();
        let restored = WorkspaceData::from_json(&json).unwrap();
        assert_eq!(restored.active_url, "https://single.com");
    }
}
