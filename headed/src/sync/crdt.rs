//! CRDT (Conflict-free Replicated Data Type) merge for sync.
//!
//! Implements LWW-Element-Set for bookmarks/settings and append-only log for history.
//! These are the core data structures for conflict-free multi-device synchronization.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A timestamp used for Last-Writer-Wins conflict resolution.
/// Uses hybrid logical clock (HLC) style: (physical_time, device_id) for total ordering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct HlcTimestamp {
    /// Physical time as milliseconds since Unix epoch.
    pub physical_time: u64,
    /// Device ID for tie-breaking (lexicographic comparison).
    pub device_id: String,
}

/// An item in a LWW-Element-Set.
/// Both the "add" set and "remove" set contain these entries.
/// An item is considered "in the set" if it's in the add set with a timestamp
/// greater than any matching entry in the remove set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LwwEntry<T> {
    /// Unique identifier for this item.
    pub id: String,
    /// The data payload.
    pub data: T,
    /// When this entry was last modified.
    pub timestamp: HlcTimestamp,
    /// Whether this entry has been tombstoned (deleted).
    pub tombstoned: bool,
}

/// LWW-Element-Set for bookmarks and settings.
/// Provides conflict-free merge by keeping the latest write for each item ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LwwElementSet<T> {
    entries: HashMap<String, LwwEntry<T>>,
}

impl<T: Clone> Default for LwwElementSet<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<T: Clone> LwwElementSet<T> {
    /// Create an empty LWW-Element-Set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or update an entry. Uses LWW: if the new timestamp is >= existing, replace.
    pub fn upsert(&mut self, id: String, data: T, timestamp: HlcTimestamp) {
        let should_insert = match self.entries.get(&id) {
            Some(existing) => timestamp >= existing.timestamp,
            None => true,
        };

        if should_insert {
            let id_clone = id.clone();
            self.entries.insert(
                id,
                LwwEntry {
                    id: id_clone,
                    data,
                    timestamp,
                    tombstoned: false,
                },
            );
        }
    }
    /// Tombstone (soft-delete) an entry.
    pub fn remove(&mut self, id: &str, timestamp: HlcTimestamp) {
        if let Some(entry) = self.entries.get_mut(id)
            && timestamp >= entry.timestamp
        {
            entry.tombstoned = true;
            entry.timestamp = timestamp;
        }
    }

    /// Get a non-tombstoned entry.
    pub fn get(&self, id: &str) -> Option<&T> {
        self.entries
            .get(id)
            .filter(|e| !e.tombstoned)
            .map(|e| &e.data)
    }

    /// Get all non-tombstoned entries.
    pub fn get_all(&self) -> Vec<&T> {
        self.entries
            .values()
            .filter(|e| !e.tombstoned)
            .map(|e| &e.data)
            .collect()
    }

    /// Get all entries (including tombstoned) for sync.
    /// Returns references to all entries with their full metadata for serialization.
    pub fn get_all_with_metadata(&self) -> Vec<&LwwEntry<T>> {
        self.entries.values().collect()
    }

    /// Get all raw entries.
    pub fn entries(&self) -> &HashMap<String, LwwEntry<T>> {
        &self.entries
    }

    /// Merge another LWW-Element-Set into this one using LWW semantics.
    pub fn merge(&mut self, other: &LwwElementSet<T>) {
        for (id, entry) in &other.entries {
            match self.entries.get(id) {
                Some(existing) => {
                    if entry.timestamp > existing.timestamp {
                        self.entries.insert(id.clone(), entry.clone());
                    }
                }
                None => {
                    self.entries.insert(id.clone(), entry.clone());
                }
            }
        }
    }

    /// Count of active (non-tombstoned) entries.
    pub fn active_count(&self) -> usize {
        self.entries.values().filter(|e| !e.tombstoned).count()
    }

    /// Count of all entries including tombstones.
    pub fn total_count(&self) -> usize {
        self.entries.len()
    }

    /// Remove tombstones older than the given cutoff time.
    pub fn garbage_collect(&mut self, cutoff: &HlcTimestamp) {
        self.entries
            .retain(|_, entry| !entry.tombstoned || &entry.timestamp >= cutoff);
    }
}

impl<T: Clone + PartialEq> PartialEq for LwwElementSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self.entries.iter().all(|(k, v)| {
                other
                    .entries
                    .get(k)
                    .is_some_and(|ov| v.data == ov.data && v.tombstoned == ov.tombstoned)
            })
    }
}

/// A history event in the append-only log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEvent {
    /// Unique event ID (UUID).
    pub id: String,
    /// URL visited.
    pub url: String,
    /// Page title.
    pub title: Option<String>,
    /// Visit timestamp (ms since epoch).
    pub visit_time: u64,
    /// Device that created this event.
    pub device_id: String,
}

/// Append-only log for history entries.
/// History uses a set-based approach: each (url, visit_time) pair is unique.
/// Merge is simply the union of both sets with deduplication by event ID.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryLog {
    events: HashMap<String, HistoryEvent>,
}

impl HistoryLog {
    /// Create an empty history log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a history event. If an event with the same ID exists, keeps the newer one.
    pub fn append(&mut self, event: HistoryEvent) {
        self.events.entry(event.id.clone()).or_insert(event);
    }

    /// Get all events sorted by visit time (most recent first).
    pub fn get_all_sorted(&self) -> Vec<&HistoryEvent> {
        let mut events: Vec<_> = self.events.values().collect();
        events.sort_by_key(|e| std::cmp::Reverse(e.visit_time));
        events
    }

    /// Get recent events (up to `limit`).
    pub fn get_recent(&self, limit: usize) -> Vec<&HistoryEvent> {
        self.get_all_sorted().into_iter().take(limit).collect()
    }

    /// Merge another history log into this one. Union with dedup by event ID.
    pub fn merge(&mut self, other: &HistoryLog) {
        for (id, event) in &other.events {
            self.events
                .entry(id.clone())
                .or_insert_with(|| event.clone());
        }
    }

    /// Count of events.
    pub fn count(&self) -> usize {
        self.events.len()
    }

    /// Search events by URL substring.
    pub fn search_by_url(&self, query: &str) -> Vec<&HistoryEvent> {
        let query = query.to_ascii_lowercase();
        self.events
            .values()
            .filter(|e| e.url.to_ascii_lowercase().contains(&query))
            .collect()
    }

    /// Prune events older than the given cutoff time.
    pub fn prune_older_than(&mut self, cutoff_ms: u64) {
        self.events.retain(|_, e| e.visit_time >= cutoff_ms);
    }
}

impl PartialEq for HistoryLog {
    fn eq(&self, other: &Self) -> bool {
        self.events.len() == other.events.len()
            && self.events.keys().all(|k| other.events.contains_key(k))
    }
}

/// Bookmark data for the LWW-Element-Set.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BookmarkData {
    pub title: String,
    pub url: String,
    pub parent_folder_id: Option<String>,
    pub position: u32,
}

/// Setting data for the LWW-Element-Set.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettingData {
    pub key: String,
    pub value: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(time: u64, device: &str) -> HlcTimestamp {
        HlcTimestamp {
            physical_time: time,
            device_id: device.to_string(),
        }
    }

    #[test]
    fn test_lww_upsert_new() {
        let mut set: LwwElementSet<String> = LwwElementSet::new();
        set.upsert("k1".into(), "v1".into(), ts(100, "dev1"));
        assert_eq!(set.get("k1"), Some(&"v1".to_string()));
    }

    #[test]
    fn test_lww_upsert_newer_wins() {
        let mut set: LwwElementSet<String> = LwwElementSet::new();
        set.upsert("k1".into(), "old".into(), ts(100, "dev1"));
        set.upsert("k1".into(), "new".into(), ts(200, "dev1"));
        assert_eq!(set.get("k1"), Some(&"new".to_string()));
    }

    #[test]
    fn test_lww_upsert_older_loses() {
        let mut set: LwwElementSet<String> = LwwElementSet::new();
        set.upsert("k1".into(), "new".into(), ts(200, "dev1"));
        set.upsert("k1".into(), "old".into(), ts(100, "dev1"));
        assert_eq!(set.get("k1"), Some(&"new".to_string()));
    }

    #[test]
    fn test_lww_upsert_same_time_device_tiebreak() {
        let mut set: LwwElementSet<String> = LwwElementSet::new();
        set.upsert("k1".into(), "from_a".into(), ts(100, "devA"));
        // devB > devA lexicographically, so devB wins on same timestamp
        set.upsert("k1".into(), "from_b".into(), ts(100, "devB"));
        assert_eq!(set.get("k1"), Some(&"from_b".to_string()));
    }

    #[test]
    fn test_lww_remove() {
        let mut set: LwwElementSet<String> = LwwElementSet::new();
        set.upsert("k1".into(), "v1".into(), ts(100, "dev1"));
        set.remove("k1", ts(200, "dev1"));
        assert_eq!(set.get("k1"), None);
        assert_eq!(set.active_count(), 0);
        // Entry still exists as tombstone
        assert_eq!(set.total_count(), 1);
    }

    #[test]
    fn test_lww_remove_older_ignored() {
        let mut set: LwwElementSet<String> = LwwElementSet::new();
        set.upsert("k1".into(), "v1".into(), ts(200, "dev1"));
        set.remove("k1", ts(100, "dev1")); // older, should not tombstone
        assert_eq!(set.get("k1"), Some(&"v1".to_string()));
    }

    #[test]
    fn test_lww_merge() {
        let mut set_a: LwwElementSet<String> = LwwElementSet::new();
        set_a.upsert("k1".into(), "a_v1".into(), ts(100, "dev1"));
        set_a.upsert("k2".into(), "a_v2".into(), ts(100, "dev1"));

        let mut set_b: LwwElementSet<String> = LwwElementSet::new();
        set_b.upsert("k1".into(), "b_v1_newer".into(), ts(200, "dev2"));
        set_b.upsert("k3".into(), "b_v3".into(), ts(150, "dev2"));

        set_a.merge(&set_b);

        assert_eq!(set_a.get("k1"), Some(&"b_v1_newer".to_string()));
        assert_eq!(set_a.get("k2"), Some(&"a_v2".to_string()));
        assert_eq!(set_a.get("k3"), Some(&"b_v3".to_string()));
    }

    #[test]
    fn test_lww_merge_with_tombstone() {
        let mut set_a: LwwElementSet<String> = LwwElementSet::new();
        set_a.upsert("k1".into(), "v1".into(), ts(100, "dev1"));

        let mut set_b: LwwElementSet<String> = LwwElementSet::new();
        set_b.upsert("k1".into(), "v1".into(), ts(100, "dev1"));
        set_b.remove("k1", ts(200, "dev1"));

        set_a.merge(&set_b);
        assert_eq!(set_a.get("k1"), None);
    }

    #[test]
    fn test_lww_get_all() {
        let mut set: LwwElementSet<String> = LwwElementSet::new();
        set.upsert("k1".into(), "v1".into(), ts(100, "dev1"));
        set.upsert("k2".into(), "v2".into(), ts(100, "dev1"));
        set.upsert("k3".into(), "v3".into(), ts(100, "dev1"));
        set.remove("k2", ts(200, "dev1"));

        let all = set.get_all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_lww_garbage_collect() {
        let mut set: LwwElementSet<String> = LwwElementSet::new();
        set.upsert("k1".into(), "v1".into(), ts(100, "dev1"));
        set.remove("k1", ts(200, "dev1"));
        set.upsert("k2".into(), "v2".into(), ts(300, "dev1"));

        // GC tombstones older than 250
        set.garbage_collect(&ts(250, "dev1"));

        // k1 tombstone (ts=200) should be removed
        assert_eq!(set.total_count(), 1); // Only k2 remains
    }

    #[test]
    fn test_lww_equality() {
        let mut set_a: LwwElementSet<String> = LwwElementSet::new();
        set_a.upsert("k1".into(), "v1".into(), ts(100, "dev1"));

        let mut set_b: LwwElementSet<String> = LwwElementSet::new();
        set_b.upsert("k1".into(), "v1".into(), ts(100, "dev1"));

        assert_eq!(set_a, set_b);
    }

    #[test]
    fn test_history_append() {
        let mut log = HistoryLog::new();
        log.append(HistoryEvent {
            id: "evt1".into(),
            url: "https://example.com".into(),
            title: Some("Example".into()),
            visit_time: 1000,
            device_id: "dev1".into(),
        });
        assert_eq!(log.count(), 1);
    }

    #[test]
    fn test_history_dedup_by_id() {
        let mut log = HistoryLog::new();
        log.append(HistoryEvent {
            id: "evt1".into(),
            url: "https://example.com".into(),
            title: Some("First".into()),
            visit_time: 1000,
            device_id: "dev1".into(),
        });
        // Same ID, should not duplicate
        log.append(HistoryEvent {
            id: "evt1".into(),
            url: "https://example.com".into(),
            title: Some("First".into()),
            visit_time: 1000,
            device_id: "dev1".into(),
        });
        assert_eq!(log.count(), 1);
    }

    #[test]
    fn test_history_merge() {
        let mut log_a = HistoryLog::new();
        log_a.append(HistoryEvent {
            id: "evt1".into(),
            url: "https://a.com".into(),
            title: None,
            visit_time: 1000,
            device_id: "dev1".into(),
        });

        let mut log_b = HistoryLog::new();
        log_b.append(HistoryEvent {
            id: "evt2".into(),
            url: "https://b.com".into(),
            title: None,
            visit_time: 2000,
            device_id: "dev2".into(),
        });
        // Shared event
        log_b.append(HistoryEvent {
            id: "evt1".into(),
            url: "https://a.com".into(),
            title: None,
            visit_time: 1000,
            device_id: "dev1".into(),
        });

        log_a.merge(&log_b);
        assert_eq!(log_a.count(), 2);
    }

    #[test]
    fn test_history_sorted() {
        let mut log = HistoryLog::new();
        log.append(HistoryEvent {
            id: "evt1".into(),
            url: "https://old.com".into(),
            title: None,
            visit_time: 1000,
            device_id: "dev1".into(),
        });
        log.append(HistoryEvent {
            id: "evt2".into(),
            url: "https://new.com".into(),
            title: None,
            visit_time: 2000,
            device_id: "dev1".into(),
        });

        let sorted = log.get_all_sorted();
        assert_eq!(sorted[0].url, "https://new.com");
        assert_eq!(sorted[1].url, "https://old.com");
    }

    #[test]
    fn test_history_get_recent() {
        let mut log = HistoryLog::new();
        for i in 0..10 {
            log.append(HistoryEvent {
                id: format!("evt{i}"),
                url: format!("https://example.com/{i}"),
                title: None,
                visit_time: i as u64 * 1000,
                device_id: "dev1".into(),
            });
        }
        let recent = log.get_recent(3);
        assert_eq!(recent.len(), 3);
        // Most recent first
        assert_eq!(recent[0].id, "evt9");
    }

    #[test]
    fn test_history_search_by_url() {
        let mut log = HistoryLog::new();
        log.append(HistoryEvent {
            id: "evt1".into(),
            url: "https://example.com/page1".into(),
            title: None,
            visit_time: 1000,
            device_id: "dev1".into(),
        });
        log.append(HistoryEvent {
            id: "evt2".into(),
            url: "https://other.com/page2".into(),
            title: None,
            visit_time: 2000,
            device_id: "dev1".into(),
        });

        let results = log.search_by_url("example");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "evt1");
    }

    #[test]
    fn test_history_prune() {
        let mut log = HistoryLog::new();
        log.append(HistoryEvent {
            id: "old".into(),
            url: "https://old.com".into(),
            title: None,
            visit_time: 500,
            device_id: "dev1".into(),
        });
        log.append(HistoryEvent {
            id: "new".into(),
            url: "https://new.com".into(),
            title: None,
            visit_time: 1500,
            device_id: "dev1".into(),
        });

        log.prune_older_than(1000);
        assert_eq!(log.count(), 1);
        assert_eq!(log.get_all_sorted()[0].id, "new");
    }

    #[test]
    fn test_bookmark_data() {
        let bookmark = BookmarkData {
            title: "Example".into(),
            url: "https://example.com".into(),
            parent_folder_id: Some("folder1".into()),
            position: 0,
        };
        assert_eq!(bookmark.title, "Example");
    }

    #[test]
    fn test_setting_data() {
        let setting = SettingData {
            key: "homepage".into(),
            value: serde_json::json!("https://example.com"),
        };
        assert_eq!(setting.key, "homepage");
    }

    #[test]
    fn test_hlc_timestamp_ordering() {
        let a = ts(1000, "dev1");
        let b = ts(2000, "dev1");
        let c = ts(1000, "dev2");
        assert!(a < b);
        assert!(a < c); // same time, dev2 > dev1
    }
}
