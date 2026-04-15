//! Content script system — Lua-defined JavaScript injection.

use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct ContentScript {
    pub name: String,
    pub match_patterns: Vec<String>,
    pub grants: Vec<String>,
    pub js_code: String,
    pub enabled: bool,
}

pub struct ContentScriptManager {
    scripts: Vec<ContentScript>,
    scripts_dir: PathBuf,
}

impl ContentScriptManager {
    pub fn new() -> Self {
        let scripts_dir = directories::ProjectDirs::from("com", "aileron", "Aileron")
            .map(|dirs| dirs.config_dir().join("scripts"))
            .unwrap_or_else(|| PathBuf::from("~/.config/aileron/scripts"));

        let mut manager = Self {
            scripts: Vec::new(),
            scripts_dir,
        };
        manager.load_all();
        manager
    }

    fn load_all(&mut self) {
        if !self.scripts_dir.exists() {
            info!("No scripts directory found at {:?}", self.scripts_dir);
            return;
        }

        if let Ok(entries) = std::fs::read_dir(&self.scripts_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "lua").unwrap_or(false) {
                    match Self::parse_script(&path) {
                        Ok(script) => {
                            info!(
                                "Loaded content script: {} ({} patterns)",
                                script.name,
                                script.match_patterns.len()
                            );
                            self.scripts.push(script);
                        }
                        Err(e) => {
                            warn!("Failed to parse script {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        info!("Loaded {} content script(s)", self.scripts.len());
    }

    fn parse_script(path: &std::path::Path) -> anyhow::Result<ContentScript> {
        let source = std::fs::read_to_string(path)?;

        let mut name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".into());
        let mut match_patterns = Vec::new();
        let mut grants = Vec::new();

        if let Some(start) = source.find("==UserScript==")
            && let Some(end) = source[start..].find("==/UserScript==")
        {
            let metadata = &source[start + 15..start + end];
            for line in metadata.lines() {
                let line = line.trim();
                if let Some(value) = line.strip_prefix("@name") {
                    name = value.trim().to_string();
                } else if let Some(pattern) = line.strip_prefix("@match") {
                    match_patterns.push(pattern.trim().to_string());
                } else if let Some(grant) = line.strip_prefix("@grant") {
                    grants.push(grant.trim().to_string());
                }
            }
        }

        let js_code = if source.contains("==UserScript==") {
            match Self::eval_lua_script(&source) {
                Ok(js) => js,
                Err(e) => {
                    warn!("Lua eval failed for {:?}: {}", path, e);
                    Self::extract_js_fallback(&source)
                }
            }
        } else {
            source
                .lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n")
        };

        Ok(ContentScript {
            name,
            match_patterns,
            grants,
            js_code,
            enabled: true,
        })
    }

    fn eval_lua_script(source: &str) -> anyhow::Result<String> {
        let lua = mlua::Lua::new();
        let js_code: String = lua
            .load(source)
            .eval()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        Ok(js_code)
    }

    fn extract_js_fallback(source: &str) -> String {
        if let Some(pos) = source.find("==/UserScript==") {
            source[pos + 16..]
                .lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        } else {
            source.to_string()
        }
    }

    pub fn scripts_for_url(&self, url: &str) -> Vec<&ContentScript> {
        self.scripts
            .iter()
            .filter(|s| s.enabled && Self::url_matches_patterns(url, &s.match_patterns))
            .collect()
    }

    fn url_matches_patterns(url: &str, patterns: &[String]) -> bool {
        if patterns.is_empty() {
            return false;
        }
        patterns.iter().any(|p| Self::url_matches_pattern(url, p))
    }

    fn url_matches_pattern(url: &str, pattern: &str) -> bool {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.is_empty() {
            return url == pattern;
        }

        let mut pos = 0;
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            if let Some(found) = url[pos..].find(part) {
                pos += found + part.len();
            } else {
                return false;
            }
            if i == parts.len() - 1 && !pattern.ends_with('*') && pos != url.len() {
                return false;
            }
        }
        true
    }

    pub fn all_scripts(&self) -> &[ContentScript] {
        &self.scripts
    }

    pub fn scripts_dir(&self) -> &PathBuf {
        &self.scripts_dir
    }
}

impl Default for ContentScriptManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_matches_pattern_exact() {
        assert!(ContentScriptManager::url_matches_pattern(
            "https://github.com",
            "https://github.com"
        ));
    }

    #[test]
    fn test_url_matches_pattern_wildcard() {
        assert!(ContentScriptManager::url_matches_pattern(
            "https://api.github.com/user/repo",
            "https://*.github.com/*"
        ));
    }

    #[test]
    fn test_url_matches_pattern_wildcard_subdomain() {
        assert!(ContentScriptManager::url_matches_pattern(
            "https://api.github.com/v1/users",
            "https://*.github.com/*"
        ));
    }

    #[test]
    fn test_url_matches_pattern_no_match() {
        assert!(!ContentScriptManager::url_matches_pattern(
            "https://google.com",
            "https://*.github.com/*"
        ));
    }

    #[test]
    fn test_url_matches_pattern_trailing_wildcard() {
        assert!(ContentScriptManager::url_matches_pattern(
            "https://github.com/user/repo/issues/42",
            "https://github.com/*"
        ));
    }

    #[test]
    fn test_url_matches_pattern_no_trailing_wildcard() {
        assert!(!ContentScriptManager::url_matches_pattern(
            "https://github.com/user/repo",
            "https://github.com"
        ));
    }

    #[test]
    fn test_scripts_for_url_filters() {
        let manager = ContentScriptManager {
            scripts: vec![
                ContentScript {
                    name: "gh-script".into(),
                    match_patterns: vec!["https://*.github.com/*".into()],
                    grants: vec![],
                    js_code: "console.log('hi')".into(),
                    enabled: true,
                },
                ContentScript {
                    name: "disabled-script".into(),
                    match_patterns: vec!["https://*.github.com/*".into()],
                    grants: vec![],
                    js_code: "console.log('no')".into(),
                    enabled: false,
                },
                ContentScript {
                    name: "other-script".into(),
                    match_patterns: vec!["https://*.reddit.com/*".into()],
                    grants: vec![],
                    js_code: "console.log('other')".into(),
                    enabled: true,
                },
            ],
            scripts_dir: PathBuf::from("/tmp"),
        };

        let matches = manager.scripts_for_url("https://api.github.com/user/repo");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "gh-script");
    }
}
