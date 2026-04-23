use anyhow::Result;
use rusqlite::{params, Connection};
use url::Url;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub visited_at: String,
    pub visit_count: i64,
}

impl HistoryEntry {
    pub fn url(&self) -> Result<Url> {
        Url::parse(&self.url).map_err(Into::into)
    }
}

pub fn record_visit(conn: &Connection, url: &Url, title: &str) -> Result<()> {
    let url_str = url.as_str();
    conn.execute(
        "INSERT INTO history (url, title, visited_at) VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(url) DO UPDATE SET visit_count = visit_count + 1, visited_at = datetime('now'), title = ?2",
        params![url_str, title],
    )?;
    Ok(())
}

pub fn recent_entries(conn: &Connection, limit: usize) -> Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, title, visited_at, visit_count FROM history ORDER BY id DESC LIMIT ?1",
    )?;
    let entries = stmt
        .query_map(params![limit as i64], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                visited_at: row.get(3)?,
                visit_count: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<HistoryEntry>, rusqlite::Error>>()?;
    Ok(entries)
}

pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<HistoryEntry>> {
    let pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT id, url, title, visited_at, visit_count FROM history
         WHERE url LIKE ?1 OR title LIKE ?1
         ORDER BY visit_count DESC, visited_at DESC LIMIT ?2",
    )?;
    let entries = stmt
        .query_map(params![pattern, limit as i64], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                visited_at: row.get(3)?,
                visit_count: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<HistoryEntry>, rusqlite::Error>>()?;
    Ok(entries)
}

/// Search history with frecency ranking.
/// Score = visit_count / log2(age_in_hours + 2).
/// Returns entries sorted by frecency score (highest first).
pub fn search_frecency(conn: &Connection, query: &str, limit: usize) -> Result<Vec<(HistoryEntry, f64)>> {
    // Fetch more candidates than needed, then rank and truncate
    let pattern = format!("%{}%", query);
    let candidate_limit = (limit * 5).max(50);
    let mut stmt = conn.prepare(
        "SELECT id, url, title, visited_at, visit_count FROM history
         WHERE url LIKE ?1 OR title LIKE ?1
         ORDER BY visit_count DESC, visited_at DESC LIMIT ?2",
    )?;
    let entries: Vec<HistoryEntry> = stmt
        .query_map(params![pattern, candidate_limit as i64], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                url: row.get(1)?,
                title: row.get(2)?,
                visited_at: row.get(3)?,
                visit_count: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let now_ts = chrono::Utc::now().timestamp();
    let mut scored: Vec<(HistoryEntry, f64)> = entries
        .into_iter()
        .map(|entry| {
            // Parse visited_at as datetime and compute age in hours
            let age_hours = chrono::DateTime::parse_from_rfc3339(&format!("{}T00:00:00Z", entry.visited_at))
                .ok()
                .map(|dt| (now_ts - dt.timestamp()).max(1) as f64 / 3600.0)
                .unwrap_or(720.0); // default 30 days if parsing fails
            let visit_count = entry.visit_count.max(1) as f64;
            let frecency = visit_count / (age_hours + 2.0).log2().max(1.0);
            (entry, frecency)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored)
}

pub fn prune_old(conn: &Connection, days: u32) -> Result<usize> {
    let deleted = conn.execute(
        "DELETE FROM history WHERE visited_at < datetime('now', ?1)",
        params![format!("-{} days", days)],
    )?;
    Ok(deleted)
}

pub fn clear_history(conn: &Connection) -> Result<usize> {
    let count = conn.execute("DELETE FROM history", [])?;
    Ok(count)
}

/// Insert a history entry only if the URL doesn't already exist.
/// Used for importing from other browsers (skip duplicates).
/// Returns true if inserted, false if duplicate.
pub fn import_visit(conn: &Connection, url: &str, title: &str, visited_at: &str) -> Result<bool> {
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM history WHERE url = ?1",
            params![url],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;
    if exists {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO history (url, title, visited_at) VALUES (?1, ?2, ?3)",
        params![url, title, visited_at],
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn setup_db() -> Connection {
        let file = NamedTempFile::new().unwrap();
        let conn = crate::db::open_database(file.path()).unwrap();
        conn
    }

    #[test]
    fn test_record_and_retrieve() {
        let conn = setup_db();
        let url = Url::parse("https://example.com").unwrap();
        record_visit(&conn, &url, "Example").unwrap();

        let entries = recent_entries(&conn, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].url, "https://example.com/");
        assert_eq!(entries[0].title, "Example");
        assert_eq!(entries[0].visit_count, 1);
    }

    #[test]
    fn test_duplicate_url_increments_count() {
        let conn = setup_db();
        let url = Url::parse("https://example.com").unwrap();
        record_visit(&conn, &url, "Example").unwrap();
        record_visit(&conn, &url, "Example Updated").unwrap();

        let entries = recent_entries(&conn, 10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].visit_count, 2);
        assert_eq!(entries[0].title, "Example Updated");
    }

    #[test]
    fn test_search() {
        let conn = setup_db();
        record_visit(
            &conn,
            &Url::parse("https://github.com/servo/servo").unwrap(),
            "Servo",
        )
        .unwrap();
        record_visit(
            &conn,
            &Url::parse("https://example.com").unwrap(),
            "Example",
        )
        .unwrap();
        record_visit(
            &conn,
            &Url::parse("https://github.com/rust-lang/rust").unwrap(),
            "Rust",
        )
        .unwrap();

        let results = search(&conn, "github", 10).unwrap();
        assert_eq!(results.len(), 2);

        let results = search(&conn, "example", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_recent_ordering() {
        let conn = setup_db();
        record_visit(&conn, &Url::parse("https://a.com").unwrap(), "A").unwrap();
        record_visit(&conn, &Url::parse("https://b.com").unwrap(), "B").unwrap();
        record_visit(&conn, &Url::parse("https://c.com").unwrap(), "C").unwrap();

        let entries = recent_entries(&conn, 10).unwrap();
        assert_eq!(entries.len(), 3);
        // Most recent first: C was recorded last
        assert!(entries[0].url.contains("c.com"));
        assert!(entries[1].url.contains("b.com"));
        assert!(entries[2].url.contains("a.com"));
    }

    #[test]
    fn test_limit() {
        let conn = setup_db();
        for i in 0..5 {
            record_visit(
                &conn,
                &Url::parse(&format!("https://{}.com", i)).unwrap(),
                &i.to_string(),
            )
            .unwrap();
        }
        let entries = recent_entries(&conn, 3).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_clear_history() {
        let conn = setup_db();
        record_visit(&conn, &Url::parse("https://a.com").unwrap(), "A").unwrap();
        record_visit(&conn, &Url::parse("https://b.com").unwrap(), "B").unwrap();
        assert_eq!(recent_entries(&conn, 10).unwrap().len(), 2);

        let count = clear_history(&conn).unwrap();
        assert_eq!(count, 2);
        assert_eq!(recent_entries(&conn, 10).unwrap().len(), 0);
    }

    #[test]
    fn test_import_visit_skips_duplicate() {
        let conn = setup_db();
        record_visit(
            &conn,
            &Url::parse("https://example.com/").unwrap(),
            "Example",
        )
        .unwrap();

        let inserted =
            import_visit(&conn, "https://example.com/", "Dup", "2024-01-01 00:00:00").unwrap();
        assert!(!inserted);

        let inserted =
            import_visit(&conn, "https://new.com/", "New", "2024-01-01 00:00:00").unwrap();
        assert!(inserted);

        assert_eq!(recent_entries(&conn, 10).unwrap().len(), 2);
    }
}
