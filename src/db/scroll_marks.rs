use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;

/// Save or update a scroll mark (url + letter → fraction).
pub fn set_scroll_mark(conn: &Connection, url: &str, letter: char, fraction: f64) -> Result<()> {
    conn.execute(
        "INSERT INTO scroll_marks (url, letter, fraction) VALUES (?1, ?2, ?3)
         ON CONFLICT(url, letter) DO UPDATE SET fraction = excluded.fraction",
        rusqlite::params![url, letter.to_string(), fraction],
    )?;
    Ok(())
}

/// Remove a scroll mark by URL and letter.
pub fn remove_scroll_mark(conn: &Connection, url: &str, letter: char) -> Result<bool> {
    let rows = conn.execute(
        "DELETE FROM scroll_marks WHERE url = ?1 AND letter = ?2",
        rusqlite::params![url, letter.to_string()],
    )?;
    Ok(rows > 0)
}

/// Load all scroll marks for a given URL.
pub fn load_scroll_marks_for_url(conn: &Connection, url: &str) -> Result<HashMap<char, f64>> {
    let mut stmt = conn.prepare("SELECT letter, fraction FROM scroll_marks WHERE url = ?1")?;
    let rows = stmt.query_map(rusqlite::params![url], |row| {
        let letter_str: String = row.get(0)?;
        let fraction: f64 = row.get(1)?;
        Ok((letter_str, fraction))
    })?;

    let mut map = HashMap::new();
    for row in rows {
        let (letter_str, fraction) = row?;
        if let Some(ch) = letter_str.chars().next() {
            map.insert(ch, fraction);
        }
    }
    Ok(map)
}

/// Remove all scroll marks for a URL.
pub fn clear_scroll_marks_for_url(conn: &Connection, url: &str) -> Result<usize> {
    let rows = conn.execute(
        "DELETE FROM scroll_marks WHERE url = ?1",
        rusqlite::params![url],
    )?;
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
            "CREATE TABLE IF NOT EXISTS scroll_marks (
                url TEXT NOT NULL,
                letter TEXT NOT NULL,
                fraction REAL NOT NULL,
                PRIMARY KEY (url, letter)
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_set_and_load_scroll_marks() {
        let db = test_db();
        set_scroll_mark(&db, "https://example.com", 'a', 0.5).unwrap();
        set_scroll_mark(&db, "https://example.com", 'b', 0.75).unwrap();

        let marks = load_scroll_marks_for_url(&db, "https://example.com").unwrap();
        assert_eq!(marks.len(), 2);
        assert_eq!(marks.get(&'a'), Some(&0.5));
        assert_eq!(marks.get(&'b'), Some(&0.75));
    }

    #[test]
    fn test_set_scroll_mark_upsert() {
        let db = test_db();
        set_scroll_mark(&db, "https://example.com", 'a', 0.5).unwrap();
        set_scroll_mark(&db, "https://example.com", 'a', 0.9).unwrap();

        let marks = load_scroll_marks_for_url(&db, "https://example.com").unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks.get(&'a'), Some(&0.9));
    }

    #[test]
    fn test_different_urls_independent() {
        let db = test_db();
        set_scroll_mark(&db, "https://a.com", 'x', 0.1).unwrap();
        set_scroll_mark(&db, "https://b.com", 'x', 0.9).unwrap();

        let a = load_scroll_marks_for_url(&db, "https://a.com").unwrap();
        let b = load_scroll_marks_for_url(&db, "https://b.com").unwrap();
        assert_eq!(a.get(&'x'), Some(&0.1));
        assert_eq!(b.get(&'x'), Some(&0.9));
    }

    #[test]
    fn test_remove_scroll_mark() {
        let db = test_db();
        set_scroll_mark(&db, "https://example.com", 'a', 0.5).unwrap();
        assert!(remove_scroll_mark(&db, "https://example.com", 'a').unwrap());
        assert!(!remove_scroll_mark(&db, "https://example.com", 'z').unwrap());

        let marks = load_scroll_marks_for_url(&db, "https://example.com").unwrap();
        assert!(marks.is_empty());
    }

    #[test]
    fn test_clear_scroll_marks_for_url() {
        let db = test_db();
        set_scroll_mark(&db, "https://example.com", 'a', 0.5).unwrap();
        set_scroll_mark(&db, "https://example.com", 'b', 0.75).unwrap();
        set_scroll_mark(&db, "https://other.com", 'a', 0.1).unwrap();

        assert_eq!(clear_scroll_marks_for_url(&db, "https://example.com").unwrap(), 2);
        let marks = load_scroll_marks_for_url(&db, "https://example.com").unwrap();
        assert!(marks.is_empty());

        // Other URL's marks unaffected
        let other = load_scroll_marks_for_url(&db, "https://other.com").unwrap();
        assert_eq!(other.len(), 1);
    }
}
