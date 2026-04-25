use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;

/// Save or update a tab name for a pane.
pub fn set_tab_name(conn: &Connection, pane_id: &str, name: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO tab_names (pane_id, name, updated_at) VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(pane_id) DO UPDATE SET name = excluded.name, updated_at = datetime('now')",
        rusqlite::params![pane_id, name],
    )?;
    Ok(())
}

/// Remove a tab name by pane ID.
pub fn remove_tab_name(conn: &Connection, pane_id: &str) -> Result<bool> {
    let rows = conn.execute(
        "DELETE FROM tab_names WHERE pane_id = ?1",
        rusqlite::params![pane_id],
    )?;
    Ok(rows > 0)
}

/// Load all tab names from the database.
pub fn load_tab_names(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT pane_id, name FROM tab_names")?;
    let rows = stmt.query_map([], |row| {
        let pane_id: String = row.get(0)?;
        let name: String = row.get(1)?;
        Ok((pane_id, name))
    })?;

    let mut map = HashMap::new();
    for row in rows {
        let (pane_id, name) = row?;
        map.insert(pane_id, name);
    }
    Ok(map)
}

/// Clear all tab names (for session reset).
pub fn clear_all_tab_names(conn: &Connection) -> Result<usize> {
    let rows = conn.execute("DELETE FROM tab_names", [])?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tab_names (
                pane_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_set_and_load_tab_names() {
        let db = test_db();
        set_tab_name(&db, "abc-123", "GitHub").unwrap();
        set_tab_name(&db, "def-456", "Gmail").unwrap();

        let names = load_tab_names(&db).unwrap();
        assert_eq!(names.len(), 2);
        assert_eq!(names.get("abc-123").unwrap(), "GitHub");
    }

    #[test]
    fn test_set_tab_name_upsert() {
        let db = test_db();
        set_tab_name(&db, "abc-123", "Old").unwrap();
        set_tab_name(&db, "abc-123", "New").unwrap();

        let names = load_tab_names(&db).unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names.get("abc-123").unwrap(), "New");
    }

    #[test]
    fn test_remove_tab_name() {
        let db = test_db();
        set_tab_name(&db, "abc-123", "GitHub").unwrap();
        assert!(remove_tab_name(&db, "abc-123").unwrap());
        assert!(!remove_tab_name(&db, "nonexistent").unwrap());
    }
}
