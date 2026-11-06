use url::Url;
use uuid::Uuid;

/// A single tab within a pane. Each tab has its own URL, title, and identity.
#[derive(Debug, Clone)]
pub struct Tab {
    pub id: Uuid,
    pub url: Url,
    pub title: String,
}

impl Tab {
    pub fn new(url: Url) -> Self {
        let title = url.to_string();
        Self {
            id: Uuid::new_v4(),
            url,
            title,
        }
    }
}

/// An ordered list of tabs with one active tab at a time.
///
/// Invariant: `tabs` is never empty after construction.
/// Invariant: `active_index < tabs.len()`.
#[derive(Debug, Clone)]
pub struct TabList {
    tabs: Vec<Tab>,
    active_index: usize,
}

impl TabList {
    /// Create a new TabList with a single tab.
    pub fn new(url: Url) -> Self {
        Self {
            tabs: vec![Tab::new(url)],
            active_index: 0,
        }
    }

    /// Number of tabs.
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// True if no tabs (should never happen after construction).
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Get the currently active tab.
    pub fn active(&self) -> &Tab {
        &self.tabs[self.active_index]
    }

    /// Get a mutable reference to the currently active tab.
    pub fn active_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_index]
    }

    /// Index of the currently active tab.
    pub fn active_index(&self) -> usize {
        self.active_index
    }

    /// Get a tab by index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Tab> {
        self.tabs.get(index)
    }

    /// Iterate over all tabs.
    pub fn iter(&self) -> impl Iterator<Item = &Tab> {
        self.tabs.iter()
    }

    /// Iterate mutably over all tabs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Tab> {
        self.tabs.iter_mut()
    }

    /// Find a tab by its UUID. Returns the index.
    #[must_use]
    pub fn find_index(&self, tab_id: Uuid) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == tab_id)
    }

    /// Get a tab by its UUID.
    #[must_use]
    pub fn get_by_id(&self, tab_id: Uuid) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == tab_id)
    }

    /// Add a new tab and switch to it. Returns the new tab's ID.
    pub fn add(&mut self, url: Url) -> Uuid {
        let tab = Tab::new(url);
        let id = tab.id;
        // Insert after the active tab
        self.tabs.insert(self.active_index + 1, tab);
        self.active_index += 1;
        id
    }

    /// Switch to a tab by index. Returns false if index is out of bounds.
    pub fn switch_to(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.active_index = index;
            true
        } else {
            false
        }
    }

    /// Switch to a tab by its UUID. Returns false if not found.
    pub fn switch_to_id(&mut self, tab_id: Uuid) -> bool {
        if let Some(idx) = self.find_index(tab_id) {
            self.active_index = idx;
            true
        } else {
            false
        }
    }

    /// Close a tab by index. Returns the closed tab's data, or None if invalid.
    /// If the closed tab was active, the next tab becomes active.
    /// Returns None if this is the last tab (use `is_single()` to check).
    #[must_use]
    pub fn close(&mut self, index: usize) -> Option<Tab> {
        if index >= self.tabs.len() || self.tabs.len() <= 1 {
            return None;
        }
        let removed = self.tabs.remove(index);
        // Adjust active index
        if self.tabs.is_empty() {
            // Should not happen (checked len <= 1 above), but guard anyway
            return Some(removed);
        }
        if index < self.active_index {
            // Closed tab was before active: shift active left
            self.active_index -= 1;
        } else if index == self.active_index {
            // Closed the active tab: clamp to valid range
            self.active_index = self.active_index.min(self.tabs.len() - 1);
        }
        // index > active_index: no adjustment needed
        Some(removed)
    }

    /// Close a tab by its UUID. Returns the closed tab's data.
    #[must_use]
    pub fn close_by_id(&mut self, tab_id: Uuid) -> Option<Tab> {
        let index = self.find_index(tab_id)?;
        self.close(index)
    }

    /// Close the currently active tab. Returns the closed tab's data.
    /// After closing, the next tab becomes active.
    #[must_use]
    pub fn close_active(&mut self) -> Option<Tab> {
        let index = self.active_index;
        self.close(index)
    }

    /// Switch to the next tab (wraps around).
    pub fn next(&mut self) {
        if self.tabs.len() > 1 {
            self.active_index = (self.active_index + 1) % self.tabs.len();
        }
    }

    /// Switch to the previous tab (wraps around).
    pub fn prev(&mut self) {
        if self.tabs.len() > 1 {
            self.active_index = if self.active_index == 0 {
                self.tabs.len() - 1
            } else {
                self.active_index - 1
            };
        }
    }

    /// True if there is only one tab (cannot close further without closing pane).
    pub fn is_single(&self) -> bool {
        self.tabs.len() <= 1
    }
}

/// Metadata for a single browser pane (leaf node in BSP tree).
///
/// A pane owns a `TabList` — one tab is active at a time, and the pane's
/// identity (UUID) is stable across tab switches. The BSP tree references
/// pane IDs, not tab IDs.
#[derive(Debug, Clone)]
pub struct Pane {
    pub id: Uuid,
    pub tabs: TabList,
    pub session_id: Option<String>,
}

impl Pane {
    pub fn new(url: Url) -> Self {
        Self {
            id: Uuid::new_v4(),
            tabs: TabList::new(url),
            session_id: None,
        }
    }

    /// Convenience: get the active tab's URL.
    pub fn url(&self) -> &Url {
        &self.tabs.active().url
    }

    /// Convenience: get the active tab's title.
    pub fn title(&self) -> &str {
        &self.tabs.active().title
    }

    /// Convenience: get the active tab's UUID.
    pub fn active_tab_id(&self) -> Uuid {
        self.tabs.active().id
    }

    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Tab tests ──────────────────────────────────────────────────

    #[test]
    fn test_tab_new() {
        let url = Url::parse("https://example.com").unwrap();
        let tab = Tab::new(url.clone());
        assert_eq!(tab.url.as_str(), "https://example.com/");
        assert_eq!(tab.title, "https://example.com/");
    }

    #[test]
    fn test_tab_unique_ids() {
        let url = Url::parse("https://example.com").unwrap();
        let t1 = Tab::new(url.clone());
        let t2 = Tab::new(url);
        assert_ne!(t1.id, t2.id);
    }

    // ─── TabList tests ──────────────────────────────────────────────

    #[test]
    fn test_tablist_new() {
        let tl = TabList::new(Url::parse("https://example.com").unwrap());
        assert_eq!(tl.len(), 1);
        assert_eq!(tl.active_index(), 0);
        assert_eq!(tl.active().url.as_str(), "https://example.com/");
        assert!(!tl.is_empty());
        assert!(tl.is_single());
    }

    #[test]
    fn test_tablist_add() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        let id2 = tl.add(Url::parse("https://b.com").unwrap());
        assert_eq!(tl.len(), 2);
        assert_eq!(tl.active_index(), 1);
        assert_eq!(tl.active().url.as_str(), "https://b.com/");
        assert_ne!(id2, tl.tabs[0].id);
    }

    #[test]
    fn test_tablist_add_inserts_after_active() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        let id2 = tl.add(Url::parse("https://b.com").unwrap());
        let id3 = tl.add(Url::parse("https://c.com").unwrap());
        assert_eq!(tl.len(), 3);
        // id2 is at index 1, id3 inserted after it at index 2
        assert_eq!(tl.tabs[0].url.as_str(), "https://a.com/");
        assert_eq!(tl.tabs[1].id, id2);
        assert_eq!(tl.tabs[2].id, id3);
        assert_eq!(tl.active_index(), 2);
    }

    #[test]
    fn test_tablist_switch_to() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        tl.add(Url::parse("https://b.com").unwrap());
        tl.add(Url::parse("https://c.com").unwrap());
        assert!(tl.switch_to(0));
        assert_eq!(tl.active_index(), 0);
        assert!(!tl.switch_to(99)); // out of bounds
        assert_eq!(tl.active_index(), 0); // unchanged
    }

    #[test]
    fn test_tablist_switch_to_id() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        let id2 = tl.add(Url::parse("https://b.com").unwrap());
        assert!(tl.switch_to_id(id2));
        assert_eq!(tl.active_index(), 1);
        assert!(!tl.switch_to_id(Uuid::nil())); // not found
    }

    #[test]
    fn test_tablist_close_by_index() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        tl.add(Url::parse("https://b.com").unwrap());
        tl.add(Url::parse("https://c.com").unwrap());
        assert_eq!(tl.active_index(), 2);

        // Close index 1 (middle tab)
        let closed = tl.close(1).unwrap();
        assert_eq!(closed.url.as_str(), "https://b.com/");
        assert_eq!(tl.len(), 2);
        // Active was at 2, closed at 1 (before active), active shifts to 1
        assert_eq!(tl.active_index(), 1);
        assert_eq!(tl.active().url.as_str(), "https://c.com/");
    }

    #[test]
    fn test_tablist_close_active() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        tl.add(Url::parse("https://b.com").unwrap());
        tl.switch_to(0); // active is "a"
        let closed = tl.close_active().unwrap();
        assert_eq!(closed.url.as_str(), "https://a.com/");
        assert_eq!(tl.len(), 1);
        assert_eq!(tl.active_index(), 0);
        assert_eq!(tl.active().url.as_str(), "https://b.com/");
    }

    #[test]
    fn test_tablist_close_last_tab_returns_none() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        assert!(tl.is_single());
        assert!(tl.close(0).is_none());
        assert!(tl.close_active().is_none());
        assert_eq!(tl.len(), 1); // unchanged
    }

    #[test]
    fn test_tablist_close_after_active() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        tl.add(Url::parse("https://b.com").unwrap());
        tl.add(Url::parse("https://c.com").unwrap());
        tl.switch_to(0); // active at 0
        let closed = tl.close(2).unwrap(); // close after active
        assert_eq!(closed.url.as_str(), "https://c.com/");
        assert_eq!(tl.active_index(), 0); // unchanged
    }

    #[test]
    fn test_tablist_close_out_of_bounds() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        assert!(tl.close(99).is_none());
    }

    #[test]
    fn test_tablist_close_by_id() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        let id2 = tl.add(Url::parse("https://b.com").unwrap());
        let closed = tl.close_by_id(id2).unwrap();
        assert_eq!(closed.id, id2);
        assert_eq!(tl.len(), 1);
    }

    #[test]
    fn test_tablist_next_prev() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        tl.add(Url::parse("https://b.com").unwrap());
        tl.add(Url::parse("https://c.com").unwrap());
        tl.switch_to(0);

        tl.next();
        assert_eq!(tl.active_index(), 1);
        tl.next();
        assert_eq!(tl.active_index(), 2);
        tl.next(); // wraps to 0
        assert_eq!(tl.active_index(), 0);

        tl.prev(); // wraps to 2
        assert_eq!(tl.active_index(), 2);
        tl.prev();
        assert_eq!(tl.active_index(), 1);
    }

    #[test]
    fn test_tablist_next_prev_single_tab() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        tl.next(); // no-op
        tl.prev(); // no-op
        assert_eq!(tl.active_index(), 0);
    }

    #[test]
    fn test_tablist_find_index() {
        let tl = TabList::new(Url::parse("https://a.com").unwrap());
        let id = tl.tabs[0].id;
        assert_eq!(tl.find_index(id), Some(0));
        assert_eq!(tl.find_index(Uuid::nil()), None);
    }

    #[test]
    fn test_tablist_get_by_id() {
        let tl = TabList::new(Url::parse("https://a.com").unwrap());
        let id = tl.tabs[0].id;
        let tab = tl.get_by_id(id).unwrap();
        assert_eq!(tab.url.as_str(), "https://a.com/");
    }

    #[test]
    fn test_tablist_iter() {
        let mut tl = TabList::new(Url::parse("https://a.com").unwrap());
        tl.add(Url::parse("https://b.com").unwrap());
        let urls: Vec<&str> = tl.iter().map(|t| t.url.as_str()).collect();
        assert_eq!(urls.len(), 2);
    }

    // ─── Pane tests ─────────────────────────────────────────────────

    #[test]
    fn test_pane_new() {
        let url = Url::parse("https://example.com").unwrap();
        let pane = Pane::new(url.clone());
        assert_eq!(pane.url().as_str(), "https://example.com/");
        assert_eq!(pane.title(), "https://example.com/");
        assert!(pane.session_id.is_none());
        assert_eq!(pane.tabs.len(), 1);
    }

    #[test]
    fn test_pane_unique_ids() {
        let url = Url::parse("https://example.com").unwrap();
        let p1 = Pane::new(url.clone());
        let p2 = Pane::new(url);
        assert_ne!(p1.id, p2.id);
    }

    #[test]
    fn test_pane_with_session() {
        let url = Url::parse("https://example.com").unwrap();
        let pane = Pane::new(url).with_session("session-123".to_string());
        assert_eq!(pane.session_id.as_deref(), Some("session-123"));
    }

    #[test]
    fn test_pane_title_defaults_to_url() {
        let url = Url::parse("aileron://new").unwrap();
        let pane = Pane::new(url);
        assert_eq!(pane.title(), pane.url().to_string());
    }

    #[test]
    fn test_pane_clone() {
        let url = Url::parse("https://example.com").unwrap();
        let pane = Pane::new(url).with_session("sess".into());
        let cloned = pane.clone();
        assert_eq!(pane.id, cloned.id);
        assert_eq!(pane.session_id, cloned.session_id);
        assert_eq!(pane.tabs.len(), cloned.tabs.len());
    }

    #[test]
    fn test_pane_debug() {
        let url = Url::parse("https://example.com").unwrap();
        let pane = Pane::new(url);
        let debug = format!("{pane:?}");
        assert!(debug.contains("Pane"));
        assert!(debug.contains("id:"));
    }

    #[test]
    fn test_pane_add_tab() {
        let url = Url::parse("https://a.com").unwrap();
        let mut pane = Pane::new(url);
        pane.tabs.add(Url::parse("https://b.com").unwrap());
        assert_eq!(pane.tabs.len(), 2);
        assert_eq!(pane.url().as_str(), "https://b.com/");
    }

    #[test]
    fn test_pane_active_tab_id() {
        let url = Url::parse("https://a.com").unwrap();
        let pane = Pane::new(url);
        assert_eq!(pane.active_tab_id(), pane.tabs.active().id);
    }
}
