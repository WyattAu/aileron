use crate::ui::search::{fuzzy_match, SearchItem};

/// State and logic for the command palette overlay.
/// Manages the list of items, current query, selection, and actions.
pub struct CommandPalette {
    /// Whether the palette is currently visible.
    pub open: bool,
    /// Current search query.
    pub query: String,
    /// All items that can be searched.
    items: Vec<SearchItem>,
    /// Filtered results matching the current query.
    results: Vec<SearchItem>,
    /// Index of the currently selected item in results.
    selected_index: usize,
    /// Maximum number of results to show.
    max_results: usize,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            open: false,
            query: String::new(),
            items: Vec::new(),
            results: Vec::new(),
            selected_index: 0,
            max_results: 20,
        }
    }

    /// Open the command palette and focus the search input.
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.results.clear();
        self.selected_index = 0;
    }

    /// Close the command palette.
    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.results.clear();
        self.selected_index = 0;
    }

    /// Add an item to the command palette's search index.
    pub fn add_item(&mut self, item: SearchItem) {
        self.items.push(item);
    }

    /// Add multiple items at once.
    pub fn add_items(&mut self, items: Vec<SearchItem>) {
        self.items.extend(items);
    }

    /// Set the complete list of items (replaces all existing items).
    pub fn set_items(&mut self, items: Vec<SearchItem>) {
        self.items = items;
    }

    /// Clear all items.
    pub fn clear_items(&mut self) {
        self.items.clear();
        self.results.clear();
    }

    /// Update the search query and recompute results.
    pub fn update_query(&mut self, query: &str) {
        self.query = query.to_string();
        if query.is_empty() {
            // Show recent items when query is empty (last N items)
            self.results = self
                .items
                .iter()
                .rev()
                .take(self.max_results)
                .cloned()
                .collect();
        } else {
            self.results = fuzzy_match(&self.items, query, self.max_results);
        }
        self.selected_index = 0;
    }

    /// Get the current search results.
    pub fn results(&self) -> &[SearchItem] {
        &self.results
    }

    /// Get the currently selected item, if any.
    pub fn selected_item(&self) -> Option<&SearchItem> {
        self.results.get(self.selected_index)
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else if !self.results.is_empty() {
            self.selected_index = self.results.len() - 1;
        }
    }

    /// Move selection down.
    pub fn move_down(&mut self) {
        if !self.results.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.results.len();
        }
    }

    /// Select the current item and return it (consumed).
    /// Returns None if no item is selected.
    pub fn confirm_selection(&mut self) -> Option<SearchItem> {
        if self.results.is_empty() {
            return None;
        }
        let item = self.results.get(self.selected_index).cloned();
        self.close();
        item
    }

    /// Handle a character input while the palette is open.
    /// Returns Some(action) if the input triggered a palette action.
    pub fn handle_input(&mut self, key: &str) -> PaletteAction {
        match key {
            "Up" => {
                self.move_up();
                PaletteAction::Consumed
            }
            "Down" => {
                self.move_down();
                PaletteAction::Consumed
            }
            "Enter" => {
                if let Some(item) = self.confirm_selection() {
                    PaletteAction::ItemSelected(item)
                } else if !self.query.trim().is_empty() {
                    // No results matched — submit the raw query for URL/command handling
                    let query = self.query.trim().to_string();
                    self.close();
                    PaletteAction::QuerySubmit(query)
                } else {
                    self.close();
                    PaletteAction::Closed
                }
            }
            "Escape" => {
                self.close();
                PaletteAction::Closed
            }
            "Backspace" => {
                self.query.pop();
                let q = self.query.clone();
                self.update_query(&q);
                PaletteAction::Consumed
            }
            _ => {
                // Treat as text input
                self.query.push_str(key);
                let q = self.query.clone();
                self.update_query(&q);
                PaletteAction::Consumed
            }
        }
    }
}

/// Actions that can result from palette input handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteAction {
    /// The input was consumed but no special action.
    Consumed,
    /// An item was selected. Carries the selected item so the caller
    /// doesn't need to re-read it after the palette closes.
    ItemSelected(SearchItem),
    /// The palette was closed.
    Closed,
    /// The raw query was submitted (no matching results). Contains the trimmed query.
    QuerySubmit(String),
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::search::SearchCategory;

    fn item(id: &str, label: &str) -> SearchItem {
        SearchItem {
            id: id.to_string(),
            label: label.to_string(),
            description: String::new(),
            category: SearchCategory::Command,
        }
    }

    #[test]
    fn test_open_close() {
        let mut palette = CommandPalette::new();
        assert!(!palette.open);
        palette.open();
        assert!(palette.open);
        palette.close();
        assert!(!palette.open);
    }

    #[test]
    fn test_search_results() {
        let mut palette = CommandPalette::new();
        palette.add_items(vec![
            item("1", "Quit"),
            item("2", "Split Vertical"),
            item("3", "Split Horizontal"),
        ]);
        palette.open();
        palette.update_query("split");
        assert_eq!(palette.results().len(), 2);
    }

    #[test]
    fn test_navigation() {
        let mut palette = CommandPalette::new();
        palette.add_items(vec![item("1", "A"), item("2", "B"), item("3", "C")]);
        palette.open();
        palette.update_query(""); // Show all
        assert_eq!(palette.selected_index, 0);

        palette.move_down();
        assert_eq!(palette.selected_index, 1);

        palette.move_down();
        assert_eq!(palette.selected_index, 2);

        // Wrap around
        palette.move_down();
        assert_eq!(palette.selected_index, 0);

        palette.move_up();
        assert_eq!(palette.selected_index, 2);
    }

    #[test]
    fn test_confirm_selection() {
        let mut palette = CommandPalette::new();
        palette.add_items(vec![item("1", "A"), item("2", "B")]);
        palette.open();
        palette.update_query("");

        // Empty query shows items in reverse order (most recent first)
        // So "B" (id=2) is first
        let selected = palette.confirm_selection();
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().id, "2");
        assert!(!palette.open); // Should be closed after confirm
    }

    #[test]
    fn test_confirm_empty() {
        let mut palette = CommandPalette::new();
        palette.open();
        let selected = palette.confirm_selection();
        assert!(selected.is_none());
    }

    #[test]
    fn test_handle_input_text() {
        let mut palette = CommandPalette::new();
        palette.add_items(vec![item("1", "Quit"), item("2", "Quit All")]);
        palette.open();

        assert_eq!(palette.handle_input("Q"), PaletteAction::Consumed);
        assert_eq!(palette.query, "Q");
        assert_eq!(palette.handle_input("u"), PaletteAction::Consumed);
        assert_eq!(palette.query, "Qu");
        assert_eq!(palette.handle_input("i"), PaletteAction::Consumed);
        assert_eq!(palette.query, "Qui");

        // Should find both "Quit" items
        assert_eq!(palette.results().len(), 2);
    }

    #[test]
    fn test_handle_input_escape() {
        let mut palette = CommandPalette::new();
        palette.open();
        assert_eq!(palette.handle_input("Escape"), PaletteAction::Closed);
        assert!(!palette.open);
    }

    #[test]
    fn test_clear_query_shows_recent() {
        let mut palette = CommandPalette::new();
        palette.max_results = 3;
        for i in 0..5 {
            palette.add_item(item(&i.to_string(), &format!("Item {}", i)));
        }
        palette.open();
        palette.update_query(""); // Empty query shows recent items (last 3, reversed)
        assert_eq!(palette.results().len(), 3);
        // Most recent first: 4, 3, 2
        assert_eq!(palette.results()[0].id, "4");
    }

    #[test]
    fn test_selected_item() {
        let mut palette = CommandPalette::new();
        palette.add_items(vec![item("1", "A"), item("2", "B")]);
        palette.open();
        palette.update_query("");
        // Most recent first: "B" (id=2) is at index 0
        assert_eq!(palette.selected_item().unwrap().id, "2");
        palette.move_down();
        assert_eq!(palette.selected_item().unwrap().id, "1");
    }
}
