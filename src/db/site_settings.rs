//! Per-site settings stored in SQLite.
//!
//! Each setting row is a URL pattern (exact, wildcard, or regex) with optional
//! overrides for zoom, adblock, javascript, cookies, and autoplay.
//! NULL values mean "use global default from Config".

use anyhow::Result;
use regex::Regex;
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use tracing::warn;

static REGEX_CACHE: LazyLock<Mutex<HashMap<String, Regex>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn cached_regex(pattern: &str) -> Option<Regex> {
    let mut cache = REGEX_CACHE.lock().ok()?;
    if let Some(re) = cache.get(pattern) {
        Some(re.clone())
    } else {
        match Regex::new(pattern) {
            Ok(re) => {
                cache.insert(pattern.to_string(), re.clone());
                Some(re)
            }
            Err(_) => None,
        }
    }
}

fn invalidate_regex_cache() {
    if let Ok(mut cache) = REGEX_CACHE.lock() {
        cache.clear();
    }
}

/// A per-site setting entry.
#[derive(Debug, Clone)]
pub struct SiteSettings {
    pub id: i64,
    pub pattern: String,
    pub pattern_type: String,
    pub zoom_level: Option<f64>,
    pub adblock_enabled: Option<bool>,
    pub javascript_enabled: Option<bool>,
    pub cookies_enabled: Option<bool>,
    pub autoplay_enabled: Option<bool>,
    pub created_at: String,
}

/// Insert or update a site setting. If a setting with the same pattern and
/// pattern_type already exists, the given fields are merged (NULL fields are
/// left unchanged).
#[allow(clippy::too_many_arguments)]
pub fn upsert_site_setting(
    conn: &Connection,
    pattern: &str,
    pattern_type: &str,
    zoom_level: Option<f64>,
    adblock_enabled: Option<bool>,
    javascript_enabled: Option<bool>,
    cookies_enabled: Option<bool>,
    autoplay_enabled: Option<bool>,
) -> Result<()> {
    invalidate_regex_cache();
    conn.execute(
        "INSERT INTO site_settings (pattern, pattern_type, zoom_level, adblock_enabled, javascript_enabled, cookies_enabled, autoplay_enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(pattern, pattern_type) DO UPDATE SET
             zoom_level = COALESCE(?3, zoom_level),
             adblock_enabled = COALESCE(?4, adblock_enabled),
             javascript_enabled = COALESCE(?5, javascript_enabled),
             cookies_enabled = COALESCE(?6, cookies_enabled),
             autoplay_enabled = COALESCE(?7, autoplay_enabled)",
        params![
            pattern,
            pattern_type,
            zoom_level,
            adblock_enabled.map(|b| b as i32),
            javascript_enabled.map(|b| b as i32),
            cookies_enabled.map(|b| b as i32),
            autoplay_enabled.map(|b| b as i32),
        ],
    )?;
    Ok(())
}

/// Set a single field for a site setting. Creates the row if it doesn't exist.
pub fn set_site_field(
    conn: &Connection,
    pattern: &str,
    pattern_type: &str,
    field: &str,
    value: Option<&str>,
) -> Result<()> {
    invalidate_regex_cache();
    match (field, value) {
        ("zoom", Some(v)) => {
            let z: f64 = v.parse().unwrap_or(1.0);
            conn.execute(
                "INSERT INTO site_settings (pattern, pattern_type, zoom_level) VALUES (?1, ?2, ?3)
                 ON CONFLICT(pattern, pattern_type) DO UPDATE SET zoom_level = ?3",
                params![pattern, pattern_type, z],
            )?;
        }
        ("adblock", Some(v)) => {
            let b: bool = !v.contains("off") && !v.contains("false") && !v.contains("0");
            conn.execute(
                "INSERT INTO site_settings (pattern, pattern_type, adblock_enabled) VALUES (?1, ?2, ?3)
                 ON CONFLICT(pattern, pattern_type) DO UPDATE SET adblock_enabled = ?3",
                params![pattern, pattern_type, b as i32],
            )?;
        }
        ("javascript" | "js", Some(v)) => {
            let b: bool = !v.contains("off") && !v.contains("false") && !v.contains("0");
            conn.execute(
                "INSERT INTO site_settings (pattern, pattern_type, javascript_enabled) VALUES (?1, ?2, ?3)
                 ON CONFLICT(pattern, pattern_type) DO UPDATE SET javascript_enabled = ?3",
                params![pattern, pattern_type, b as i32],
            )?;
        }
        ("cookies", Some(v)) => {
            let b: bool = !v.contains("off") && !v.contains("false") && !v.contains("0");
            conn.execute(
                "INSERT INTO site_settings (pattern, pattern_type, cookies_enabled) VALUES (?1, ?2, ?3)
                 ON CONFLICT(pattern, pattern_type) DO UPDATE SET cookies_enabled = ?3",
                params![pattern, pattern_type, b as i32],
            )?;
        }
        ("autoplay", Some(v)) => {
            let b: bool = !v.contains("off") && !v.contains("false") && !v.contains("0");
            conn.execute(
                "INSERT INTO site_settings (pattern, pattern_type, autoplay_enabled) VALUES (?1, ?2, ?3)
                 ON CONFLICT(pattern, pattern_type) DO UPDATE SET autoplay_enabled = ?3",
                params![pattern, pattern_type, b as i32],
            )?;
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown field or missing value: {} {:?}",
                field,
                value
            ));
        }
    };
    Ok(())
}

/// Get all site settings that match a given URL.
/// Matches exact patterns first, then wildcard (*.example.com), then regex.
pub fn get_site_settings_for_url(conn: &Connection, url: &str) -> Result<Vec<SiteSettings>> {
    let host = extract_host(url);
    let mut results = Vec::new();

    let mut stmt = conn.prepare(
        "SELECT id, pattern, pattern_type, zoom_level, adblock_enabled, javascript_enabled, cookies_enabled, autoplay_enabled, created_at
         FROM site_settings
         WHERE pattern_type = 'exact' AND ?1 = pattern
            OR pattern_type = 'wildcard' AND ?1 LIKE REPLACE(pattern, '*', '%')
         ORDER BY pattern_type ASC, id ASC",
    )?;

    let rows = stmt.query_map(params![host], |row| {
        Ok(SiteSettings {
            id: row.get(0)?,
            pattern: row.get(1)?,
            pattern_type: row.get(2)?,
            zoom_level: row.get(3)?,
            adblock_enabled: row.get::<_, Option<i32>>(4)?.map(|v| v != 0),
            javascript_enabled: row.get::<_, Option<i32>>(5)?.map(|v| v != 0),
            cookies_enabled: row.get::<_, Option<i32>>(6)?.map(|v| v != 0),
            autoplay_enabled: row.get::<_, Option<i32>>(7)?.map(|v| v != 0),
            created_at: row.get(8)?,
        })
    })?;

    for row in rows {
        match row {
            Ok(s) => results.push(s),
            Err(e) => warn!("Error reading site setting: {}", e),
        }
    }

    // Regex patterns
    let mut stmt = conn.prepare(
        "SELECT id, pattern, pattern_type, zoom_level, adblock_enabled, javascript_enabled, cookies_enabled, autoplay_enabled, created_at
         FROM site_settings
         WHERE pattern_type = 'regex'
         ORDER BY id ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<f64>>(3)?,
            row.get::<_, Option<i32>>(4)?,
            row.get::<_, Option<i32>>(5)?,
            row.get::<_, Option<i32>>(6)?,
            row.get::<_, Option<i32>>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;

    for row in rows {
        match row {
            Ok((
                id,
                pattern,
                pattern_type,
                zoom_level,
                adblock_enabled,
                javascript_enabled,
                cookies_enabled,
                autoplay_enabled,
                created_at,
            )) => {
                if let Some(re) = cached_regex(&pattern)
                    && re.is_match(&host)
                {
                    results.push(SiteSettings {
                        id,
                        pattern,
                        pattern_type,
                        zoom_level,
                        adblock_enabled: adblock_enabled.map(|v| v != 0),
                        javascript_enabled: javascript_enabled.map(|v| v != 0),
                        cookies_enabled: cookies_enabled.map(|v| v != 0),
                        autoplay_enabled: autoplay_enabled.map(|v| v != 0),
                        created_at,
                    });
                }
            }
            Err(e) => warn!("Error reading site setting: {}", e),
        }
    }

    Ok(results)
}

/// Delete a site setting by ID.
pub fn delete_site_setting(conn: &Connection, id: i64) -> Result<bool> {
    invalidate_regex_cache();
    let rows = conn.execute("DELETE FROM site_settings WHERE id = ?1", params![id])?;
    Ok(rows > 0)
}

/// Delete all site settings matching a pattern (by domain).
pub fn delete_site_settings_for_domain(conn: &Connection, domain: &str) -> Result<usize> {
    invalidate_regex_cache();
    let rows = conn.execute(
        "DELETE FROM site_settings WHERE pattern = ?1 OR pattern LIKE ?2",
        params![domain, format!("%{}%", domain)],
    )?;
    Ok(rows)
}

/// List all site settings.
pub fn list_site_settings(conn: &Connection) -> Result<Vec<SiteSettings>> {
    let mut stmt = conn.prepare(
        "SELECT id, pattern, pattern_type, zoom_level, adblock_enabled, javascript_enabled, cookies_enabled, autoplay_enabled, created_at
         FROM site_settings
         ORDER BY id ASC",
    )?;

    let settings = stmt
        .query_map([], |row| {
            Ok(SiteSettings {
                id: row.get(0)?,
                pattern: row.get(1)?,
                pattern_type: row.get(2)?,
                zoom_level: row.get(3)?,
                adblock_enabled: row.get::<_, Option<i32>>(4)?.map(|v| v != 0),
                javascript_enabled: row.get::<_, Option<i32>>(5)?.map(|v| v != 0),
                cookies_enabled: row.get::<_, Option<i32>>(6)?.map(|v| v != 0),
                autoplay_enabled: row.get::<_, Option<i32>>(7)?.map(|v| v != 0),
                created_at: row.get(8)?,
            })
        })?
        .filter_map(|r| {
            if let Err(e) = &r {
                warn!("Error reading site setting: {}", e);
            }
            r.ok()
        })
        .collect();
    Ok(settings)
}

/// Check if a URL matches a pattern of the given type.
pub fn url_matches_pattern(url: &str, pattern: &str, pattern_type: &str) -> bool {
    let host = extract_host(url);
    match pattern_type {
        "exact" => host == pattern,
        "wildcard" => {
            let sql_pattern = pattern.replace('*', "%");
            host == pattern || pattern_like_match(&host, &sql_pattern)
        }
        "regex" => cached_regex(pattern)
            .map(|re| re.is_match(&host))
            .unwrap_or(false),
        _ => false,
    }
}

fn pattern_like_match(host: &str, sql_pattern: &str) -> bool {
    if sql_pattern.contains('%') {
        let parts: Vec<&str> = sql_pattern.split('%').collect();
        parts.iter().all(|p| p.is_empty() || host.contains(p))
            && host.starts_with(parts.first().unwrap_or(&""))
            && host.ends_with(parts.last().unwrap_or(&""))
    } else {
        host == sql_pattern
    }
}

/// Extract the host from a URL string.
fn extract_host(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .unwrap_or_else(|| url.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS site_settings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern TEXT NOT NULL,
                pattern_type TEXT NOT NULL DEFAULT 'exact',
                zoom_level REAL,
                adblock_enabled INTEGER,
                javascript_enabled INTEGER,
                cookies_enabled INTEGER,
                autoplay_enabled INTEGER,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_site_settings_pattern ON site_settings(pattern, pattern_type);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_upsert_and_list() {
        let conn = test_db();
        upsert_site_setting(
            &conn,
            "example.com",
            "exact",
            Some(1.5),
            Some(true),
            None,
            None,
            None,
        )
        .unwrap();

        let settings = list_site_settings(&conn).unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].pattern, "example.com");
        assert_eq!(settings[0].zoom_level, Some(1.5));
        assert_eq!(settings[0].adblock_enabled, Some(true));
        assert_eq!(settings[0].javascript_enabled, None);
    }

    #[test]
    fn test_upsert_merge() {
        let conn = test_db();
        upsert_site_setting(
            &conn,
            "example.com",
            "exact",
            Some(1.5),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        upsert_site_setting(
            &conn,
            "example.com",
            "exact",
            None,
            Some(false),
            None,
            None,
            None,
        )
        .unwrap();

        let settings = list_site_settings(&conn).unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].zoom_level, Some(1.5));
        assert_eq!(settings[0].adblock_enabled, Some(false));
    }

    #[test]
    fn test_delete_by_id() {
        let conn = test_db();
        upsert_site_setting(&conn, "example.com", "exact", None, None, None, None, None).unwrap();
        let settings = list_site_settings(&conn).unwrap();
        let id = settings[0].id;

        let deleted = delete_site_setting(&conn, id).unwrap();
        assert!(deleted);
        assert!(list_site_settings(&conn).unwrap().is_empty());
    }

    #[test]
    fn test_exact_match() {
        let conn = test_db();
        upsert_site_setting(
            &conn,
            "example.com",
            "exact",
            Some(2.0),
            None,
            None,
            None,
            None,
        )
        .unwrap();

        let results = get_site_settings_for_url(&conn, "https://example.com/page").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].zoom_level, Some(2.0));
    }

    #[test]
    fn test_wildcard_match() {
        let conn = test_db();
        upsert_site_setting(
            &conn,
            "*.example.com",
            "wildcard",
            None,
            Some(true),
            None,
            None,
            None,
        )
        .unwrap();

        let results = get_site_settings_for_url(&conn, "https://sub.example.com/page").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].adblock_enabled, Some(true));

        let results = get_site_settings_for_url(&conn, "https://other.com/page").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_regex_match() {
        let conn = test_db();
        upsert_site_setting(
            &conn,
            r".*\.google\.(com|co\.uk)",
            "regex",
            None,
            None,
            Some(false),
            None,
            None,
        )
        .unwrap();

        let results = get_site_settings_for_url(&conn, "https://www.google.com/search").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].javascript_enabled, Some(false));

        let results = get_site_settings_for_url(&conn, "https://example.com").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_set_site_field() {
        let conn = test_db();
        set_site_field(&conn, "example.com", "exact", "zoom", Some("2.0")).unwrap();
        set_site_field(&conn, "example.com", "exact", "adblock", Some("off")).unwrap();

        let settings = list_site_settings(&conn).unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].zoom_level, Some(2.0));
        assert_eq!(settings[0].adblock_enabled, Some(false));
    }

    #[test]
    fn test_delete_for_domain() {
        let conn = test_db();
        upsert_site_setting(&conn, "example.com", "exact", None, None, None, None, None).unwrap();
        upsert_site_setting(
            &conn,
            "*.example.com",
            "wildcard",
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        upsert_site_setting(&conn, "other.com", "exact", None, None, None, None, None).unwrap();

        let count = delete_site_settings_for_domain(&conn, "example.com").unwrap();
        assert!(count >= 2);

        let settings = list_site_settings(&conn).unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].pattern, "other.com");
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(
            extract_host("https://www.example.com/path"),
            "www.example.com"
        );
        assert_eq!(extract_host("http://example.com"), "example.com");
        assert_eq!(extract_host("not-a-url"), "not-a-url");
    }
}
