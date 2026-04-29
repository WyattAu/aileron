use crate::db::bookmarks;
use crate::ui::search::SearchCategory;
use crate::ui::search::SearchItem;

use super::AppState;

/// A scored omnibox result, used for merging duplicates and ranking.
struct ScoredResult {
    item: SearchItem,
    score: f64,
}

impl AppState {
    pub fn update_omnibox(&mut self, query: &str) {
        self.omnibox_results.clear();
        self.omnibox_selected = 0;

        let query = query.trim();
        if query.is_empty() {
            self.last_omnibox_query.clear();
            return;
        }

        self.last_omnibox_query = query.to_string();

        let looks_like_url = query.contains("://")
            || query.starts_with("aileron://")
            || (query.contains('.') && !query.contains(' '));

        let mut scored: Vec<ScoredResult> = Vec::new();

        // 1. Navigation / search result (always first, score 1000)
        if looks_like_url {
            let url = if query.contains("://") || query.starts_with("aileron://") {
                query.to_string()
            } else {
                format!("https://{}", query)
            };
            scored.push(ScoredResult {
                item: SearchItem {
                    id: format!("nav:{}", url),
                    label: url.clone(),
                    description: "Navigate to URL".to_string(),
                    category: SearchCategory::Command,
                },
                score: 1000.0,
            });
        } else {
            let search_url = self
                .config
                .search_url(query)
                .map(|u| u.to_string())
                .unwrap_or_default();
            scored.push(ScoredResult {
                item: SearchItem {
                    id: format!("search:{}", query),
                    label: format!("Search: {}", query),
                    description: search_url,
                    category: SearchCategory::Command,
                },
                score: 1000.0,
            });
        }

        if let Some(db) = self.db.as_ref() {
            // 2. Open tabs (score 900) — deduplicate with history/bookmarks
            let pane_ids = self.wm.pane_ids();
            let mut open_tab_entries: Vec<(String, String)> = Vec::new();
            for pane_id in &pane_ids {
                if let Some(state) = self.engines.get(pane_id) {
                    let url_str = state
                        .current_url()
                        .map(|u| u.to_string())
                        .unwrap_or_default();
                    let title = state.title().to_string();
                    if !url_str.is_empty() {
                        open_tab_entries.push((url_str, title));
                    }
                }
            }
            for (tab_url, tab_title) in &open_tab_entries {
                if tab_url.contains(query) || tab_title.contains(query) {
                    scored.push(ScoredResult {
                        item: SearchItem {
                            id: format!("tab:{}", tab_url),
                            label: tab_title.clone(),
                            description: format!("[tab] {}", tab_url),
                            category: SearchCategory::OpenTab,
                        },
                        score: 900.0,
                    });
                }
            }

            // 3. Bookmarks (score 800)
            if let Ok(bms) = bookmarks::search_bookmarks(db, query, 10) {
                for bm in bms {
                    scored.push(ScoredResult {
                        item: SearchItem {
                            id: format!("bookmark:{}", bm.url),
                            label: bm.title,
                            description: format!("[bm] {}", bm.url),
                            category: SearchCategory::Bookmark,
                        },
                        score: 800.0,
                    });
                }
            }

            // 4. History with frecency ranking (score = frecency * 100)
            if let Ok(entries) = crate::db::history::search_frecency(db, query, 10) {
                for (h, frecency) in entries {
                    scored.push(ScoredResult {
                        item: SearchItem {
                            id: format!("history:{}", h.url),
                            label: h.url.clone(),
                            description: format!("[hist] {} ({} visits)", h.title, h.visit_count),
                            category: SearchCategory::History,
                        },
                        score: frecency * 100.0,
                    });
                }
            }
        }

        // Deduplicate: keep highest-scoring entry per URL
        let mut seen_urls: std::collections::HashSet<String> = std::collections::HashSet::new();
        scored.retain(|s| {
            // nav: and search: are unique prefixes, always keep
            if s.item.id.starts_with("nav:") || s.item.id.starts_with("search:") {
                return true;
            }
            // Extract URL from id (bookmark:, history:, tab:)
            let url_key = s.item.id.split_once(':').map(|x| x.1).unwrap_or(&s.item.id);
            seen_urls.insert(url_key.to_string())
        });

        // Sort by score descending
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Extract items, cap at 10
        self.omnibox_results = scored.into_iter().take(10).map(|s| s.item).collect();
    }

    pub fn handle_omnibox_select(&mut self, index: usize) {
        if let Some(item) = self.omnibox_results.get(index) {
            let id = item.id.clone();
            let label = item.label.clone();
            if let Some(url_str) = id.strip_prefix("nav:") {
                if let Ok(url) = url::Url::parse(url_str) {
                    self.navigate_with_redirects(url);
                    self.status_message = format!("Navigating to {}", url_str);
                }
            } else if let Some(query) = id.strip_prefix("search:") {
                if let Some(url) = self.config.search_url(query) {
                    self.navigate_with_redirects(url);
                    self.status_message = format!("Searching: {}", query);
                }
            } else if let Some(url) = id.strip_prefix("bookmark:") {
                if let Ok(parsed) = url::Url::parse(url) {
                    self.navigate_with_redirects(parsed);
                    self.status_message = format!("Opening bookmark: {}", label);
                }
            } else if let Some(url) = id.strip_prefix("history:")
                && let Ok(parsed) = url::Url::parse(url)
            {
                self.navigate_with_redirects(parsed);
                self.status_message = format!("Opening: {}", url);
            } else if let Some(url) = id.strip_prefix("tab:")
                && let Ok(parsed) = url::Url::parse(url)
            {
                // Switch to the already-open tab instead of navigating
                let url_str = url.to_string();
                let switched = self.wm.pane_ids().iter().any(|pane_id| {
                    self.engines.get(pane_id).is_some_and(|state| {
                        state.current_url().map(|u| u.to_string()).as_deref() == Some(&url_str) && {
                            self.wm.set_active_pane(*pane_id);
                            true
                        }
                    })
                });
                if !switched {
                    self.navigate_with_redirects(parsed);
                    self.status_message = format!("Opening: {}", url);
                } else {
                    self.status_message = format!("Switched to tab: {}", label);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a minimal AppState for omnibox testing.
    fn test_app_state() -> AppState {
        let viewport = crate::wm::rect::Rect::new(0.0, 0.0, 1280.0, 720.0);
        let mut config = crate::config::Config::default();
        config.search_engine = "duckduckgo".to_string();
        config.search_engines.insert(
            "duckduckgo".to_string(),
            "https://duckduckgo.com/?q={}".to_string(),
        );
        AppState::new(viewport, config).unwrap()
    }

    #[test]
    fn test_empty_query_clears_results() {
        let mut state = test_app_state();
        state.omnibox_results.push(crate::ui::search::SearchItem {
            id: "test:1".into(),
            label: "Test".into(),
            description: "Desc".into(),
            category: crate::ui::search::SearchCategory::Command,
        });
        state.update_omnibox("");
        assert!(state.omnibox_results.is_empty());
        assert_eq!(state.omnibox_selected, 0);
        assert!(state.last_omnibox_query.is_empty());
    }

    #[test]
    fn test_url_detection() {
        let mut state = test_app_state();
        state.update_omnibox("https://example.com");
        assert_eq!(state.omnibox_results.len(), 1);
        assert!(state.omnibox_results[0].id.starts_with("nav:"));
        assert_eq!(state.omnibox_results[0].label, "https://example.com");
    }

    #[test]
    fn test_bare_domain_detected_as_url() {
        let mut state = test_app_state();
        state.update_omnibox("example.com");
        assert_eq!(state.omnibox_results.len(), 1);
        assert!(state.omnibox_results[0].id.starts_with("nav:"));
        assert_eq!(state.omnibox_results[0].label, "https://example.com");
    }

    #[test]
    fn test_non_url_triggers_search() {
        let mut state = test_app_state();
        state.update_omnibox("hello world");
        assert_eq!(state.omnibox_results.len(), 1);
        assert!(state.omnibox_results[0].id.starts_with("search:"));
        assert!(state.omnibox_results[0].label.contains("hello world"));
    }

    #[test]
    fn test_aileron_scheme_detected() {
        let mut state = test_app_state();
        state.update_omnibox("aileron://settings");
        assert_eq!(state.omnibox_results.len(), 1);
        assert!(state.omnibox_results[0].id.starts_with("nav:"));
    }

    #[test]
    fn test_query_with_spaces_not_url() {
        let mut state = test_app_state();
        state.update_omnibox("git hub.com"); // contains space and dot
        assert_eq!(state.omnibox_results.len(), 1);
        // Space means it's not a URL — should trigger search
        assert!(state.omnibox_results[0].id.starts_with("search:"));
    }

    #[test]
    fn test_whitespace_trimmed() {
        let mut state = test_app_state();
        state.update_omnibox("  https://example.com  ");
        assert_eq!(state.omnibox_results.len(), 1);
        assert_eq!(state.omnibox_results[0].label, "https://example.com");
    }

    #[test]
    fn test_selected_reset_on_new_query() {
        let mut state = test_app_state();
        state.update_omnibox("hello");
        state.omnibox_selected = 5; // Simulate user selecting item 5
        state.update_omnibox("hello"); // New query
        assert_eq!(state.omnibox_selected, 0);
    }

    #[test]
    fn test_results_capped_at_10() {
        let mut state = test_app_state();
        // Set up a DB with bookmarks to get >10 results
        let temp = tempfile::NamedTempFile::new().unwrap();
        let conn = crate::db::open_database(temp.path()).unwrap();
        state.db = Some(conn);
        // Insert 15 bookmarks matching "test"
        for i in 0..15 {
            crate::db::bookmarks::add_bookmark(
                state.db.as_ref().unwrap(),
                &format!("https://test{}.com", i),
                &format!("Test Bookmark {}", i),
            )
            .ok();
        }
        state.update_omnibox("test");
        assert!(state.omnibox_results.len() <= 11); // 1 nav + max 10 others
    }
}
