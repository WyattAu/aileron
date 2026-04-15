/// A searchable item for the command palette / fuzzy finder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchItem {
    pub id: String,
    pub label: String,
    pub description: String,
    pub category: SearchCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchCategory {
    History,
    Bookmark,
    Command,
    OpenTab,
    Setting,
    Credential,
    /// User-defined command registered via aileron.cmd.create in Lua.
    Custom,
}

/// Simple fuzzy matcher using substring matching + scoring heuristic.
/// Provides fast inline search without background threads.
/// For a more advanced implementation, nucleo could be used with its
/// thread-pool based `Nucleo<T>` matcher.
pub fn fuzzy_match(items: &[SearchItem], query: &str, limit: usize) -> Vec<SearchItem> {
    let query_lower = query.to_lowercase();
    let mut scored: Vec<(usize, &SearchItem)> = items
        .iter()
        .filter_map(|item| {
            let label_lower = item.label.to_lowercase();
            let desc_lower = item.description.to_lowercase();

            // Score: position of match + whether it starts with query
            if let Some(pos) = label_lower.find(&query_lower) {
                let score = if pos == 0 {
                    1000 - item.label.len()
                } else {
                    500 - pos
                };
                Some((score, item))
            } else if let Some(pos) = desc_lower.find(&query_lower) {
                let score = 200 - pos;
                Some((score, item))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(limit);
    scored.into_iter().map(|(_, item)| item.clone()).collect()
}

/// A search engine that holds a list of items and provides incremental search.
/// Uses the inline fuzzy_match function for simplicity.
pub struct FuzzySearch {
    items: Vec<SearchItem>,
}

impl FuzzySearch {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Add an item to the search index.
    pub fn upsert(&mut self, item: SearchItem) {
        // Check if item with same ID exists, update or push
        if let Some(existing) = self.items.iter_mut().find(|i| i.id == item.id) {
            *existing = item;
        } else {
            self.items.push(item);
        }
    }

    /// Add multiple items at once.
    pub fn extend(&mut self, items: Vec<SearchItem>) {
        self.items.extend(items);
    }

    /// Search for items matching the query.
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchItem> {
        fuzzy_match(&self.items, query, limit)
    }

    /// Clear all items.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Get the total number of items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl Default for FuzzySearch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(id: &str, label: &str, desc: &str) -> SearchItem {
        SearchItem {
            id: id.to_string(),
            label: label.to_string(),
            description: desc.to_string(),
            category: SearchCategory::History,
        }
    }

    #[test]
    fn test_fuzzy_match_exact() {
        let items = vec![
            make_item("1", "GitHub", "Code hosting"),
            make_item("2", "Google", "Search engine"),
            make_item("3", "Rust Lang", "Programming language"),
        ];
        let results = fuzzy_match(&items, "GitHub", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
    }

    #[test]
    fn test_fuzzy_match_substring() {
        let items = vec![
            make_item("1", "github.com/servo/servo", "Servo browser engine"),
            make_item("2", "github.com/rust-lang/rust", "Rust compiler"),
            make_item("3", "example.com", "Example site"),
        ];
        let results = fuzzy_match(&items, "rust", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "2");
    }

    #[test]
    fn test_fuzzy_match_case_insensitive() {
        let items = vec![
            make_item("1", "GITHUB", "Code hosting"),
            make_item("2", "Google", "Search"),
        ];
        let results = fuzzy_match(&items, "github", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
    }

    #[test]
    fn test_fuzzy_match_no_results() {
        let items = vec![
            make_item("1", "GitHub", "Code"),
            make_item("2", "Google", "Search"),
        ];
        let results = fuzzy_match(&items, "zzzzz", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_fuzzy_match_respects_limit() {
        let items: Vec<SearchItem> = (0..20)
            .map(|i| make_item(&i.to_string(), &format!("item_{}", i), "desc"))
            .collect();
        let results = fuzzy_match(&items, "item", 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_fuzzy_match_description_search() {
        let items = vec![
            make_item("1", "Site A", "rust programming"),
            make_item("2", "Site B", "python programming"),
        ];
        let results = fuzzy_match(&items, "rust", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "1");
    }

    #[test]
    fn test_fuzzy_search_upsert() {
        let mut search = FuzzySearch::new();
        search.upsert(make_item("1", "GitHub", "Code hosting"));
        search.upsert(make_item("2", "Google", "Search"));
        assert_eq!(search.len(), 2);
    }

    #[test]
    fn test_fuzzy_search_update_existing() {
        let mut search = FuzzySearch::new();
        search.upsert(make_item("1", "GitHub", "Code hosting"));
        search.upsert(make_item("1", "GitHub Updated", "Code hosting updated"));
        assert_eq!(search.len(), 1);
        assert_eq!(search.items[0].label, "GitHub Updated");
    }

    #[test]
    fn test_fuzzy_search_clear() {
        let mut search = FuzzySearch::new();
        search.upsert(make_item("1", "A", "B"));
        search.clear();
        assert!(search.is_empty());
    }
}
