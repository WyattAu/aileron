//! Bookmarks CRUD operations.
//!
//! Bookmarks are stored in the SQLite `bookmarks` table (created in db::init_schema).

use anyhow::Result;
use rusqlite::{Connection, params};
use tracing::warn;

/// A bookmark entry.
#[derive(Debug, Clone)]
pub struct Bookmark {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub folder: String,
    pub created_at: String,
}

/// Add a bookmark. Returns the new bookmark's ID.
/// If the URL is already bookmarked, updates the title and folder.
pub fn add_bookmark(conn: &Connection, url: &str, title: &str) -> Result<i64> {
    add_bookmark_with_folder(conn, url, title, "")
}

/// Add a bookmark with an optional folder.
pub fn add_bookmark_with_folder(
    conn: &Connection,
    url: &str,
    title: &str,
    folder: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO bookmarks (url, title, folder) VALUES (?1, ?2, ?3)
         ON CONFLICT(url) DO UPDATE SET title = excluded.title, folder = excluded.folder",
        params![url, title, folder],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Set the folder for a bookmark by ID.
pub fn set_bookmark_folder(conn: &Connection, id: i64, folder: &str) -> Result<bool> {
    let rows = conn.execute(
        "UPDATE bookmarks SET folder = ?1 WHERE id = ?2",
        params![folder, id],
    )?;
    Ok(rows > 0)
}

/// Get all distinct folder names.
pub fn list_folders(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT DISTINCT folder FROM bookmarks WHERE folder != '' ORDER BY folder")?;
    let folders = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(folders)
}

/// Remove a bookmark by URL.
pub fn remove_bookmark(conn: &Connection, url: &str) -> Result<bool> {
    let rows = conn.execute("DELETE FROM bookmarks WHERE url = ?1", params![url])?;
    Ok(rows > 0)
}

/// Remove a bookmark by ID.
pub fn remove_bookmark_by_id(conn: &Connection, id: i64) -> Result<bool> {
    let rows = conn.execute("DELETE FROM bookmarks WHERE id = ?1", params![id])?;
    Ok(rows > 0)
}

/// Check if a URL is bookmarked.
pub fn is_bookmarked(conn: &Connection, url: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM bookmarks WHERE url = ?1",
        params![url],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

/// Get all bookmarks, ordered by creation date (newest first).
pub fn all_bookmarks(conn: &Connection) -> Result<Vec<Bookmark>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, title, folder, created_at FROM bookmarks ORDER BY folder, created_at DESC",
    )?;
    let bookmarks = stmt
        .query_map([], |row| {
            Ok(Bookmark {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                folder: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .filter_map(|r| {
            if let Err(e) = &r {
                warn!("Error reading bookmark: {}", e);
            }
            r.ok()
        })
        .collect();
    Ok(bookmarks)
}

/// Search bookmarks by query string (matches URL or title).
pub fn search_bookmarks(conn: &Connection, query: &str, limit: usize) -> Result<Vec<Bookmark>> {
    let pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT id, url, title, folder, created_at FROM bookmarks
         WHERE url LIKE ?1 OR title LIKE ?1
         ORDER BY created_at DESC
         LIMIT ?2",
    )?;
    let bookmarks = stmt
        .query_map(params![pattern, limit as i64], |row| {
            Ok(Bookmark {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                folder: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(bookmarks)
}

/// Clear all bookmarks. Returns the number of entries deleted.
pub fn clear_bookmarks(conn: &Connection) -> Result<usize> {
    let count = conn.execute("DELETE FROM bookmarks", [])?;
    Ok(count)
}

/// Add a bookmark only if the URL doesn't already exist.
/// Returns true if inserted, false if duplicate.
pub fn import_bookmark(conn: &Connection, url: &str, title: &str) -> Result<bool> {
    if is_bookmarked(conn, url) {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO bookmarks (url, title) VALUES (?1, ?2)",
        params![url, title],
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS bookmarks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL DEFAULT '',
                folder TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_add_and_list_bookmarks() {
        let conn = test_db();
        add_bookmark(&conn, "https://example.com", "Example").unwrap();
        add_bookmark(&conn, "https://rust-lang.org", "Rust").unwrap();

        let bookmarks = all_bookmarks(&conn).unwrap();
        assert_eq!(bookmarks.len(), 2);
        let urls: Vec<&str> = bookmarks.iter().map(|b| b.url.as_str()).collect();
        assert!(urls.contains(&"https://example.com"));
        assert!(urls.contains(&"https://rust-lang.org"));
    }

    #[test]
    fn test_is_bookmarked() {
        let conn = test_db();
        assert!(!is_bookmarked(&conn, "https://example.com"));

        add_bookmark(&conn, "https://example.com", "Example").unwrap();
        assert!(is_bookmarked(&conn, "https://example.com"));
    }

    #[test]
    fn test_remove_bookmark() {
        let conn = test_db();
        add_bookmark(&conn, "https://example.com", "Example").unwrap();
        assert!(is_bookmarked(&conn, "https://example.com"));

        let removed = remove_bookmark(&conn, "https://example.com").unwrap();
        assert!(removed);
        assert!(!is_bookmarked(&conn, "https://example.com"));

        // Removing non-existent returns false
        let removed = remove_bookmark(&conn, "https://example.com").unwrap();
        assert!(!removed);
    }

    #[test]
    fn test_upsert_bookmark() {
        let conn = test_db();
        add_bookmark(&conn, "https://example.com", "Old Title").unwrap();
        add_bookmark(&conn, "https://example.com", "New Title").unwrap();

        let bookmarks = all_bookmarks(&conn).unwrap();
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].title, "New Title");
    }

    #[test]
    fn test_search_bookmarks() {
        let conn = test_db();
        add_bookmark(&conn, "https://rust-lang.org", "Rust Programming").unwrap();
        add_bookmark(&conn, "https://example.com", "Example Domain").unwrap();
        add_bookmark(&conn, "https://github.com", "GitHub").unwrap();

        let results = search_bookmarks(&conn, "rust", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://rust-lang.org");

        let results = search_bookmarks(&conn, "a", 10).unwrap();
        assert_eq!(results.len(), 2); // "Rust" and "GitHub" both match
    }

    #[test]
    fn test_search_limit() {
        let conn = test_db();
        for i in 0..10 {
            add_bookmark(
                &conn,
                &format!("https://example{}.com", i),
                &format!("Site {}", i),
            )
            .unwrap();
        }

        let results = search_bookmarks(&conn, "example", 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_clear_bookmarks() {
        let conn = test_db();
        add_bookmark(&conn, "https://example.com", "Example").unwrap();
        add_bookmark(&conn, "https://rust-lang.org", "Rust").unwrap();
        assert_eq!(all_bookmarks(&conn).unwrap().len(), 2);

        let count = clear_bookmarks(&conn).unwrap();
        assert_eq!(count, 2);
        assert_eq!(all_bookmarks(&conn).unwrap().len(), 0);
    }

    #[test]
    fn test_import_bookmark_skips_duplicate() {
        let conn = test_db();
        add_bookmark(&conn, "https://example.com", "Example").unwrap();

        let inserted = import_bookmark(&conn, "https://example.com", "Example (dup)").unwrap();
        assert!(!inserted);

        let inserted = import_bookmark(&conn, "https://new.com", "New").unwrap();
        assert!(inserted);

        assert_eq!(all_bookmarks(&conn).unwrap().len(), 2);
    }
}
