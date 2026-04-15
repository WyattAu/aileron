use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct DownloadEntry {
    pub id: i64,
    pub url: String,
    pub filename: String,
    pub dest_path: String,
    pub started_at: String,
    pub status: String,
}

pub fn record_download(
    conn: &Connection,
    url: &str,
    filename: &str,
    dest_path: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO downloads (url, filename, dest_path) VALUES (?1, ?2, ?3)",
        params![url, filename, dest_path],
    )?;
    Ok(())
}

pub fn mark_completed(conn: &Connection, url: &str) -> Result<()> {
    let id: Option<i64> = conn.query_row(
        "SELECT id FROM downloads WHERE url = ?1 AND status = 'started' ORDER BY id DESC LIMIT 1",
        params![url],
        |row| row.get(0),
    ).ok();
    if let Some(id) = id {
        conn.execute(
            "UPDATE downloads SET status = 'completed' WHERE id = ?1",
            params![id],
        )?;
    }
    Ok(())
}

pub fn recent_downloads(conn: &Connection, limit: usize) -> Result<Vec<DownloadEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, filename, dest_path, started_at, status FROM downloads ORDER BY id DESC LIMIT ?1",
    )?;
    let entries = stmt
        .query_map(params![limit as i64], |row| {
            Ok(DownloadEntry {
                id: row.get(0)?,
                url: row.get(1)?,
                filename: row.get(2)?,
                dest_path: row.get(3)?,
                started_at: row.get(4)?,
                status: row.get(5)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(entries)
}

pub fn clear_downloads(conn: &Connection) -> Result<usize> {
    let count = conn.execute("DELETE FROM downloads", [])?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS downloads (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                filename TEXT NOT NULL,
                dest_path TEXT NOT NULL,
                started_at TEXT NOT NULL DEFAULT (datetime('now')),
                status TEXT NOT NULL DEFAULT 'started'
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_record_and_list() {
        let conn = test_db();
        record_download(
            &conn,
            "https://example.com/file.pdf",
            "file.pdf",
            "/home/user/Downloads/file.pdf",
        )
        .unwrap();
        record_download(
            &conn,
            "https://example.com/image.png",
            "image.png",
            "/home/user/Downloads/image.png",
        )
        .unwrap();
        let entries = recent_downloads(&conn, 10).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].filename, "image.png");
    }

    #[test]
    fn test_mark_completed() {
        let conn = test_db();
        record_download(
            &conn,
            "https://example.com/file.pdf",
            "file.pdf",
            "/home/user/Downloads/file.pdf",
        )
        .unwrap();
        mark_completed(&conn, "https://example.com/file.pdf").unwrap();
        let entries = recent_downloads(&conn, 10).unwrap();
        assert_eq!(entries[0].status, "completed");
    }

    #[test]
    fn test_clear_downloads() {
        let conn = test_db();
        record_download(&conn, "https://a.com/f.pdf", "f.pdf", "/d/f.pdf").unwrap();
        record_download(&conn, "https://b.com/g.png", "g.png", "/d/g.png").unwrap();
        let count = clear_downloads(&conn).unwrap();
        assert_eq!(count, 2);
        assert!(recent_downloads(&conn, 10).unwrap().is_empty());
    }
}
