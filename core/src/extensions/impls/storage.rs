use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::extensions::permissions::Permission;
use crate::extensions::storage::{StorageApi, StorageArea, StorageChanges, StorageGetKeys};
use crate::extensions::types::{ExtensionError, ExtensionId, Result};

use super::StorageChangeCallback;

pub(super) struct AileronStorageArea {
    data: Mutex<HashMap<String, serde_json::Value>>,
    change_callbacks: Mutex<Vec<StorageChangeCallback>>,
    /// If set, data is persisted to this JSON file on every mutation.
    storage_file: Option<std::path::PathBuf>,
    /// Permissions granted to the owning extension.
    granted_permissions: std::collections::HashSet<Permission>,
}

impl AileronStorageArea {
    pub(super) fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
            change_callbacks: Mutex::new(Vec::new()),
            storage_file: None,
            granted_permissions: std::collections::HashSet::new(),
        }
    }

    /// Create a persistent storage area backed by a JSON file.
    /// If the file exists, data is loaded from it on creation.
    /// If the file does not exist, an empty area is created and the
    /// file will be written on the first mutation.
    pub(super) fn with_persistence(storage_file: std::path::PathBuf) -> Self {
        let initial_data = Self::load_from_file(&storage_file);
        Self {
            data: Mutex::new(initial_data),
            change_callbacks: Mutex::new(Vec::new()),
            storage_file: Some(storage_file),
            granted_permissions: std::collections::HashSet::new(),
        }
    }

    pub(super) fn set_permissions(&mut self, permissions: std::collections::HashSet<Permission>) {
        self.granted_permissions = permissions;
    }

    fn has_storage_permission(&self) -> bool {
        self.granted_permissions.contains(&Permission::Storage)
    }

    fn load_from_file(path: &std::path::Path) -> HashMap<String, serde_json::Value> {
        if !path.exists() {
            return HashMap::new();
        }
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(data) => data,
                Err(e) => {
                    tracing::warn!(
                        target: "extensions",
                        "Failed to parse storage file {:?}: {}, starting empty",
                        path, e
                    );
                    HashMap::new()
                }
            },
            Err(e) => {
                tracing::warn!(
                    target: "extensions",
                    "Failed to read storage file {:?}: {}, starting empty",
                    path, e
                );
                HashMap::new()
            }
        }
    }

    fn persist_to_file(&self) {
        if let Some(ref path) = self.storage_file {
            let data = self.data.lock().unwrap_or_else(|e| e.into_inner());
            // Only write if we have data (avoid creating empty files unnecessarily)
            if data.is_empty() {
                // Remove the file if it exists and data is empty after clear
                let _ = std::fs::remove_file(path);
                return;
            }
            match serde_json::to_string_pretty(&*data) {
                Ok(json) => {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(path, &json) {
                        tracing::warn!(
                            target: "extensions",
                            "Failed to write storage file {:?}: {}",
                            path, e
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        target: "extensions",
                        "Failed to serialize storage data: {}",
                        e
                    );
                }
            }
        }
    }

    /// Fire change callbacks for the given changes.
    fn fire_change_callbacks(&self, changes: StorageChanges, area_name: String) {
        if changes.is_empty() {
            return;
        }
        let callbacks: Vec<_> = self
            .change_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect();
        for cb in callbacks {
            cb(changes.clone(), area_name.clone());
        }
    }
}

impl StorageArea for AileronStorageArea {
    fn get(&self, keys: StorageGetKeys) -> Result<StorageChanges> {
        let data = self
            .data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {e}")))?;
        let result = match keys {
            StorageGetKeys::Single(key) => {
                let mut map = HashMap::new();
                if let Some(value) = data.get(&key) {
                    map.insert(key, value.clone());
                }
                map
            }
            StorageGetKeys::Multiple(keys) => {
                let mut map = HashMap::new();
                for key in keys {
                    if let Some(value) = data.get(&key) {
                        map.insert(key, value.clone());
                    }
                }
                map
            }
            StorageGetKeys::All => data.clone(),
        };
        Ok(result)
    }

    fn set(&self, items: StorageChanges) -> Result<()> {
        if !self.has_storage_permission() {
            tracing::warn!(
                target: "extensions",
                "storage.local.set: denied — 'storage' permission not granted"
            );
            return Ok(());
        }
        let mut data = self
            .data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {e}")))?;
        let mut changes = StorageChanges::new();
        for (key, new_value) in items {
            data.insert(key.clone(), new_value.clone());
            changes.insert(key, new_value);
        }
        drop(data);
        self.fire_change_callbacks(changes, "local".into());
        self.persist_to_file();
        Ok(())
    }

    fn remove(&self, keys: Vec<String>) -> Result<()> {
        if !self.has_storage_permission() {
            tracing::warn!(
                target: "extensions",
                "storage.local.remove: denied — 'storage' permission not granted"
            );
            return Ok(());
        }
        let mut data = self
            .data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {e}")))?;
        let mut changes = StorageChanges::new();
        for key in keys {
            if data.remove(&key).is_some() {
                // Use null to indicate removal in changes
                changes.insert(key, serde_json::Value::Null);
            }
        }
        drop(data);
        self.fire_change_callbacks(changes, "local".into());
        self.persist_to_file();
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        if !self.has_storage_permission() {
            tracing::warn!(
                target: "extensions",
                "storage.local.clear: denied — 'storage' permission not granted"
            );
            return Ok(());
        }
        let mut data = self
            .data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {e}")))?;
        if data.is_empty() {
            return Ok(());
        }
        data.clear();
        drop(data);
        // Fire with empty changes to signal clear occurred
        self.fire_change_callbacks(StorageChanges::new(), "local".into());
        self.persist_to_file();
        Ok(())
    }

    fn get_bytes_in_use(&self, keys: Option<Vec<String>>) -> Result<u64> {
        let data = self
            .data
            .lock()
            .map_err(|e| ExtensionError::Runtime(format!("Storage lock poisoned: {e}")))?;
        let bytes: usize = match keys {
            Some(keys) => keys
                .iter()
                .filter_map(|k| data.get(k))
                .map(|v| v.to_string().len())
                .sum(),
            None => data.values().map(|v| v.to_string().len()).sum(),
        };
        Ok(bytes as u64)
    }

    fn on_changed(&self, callback: Arc<dyn Fn(StorageChanges, String) + Send + Sync>) {
        self.change_callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(callback);
    }
}

pub(super) struct AileronStorageApi {
    local: AileronStorageArea,
    sync: AileronStorageArea,
    managed: AileronStorageArea,
}

impl AileronStorageApi {
    /// Create an in-memory (non-persistent) storage API.
    pub(super) fn new() -> Self {
        Self {
            local: AileronStorageArea::new(),
            sync: AileronStorageArea::new(),
            managed: AileronStorageArea::new(),
        }
    }

    /// Create a persistent storage API backed by JSON files.
    /// Files are stored under `storage_dir/<extension_id>/<area>.json`.
    pub(super) fn with_persistence(
        storage_dir: std::path::PathBuf,
        extension_id: &ExtensionId,
    ) -> Self {
        let ext_dir = storage_dir.join(&extension_id.0);
        Self {
            local: AileronStorageArea::with_persistence(ext_dir.join("local.json")),
            sync: AileronStorageArea::with_persistence(ext_dir.join("sync.json")),
            managed: AileronStorageArea::with_persistence(ext_dir.join("managed.json")),
        }
    }

    pub(super) fn set_permissions(&mut self, permissions: std::collections::HashSet<Permission>) {
        self.local.set_permissions(permissions.clone());
        self.sync.set_permissions(permissions.clone());
        self.managed.set_permissions(permissions);
    }
}

impl StorageApi for AileronStorageApi {
    fn local(&self) -> &dyn StorageArea {
        &self.local
    }

    fn sync(&self) -> &dyn StorageArea {
        &self.sync
    }

    fn managed(&self) -> &dyn StorageArea {
        &self.managed
    }
}
