use std::collections::HashMap;

use crate::extensions::types::Result;

/// Keys to retrieve from storage.
#[derive(Debug, Clone)]
pub enum StorageGetKeys {
    Single(String),
    Multiple(Vec<String>),
    All,
}

/// A map of key-value pairs for storage operations.
pub type StorageChanges = HashMap<String, serde_json::Value>;

/// Storage change event data.
#[derive(Debug, Clone)]
pub struct StorageChange {
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
}

/// A storage area with get/set/remove/clear operations.
pub trait StorageArea: Send + Sync {
    fn get(&self, keys: StorageGetKeys) -> Result<StorageChanges>;

    fn set(&self, items: StorageChanges) -> Result<()>;

    fn remove(&self, keys: Vec<String>) -> Result<()>;

    fn clear(&self) -> Result<()>;

    fn get_bytes_in_use(&self, keys: Option<Vec<String>>) -> Result<u64>;

    fn on_changed(&self, callback: Box<dyn Fn(StorageChanges, String) + Send + Sync>);
}

/// Key-value storage for extensions.
pub trait StorageApi: Send + Sync {
    fn local(&self) -> &dyn StorageArea;

    fn sync(&self) -> &dyn StorageArea;

    fn managed(&self) -> &dyn StorageArea;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_get_keys_single() {
        let keys = StorageGetKeys::Single("foo".into());
        match keys {
            StorageGetKeys::Single(s) => assert_eq!(s, "foo"),
            _ => panic!("Expected Single"),
        }
    }

    #[test]
    fn test_storage_get_keys_multiple() {
        let keys = StorageGetKeys::Multiple(vec!["a".into(), "b".into()]);
        match keys {
            StorageGetKeys::Multiple(v) => assert_eq!(v.len(), 2),
            _ => panic!("Expected Multiple"),
        }
    }

    #[test]
    fn test_storage_get_keys_all() {
        let keys = StorageGetKeys::All;
        match keys {
            StorageGetKeys::All => {}
            _ => panic!("Expected All"),
        }
    }

    #[test]
    fn test_storage_changes_type() {
        let mut changes: StorageChanges = HashMap::new();
        changes.insert("key1".into(), serde_json::Value::String("value1".into()));
        changes.insert("key2".into(), serde_json::json!(42));
        assert_eq!(changes.len(), 2);
    }

    #[test]
    fn test_storage_change() {
        let change = StorageChange {
            old_value: Some(serde_json::Value::String("old".into())),
            new_value: Some(serde_json::Value::String("new".into())),
        };
        assert!(change.old_value.is_some());
        assert!(change.new_value.is_some());
    }
}
