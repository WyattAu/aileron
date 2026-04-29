//! Browser data import: Firefox and Chrome bookmarks/history.
//!
//! Free functions that take `&Connection` and return status messages.

use crate::db::bookmarks;

/// Import bookmarks and history from Firefox profiles.
/// Returns a status message describing what was imported.
pub fn import_firefox(db: &rusqlite::Connection) -> String {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return "Cannot determine HOME directory".into(),
    };

    let firefox_dir = std::path::Path::new(&home).join(".mozilla/firefox");
    if !firefox_dir.exists() {
        return "Firefox data not found (~/.mozilla/firefox)".into();
    }

    let mut bookmarks_imported = 0usize;
    let mut history_imported = 0usize;

    let profiles: Vec<std::path::PathBuf> = std::fs::read_dir(&firefox_dir)
        .ok()
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map(|n| n.ends_with(".default") || n.contains(".default-"))
                        .unwrap_or(false)
                })
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();

    for profile_dir in &profiles {
        let bk_dir = profile_dir.join("bookmarkbackups");
        if bk_dir.exists()
            && let Ok(entries) = std::fs::read_dir(&bk_dir)
        {
            let mut backups: Vec<std::path::PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e == "json" || e == "html")
                        .unwrap_or(false)
                })
                .collect();
            backups.sort();
            backups.reverse();
            if let Some(latest) = backups.first() {
                let ext = latest.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "json" {
                    bookmarks_imported += import_firefox_bookmarks_json(db, latest);
                } else {
                    bookmarks_imported += import_firefox_bookmarks_html(db, latest);
                }
            }
        }

        let places_path = profile_dir.join("places.sqlite");
        if places_path.exists() {
            history_imported += import_firefox_history(db, &places_path);
        }
    }

    format!(
        "Firefox import: {} bookmarks, {} history entries",
        bookmarks_imported, history_imported
    )
}

fn import_firefox_bookmarks_json(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(j) => j,
        Err(_) => return 0,
    };
    let mut count = 0usize;
    walk_firefox_json_bookmarks(&json, db, &mut count);
    count
}

fn walk_firefox_json_bookmarks(
    node: &serde_json::Value,
    db: &rusqlite::Connection,
    count: &mut usize,
) {
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            if child.get("type").and_then(|t| t.as_str()) == Some("text/x-moz-place")
                && let (Some(url), Some(title)) = (
                    child.get("uri").and_then(|u| u.as_str()),
                    child.get("title").and_then(|t| t.as_str()),
                )
                && url.starts_with("http")
                && bookmarks::import_bookmark(db, url, title).unwrap_or(false)
            {
                *count += 1;
            }
            walk_firefox_json_bookmarks(child, db, count);
        }
    }
}

fn import_firefox_bookmarks_html(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let mut count = 0usize;
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("<DT><A ")
            && let Some(href_start) = rest.find("HREF=\"")
            && let Some(href_end) = rest[href_start + 6..].find('"')
        {
            let after_href = &rest[href_start + 6..];
            let url = &after_href[..href_end];
            let title = after_href[href_end + 1..]
                .find('>')
                .and_then(|gt| {
                    let after_gt = &after_href[gt + 1..];
                    after_gt.find("</A>").map(|end| &after_gt[..end])
                })
                .unwrap_or(url);
            if url.starts_with("http")
                && bookmarks::import_bookmark(db, url, title).unwrap_or(false)
            {
                count += 1;
            }
        }
    }
    count
}

fn import_firefox_history(db: &rusqlite::Connection, places_path: &std::path::Path) -> usize {
    let conn = match rusqlite::Connection::open_with_flags(
        places_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let mut stmt = match conn.prepare(
        "SELECT p.url, p.title, h.visit_date
         FROM moz_places p
         JOIN moz_historyvisits h ON p.id = h.place_id
         WHERE p.url LIKE 'http%' AND h.visit_type IN (1, 2)
         ORDER BY h.visit_date DESC
         LIMIT 500",
    ) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1).unwrap_or_default(),
            row.get::<_, i64>(2).unwrap_or(0),
        ))
    }) {
        Ok(r) => r,
        Err(_) => return 0,
    };
    let mut count = 0usize;
    for row in rows.filter_map(|r| r.ok()) {
        let (url, title, visit_date) = row;
        let visited_at = if visit_date > 0 {
            let epoch_us = visit_date / 1000;
            let secs = epoch_us / 1_000_000;
            chrono::DateTime::from_timestamp(secs, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        if crate::db::history::import_visit(db, &url, &title, &visited_at).unwrap_or(false) {
            count += 1;
        }
    }
    count
}

/// Import bookmarks and history from Chrome.
/// Returns a status message describing what was imported.
pub fn import_chrome(db: &rusqlite::Connection) -> String {
    let chrome_dirs: Vec<std::path::PathBuf> = {
        #[cfg(target_os = "windows")]
        {
            let local = std::env::var("LOCALAPPDATA")
                .unwrap_or_else(|_| r"C:\Users\Default\AppData\Local".into());
            vec![
                std::path::PathBuf::from(&local)
                    .join("Google")
                    .join("Chrome")
                    .join("User Data")
                    .join("Default"),
                std::path::PathBuf::from(&local)
                    .join("Chromium")
                    .join("User Data")
                    .join("Default"),
            ]
        }
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            vec![
                std::path::PathBuf::from(&home)
                    .join("Library")
                    .join("Application Support")
                    .join("Google")
                    .join("Chrome")
                    .join("Default"),
                std::path::PathBuf::from(&home)
                    .join("Library")
                    .join("Application Support")
                    .join("Chromium")
                    .join("Default"),
            ]
        }
        #[cfg(target_os = "linux")]
        {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            vec![
                std::path::PathBuf::from(&home)
                    .join(".config")
                    .join("google-chrome")
                    .join("Default"),
                std::path::PathBuf::from(&home)
                    .join(".config")
                    .join("chromium")
                    .join("Default"),
                std::path::PathBuf::from(&home)
                    .join(".var")
                    .join("app")
                    .join("com.google.Chrome")
                    .join("config")
                    .join("google-chrome")
                    .join("Default"),
            ]
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            vec![]
        }
    };

    let mut chrome_dir = None;
    for dir in &chrome_dirs {
        if dir.exists() {
            chrome_dir = Some(dir.clone());
            break;
        }
    }
    let chrome_dir = match chrome_dir {
        Some(d) => d,
        None => return "Chrome data not found (checked standard paths for your platform)".into(),
    };

    let bookmarks_path = chrome_dir.join("Bookmarks");
    let history_path = chrome_dir.join("History");

    let mut bookmarks_imported = 0usize;
    let mut history_imported = 0usize;

    if bookmarks_path.exists() {
        bookmarks_imported = import_chrome_bookmarks(db, &bookmarks_path);
    }

    if history_path.exists() {
        history_imported = import_chrome_history(db, &history_path);
    }

    format!(
        "Chrome import: {} bookmarks, {} history entries",
        bookmarks_imported, history_imported
    )
}

fn import_chrome_bookmarks(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(j) => j,
        Err(_) => return 0,
    };
    let mut count = 0usize;
    if let Some(roots) = json.get("roots").and_then(|r| r.as_object()) {
        for (_key, node) in roots {
            walk_chrome_bookmark_node(node, db, &mut count);
        }
    }
    count
}

fn walk_chrome_bookmark_node(
    node: &serde_json::Value,
    db: &rusqlite::Connection,
    count: &mut usize,
) {
    if node.get("type").and_then(|t| t.as_str()) == Some("url")
        && let (Some(url), Some(name)) = (
            node.get("url").and_then(|u| u.as_str()),
            node.get("name").and_then(|n| n.as_str()),
        )
        && url.starts_with("http")
        && bookmarks::import_bookmark(db, url, name).unwrap_or(false)
    {
        *count += 1;
        return;
    }
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            walk_chrome_bookmark_node(child, db, count);
        }
    }
}

fn import_chrome_history(db: &rusqlite::Connection, path: &std::path::Path) -> usize {
    let conn = match rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let mut stmt = match conn.prepare(
        "SELECT u.url, u.title, v.visit_time
         FROM urls u
         JOIN visits v ON u.id = v.url
         ORDER BY v.visit_time DESC
         LIMIT 500",
    ) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1).unwrap_or_default(),
            row.get::<_, i64>(2).unwrap_or(0),
        ))
    }) {
        Ok(r) => r,
        Err(_) => return 0,
    };
    let mut count = 0usize;
    for row in rows.filter_map(|r| r.ok()) {
        let (url, title, visit_time) = row;
        let visited_at = if visit_time > 0 {
            let epoch_us = visit_time / 1000;
            let secs = epoch_us / 1_000_000;
            chrono::DateTime::from_timestamp(secs, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        if crate::db::history::import_visit(db, &url, &title, &visited_at).unwrap_or(false) {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS bookmarks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL DEFAULT '',
                visited_at TEXT NOT NULL DEFAULT (datetime('now')),
                visit_count INTEGER NOT NULL DEFAULT 1
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_import_firefox_no_data() {
        let db = test_db();
        // HOME points to real dir but no Firefox data expected in test env
        let result = import_firefox(&db);
        assert!(result.contains("Firefox"));
    }

    #[test]
    fn test_import_chrome_no_data() {
        let db = test_db();
        let result = import_chrome(&db);
        assert!(result.contains("Chrome"));
    }

    #[test]
    fn test_firefox_bookmarks_html_parsing() {
        let db = test_db();
        let html = r#"<DT><A HREF="https://example.com" ADD_DATE="0">Example</A>"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), html).unwrap();
        let count = import_firefox_bookmarks_html(&db, tmp.path());
        assert_eq!(count, 1);
    }

    #[test]
    fn test_chrome_bookmarks_json_parsing() {
        let db = test_db();
        let json = r#"{
            "roots": {
                "bookmark_bar": {
                    "type": "url",
                    "url": "https://example.com",
                    "name": "Example"
                },
                "other": {
                    "children": [
                        {
                            "type": "url",
                            "url": "https://httpbin.org",
                            "name": "HTTPBin"
                        }
                    ]
                }
            }
        }"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), json).unwrap();
        let count = import_chrome_bookmarks(&db, tmp.path());
        assert_eq!(count, 2);
    }

    #[test]
    fn test_firefox_bookmarks_json_empty() {
        let db = test_db();
        let json = r#"{"checksum": "abc", "roots": {}}"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), json).unwrap();
        let count = import_firefox_bookmarks_json(&db, tmp.path());
        assert_eq!(count, 0);
    }
}
