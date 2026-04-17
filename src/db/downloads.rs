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
    pub progress_percent: i64,
    pub total_bytes: i64,
    pub received_bytes: i64,
    pub mime_type: String,
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

pub fn record_download_with_metadata(
    conn: &Connection,
    url: &str,
    filename: &str,
    dest_path: &str,
    mime_type: &str,
    total_bytes: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO downloads (url, filename, dest_path, mime_type, total_bytes) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![url, filename, dest_path, mime_type, total_bytes],
    )?;
    Ok(())
}

pub fn update_download_progress(
    conn: &Connection,
    url: &str,
    progress_percent: i64,
    received_bytes: i64,
) -> Result<()> {
    let id: Option<i64> = conn.query_row(
        "SELECT id FROM downloads WHERE url = ?1 AND status = 'started' ORDER BY id DESC LIMIT 1",
        params![url],
        |row| row.get(0),
    ).ok();
    if let Some(id) = id {
        conn.execute(
            "UPDATE downloads SET progress_percent = ?1, received_bytes = ?2 WHERE id = ?3",
            params![progress_percent, received_bytes, id],
        )?;
    }
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
            "UPDATE downloads SET status = 'completed', progress_percent = 100 WHERE id = ?1",
            params![id],
        )?;
    }
    Ok(())
}

pub fn recent_downloads(conn: &Connection, limit: usize) -> Result<Vec<DownloadEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, filename, dest_path, started_at, status, progress_percent, total_bytes, received_bytes, mime_type FROM downloads ORDER BY id DESC LIMIT ?1",
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
                progress_percent: row.get(6)?,
                total_bytes: row.get(7)?,
                received_bytes: row.get(8)?,
                mime_type: row.get(9)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(entries)
}

pub fn get_latest_download_id(conn: &Connection) -> Result<i64> {
    let id: i64 = conn.query_row(
        "SELECT id FROM downloads ORDER BY id DESC LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    Ok(id)
}

pub fn get_download_dest_path(conn: &Connection, id: i64) -> Result<String> {
    let dest: String = conn.query_row(
        "SELECT dest_path FROM downloads WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    Ok(dest)
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
                status TEXT NOT NULL DEFAULT 'started',
                progress_percent INTEGER NOT NULL DEFAULT 0,
                total_bytes INTEGER NOT NULL DEFAULT 0,
                received_bytes INTEGER NOT NULL DEFAULT 0,
                mime_type TEXT NOT NULL DEFAULT ''
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

    #[test]
    fn test_record_with_metadata() {
        let conn = test_db();
        record_download_with_metadata(
            &conn,
            "https://example.com/file.pdf",
            "file.pdf",
            "/home/user/Downloads/file.pdf",
            "application/pdf",
            1024000,
        )
        .unwrap();
        let entries = recent_downloads(&conn, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].mime_type, "application/pdf");
        assert_eq!(entries[0].total_bytes, 1024000);
        assert_eq!(entries[0].progress_percent, 0);
    }

    #[test]
    fn test_update_progress() {
        let conn = test_db();
        record_download(
            &conn,
            "https://example.com/big.zip",
            "big.zip",
            "/home/user/Downloads/big.zip",
        )
        .unwrap();
        update_download_progress(&conn, "https://example.com/big.zip", 50, 512000).unwrap();
        let entries = recent_downloads(&conn, 10).unwrap();
        assert_eq!(entries[0].progress_percent, 50);
        assert_eq!(entries[0].received_bytes, 512000);
    }

    #[test]
    fn test_mark_completed_sets_progress_100() {
        let conn = test_db();
        record_download(
            &conn,
            "https://example.com/doc.pdf",
            "doc.pdf",
            "/home/user/Downloads/doc.pdf",
        )
        .unwrap();
        mark_completed(&conn, "https://example.com/doc.pdf").unwrap();
        let entries = recent_downloads(&conn, 10).unwrap();
        assert_eq!(entries[0].status, "completed");
        assert_eq!(entries[0].progress_percent, 100);
    }

    #[test]
    fn test_get_latest_download_id() {
        let conn = test_db();
        record_download(&conn, "https://a.com/f1", "f1", "/d/f1").unwrap();
        record_download(&conn, "https://b.com/f2", "f2", "/d/f2").unwrap();
        let id = get_latest_download_id(&conn).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_get_download_dest_path() {
        let conn = test_db();
        record_download(
            &conn,
            "https://example.com/file.txt",
            "file.txt",
            "/home/user/Downloads/file.txt",
        )
        .unwrap();
        let id = get_latest_download_id(&conn).unwrap();
        let dest = get_download_dest_path(&conn, id).unwrap();
        assert_eq!(dest, "/home/user/Downloads/file.txt");
    }
}
