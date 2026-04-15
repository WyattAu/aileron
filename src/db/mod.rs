pub mod bookmarks;
pub mod downloads;
pub mod history;
pub mod workspaces;

use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

pub fn open_database(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            url TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL DEFAULT '',
            visited_at TEXT NOT NULL DEFAULT (datetime('now')),
            visit_count INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS bookmarks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            url TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_history_url ON history(url);
        CREATE INDEX IF NOT EXISTS idx_history_visited ON history(visited_at DESC);

        CREATE TABLE IF NOT EXISTS workspaces (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            data TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS downloads (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            url TEXT NOT NULL,
            filename TEXT NOT NULL,
            dest_path TEXT NOT NULL,
            started_at TEXT NOT NULL DEFAULT (datetime('now')),
            status TEXT NOT NULL DEFAULT 'started'
        );",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: open an in-memory database with schema initialized.
    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn test_schema_creates_history_table() {
        let db = test_db();
        let mut stmt = db
            .prepare("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='history'")
            .unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_schema_creates_bookmarks_table() {
        let db = test_db();
        let mut stmt = db
            .prepare("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='bookmarks'")
            .unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_schema_creates_workspaces_table() {
        let db = test_db();
        let mut stmt = db
            .prepare("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='workspaces'")
            .unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_schema_history_columns() {
        let db = test_db();
        let mut stmt = db
            .prepare("SELECT url, title, visited_at, visit_count FROM history")
            .unwrap();
        // Just verify the columns exist by preparing the statement (no rows needed)
        let _: String = stmt.query_row([], |row| row.get(0)).unwrap_or_default();
    }

    #[test]
    fn test_schema_bookmarks_columns() {
        let db = test_db();
        let mut stmt = db
            .prepare("SELECT url, title, created_at FROM bookmarks")
            .unwrap();
        let _: String = stmt.query_row([], |row| row.get(0)).unwrap_or_default();
    }

    #[test]
    fn test_schema_history_url_unique() {
        let db = test_db();
        // Insert same URL twice — second should fail due to UNIQUE constraint
        db.execute(
            "INSERT INTO history (url, title) VALUES ('https://example.com', 'Example')",
            [],
        )
        .unwrap();
        let result = db.execute(
            "INSERT INTO history (url, title) VALUES ('https://example.com', 'Dup')",
            [],
        );
        assert!(
            result.is_err(),
            "Duplicate URL should violate UNIQUE constraint"
        );
    }

    #[test]
    fn test_schema_idempotent() {
        // Running init_schema twice should not fail
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        init_schema(&conn).unwrap(); // second call — IF NOT EXISTS should handle it
    }

    #[test]
    fn test_open_database_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = open_database(&db_path).unwrap();
        // Verify we can query the tables
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM sqlite_master WHERE type='table'")
            .unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert!(count >= 3, "Should have at least 3 tables");
    }
}
