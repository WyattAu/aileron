use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;

pub fn set_quickmark(conn: &Connection, key: &str, url: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO quickmarks (letter, url, created_at) VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(letter) DO UPDATE SET url = excluded.url",
        rusqlite::params![key, url],
    )?;
    Ok(())
}

pub fn remove_quickmark(conn: &Connection, key: &str) -> Result<bool> {
    let rows = conn.execute(
        "DELETE FROM quickmarks WHERE letter = ?1",
        rusqlite::params![key],
    )?;
    Ok(rows > 0)
}

pub fn load_quickmarks(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT letter, url FROM quickmarks")?;
    let rows = stmt.query_map([], |row| {
        let key: String = row.get(0)?;
        let url: String = row.get(1)?;
        Ok((key, url))
    })?;

    let mut map = HashMap::new();
    for row in rows {
        let (key, url) = row?;
        map.insert(key, url);
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS quickmarks (
                letter TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_set_and_load_quickmarks() {
        let db = test_db();
        set_quickmark(&db, "a", "https://example.com").unwrap();
        set_quickmark(&db, "b", "https://google.com").unwrap();

        let marks = load_quickmarks(&db).unwrap();
        assert_eq!(marks.len(), 2);
        assert_eq!(marks.get("a").unwrap(), "https://example.com");
        assert_eq!(marks.get("b").unwrap(), "https://google.com");
    }

    #[test]
    fn test_set_quickmark_upsert() {
        let db = test_db();
        set_quickmark(&db, "a", "https://example.com").unwrap();
        set_quickmark(&db, "a", "https://updated.com").unwrap();

        let marks = load_quickmarks(&db).unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks.get("a").unwrap(), "https://updated.com");
    }

    #[test]
    fn test_remove_quickmark() {
        let db = test_db();
        set_quickmark(&db, "a", "https://example.com").unwrap();
        assert!(remove_quickmark(&db, "a").unwrap());
        assert!(!remove_quickmark(&db, "z").unwrap());

        let marks = load_quickmarks(&db).unwrap();
        assert!(marks.is_empty());
    }

    #[test]
    fn test_load_empty_quickmarks() {
        let db = test_db();
        let marks = load_quickmarks(&db).unwrap();
        assert!(marks.is_empty());
    }

    #[test]
    fn test_multi_char_key_quickmarks() {
        let db = test_db();
        set_quickmark(&db, "gh", "https://github.com").unwrap();
        set_quickmark(&db, "gl", "https://gitlab.com").unwrap();
        set_quickmark(&db, "rd", "https://reddit.com").unwrap();

        let marks = load_quickmarks(&db).unwrap();
        assert_eq!(marks.len(), 3);
        assert_eq!(marks.get("gh").unwrap(), "https://github.com");
        assert_eq!(marks.get("gl").unwrap(), "https://gitlab.com");
        assert_eq!(marks.get("rd").unwrap(), "https://reddit.com");
    }
}
