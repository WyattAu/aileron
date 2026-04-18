use crate::db::bookmarks;
use crate::ui::search::SearchCategory;
use crate::ui::search::SearchItem;

use super::AppState;

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

        let looks_like_url = query.contains("://") || query.starts_with("aileron://")
            || (query.contains('.') && !query.contains(' '));

        if looks_like_url {
            let url = if query.contains("://") || query.starts_with("aileron://") {
                query.to_string()
            } else {
                format!("https://{}", query)
            };
            self.omnibox_results.push(SearchItem {
                id: format!("nav:{}", url),
                label: url.clone(),
                description: "Navigate to URL".to_string(),
                category: SearchCategory::Command,
            });
        } else {
            let search_url = self.config.search_url(query)
                .map(|u| u.to_string())
                .unwrap_or_default();
            self.omnibox_results.push(SearchItem {
                id: format!("search:{}", query),
                label: format!("Search: {}", query),
                description: search_url,
                category: SearchCategory::Command,
            });
        }

        if let Some(db) = self.db.as_ref() {
            if let Ok(bookmarks) = bookmarks::search_bookmarks(db, query, 5) {
                for bm in bookmarks {
                    self.omnibox_results.push(SearchItem {
                        id: format!("bookmark:{}", bm.url),
                        label: bm.title,
                        description: bm.url,
                        category: SearchCategory::Bookmark,
                    });
                }
            }

            if let Ok(history) = crate::db::history::search(db, query, 5) {
                for h in history {
                    self.omnibox_results.push(SearchItem {
                        id: format!("history:{}", h.url),
                        label: h.url.clone(),
                        description: format!("visited {} times", h.visit_count),
                        category: SearchCategory::History,
                    });
                }
            }
        }

        if self.omnibox_results.len() > 10 {
            self.omnibox_results.truncate(10);
        }
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
            }
        }
    }
}
