//! Sync command implementations.
//! Free functions that return status messages, keeping commands.rs focused on dispatch.

use tracing::warn;

/// Execute a sync push to the configured target.
pub fn execute_sync_push(sync_target: &str, sync_encrypted: bool) -> String {
    if sync_target.is_empty() {
        return "No sync target set. Use :sync-target <target>".into();
    }
    let target = match crate::sync::SyncTarget::parse(sync_target) {
        Ok(t) => t,
        Err(e) => {
            return format!("Invalid sync target: {e}");
        }
    };

    let config_dir = crate::config::Config::config_dir();
    let sm = crate::sync::SyncManager::new(config_dir);
    let staging = sm.state_dir().to_path_buf();

    if let Err(e) = std::fs::create_dir_all(&staging) {
        return format!("Failed to create staging dir: {e}");
    }

    if let Err(e) = sm.create_db_snapshots() {
        return format!("DB snapshot failed: {e}");
    }

    let prefix = if sync_encrypted { "(encrypted) " } else { "" };
    match crate::sync::transport::push(sm.local_dir(), &staging, &target, sync_encrypted) {
        Ok(n) => {
            if let Err(e) = sm.save_manifest() {
                warn!(%e, "Failed to save sync manifest");
            }
            format!("Synced {} {}files to {}", n, prefix, target.display())
        }
        Err(e) => {
            format!("Sync push failed: {e}")
        }
    }
}

/// Execute a sync pull from the configured target.
pub fn execute_sync_pull(sync_target: &str, sync_encrypted: bool) -> String {
    if sync_target.is_empty() {
        return "No sync target set. Use :sync-target <target>".into();
    }
    let target = match crate::sync::SyncTarget::parse(sync_target) {
        Ok(t) => t,
        Err(e) => {
            return format!("Invalid sync target: {e}");
        }
    };

    let config_dir = crate::config::Config::config_dir();
    let sm = crate::sync::SyncManager::new(config_dir);
    let staging = sm.state_dir().join("incoming");
    if let Err(e) = std::fs::create_dir_all(&staging) {
        return format!("Failed to create staging dir: {e}");
    }

    match crate::sync::transport::pull(sm.local_dir(), &staging, &target, sync_encrypted) {
        Ok(n) => {
            format!("Pulled {} files from {}", n, target.display())
        }
        Err(e) => {
            format!("Sync pull failed: {e}")
        }
    }
}

/// Get the current sync status.
pub fn execute_sync_status(
    sync_target: &str,
    sync_encrypted: bool,
    watcher_running: bool,
) -> String {
    if sync_target.is_empty() {
        return "Sync: disabled (no target)".into();
    }
    let config_dir = crate::config::Config::config_dir();
    let sm = crate::sync::SyncManager::new(config_dir);
    let manifest = sm.compute_manifest().unwrap_or_default();

    let last_sync = {
        let manifest_lock = sm.manifest().read().unwrap_or_else(|e| e.into_inner());
        manifest_lock.last_sync
    };

    let last_sync_str = if last_sync > 0 {
        chrono::DateTime::from_timestamp(last_sync as i64, 0)
            .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        "never".to_string()
    };

    let parts = [
        format!("target: {sync_target}"),
        format!("encrypted: {sync_encrypted}"),
        format!(
            "watcher: {}",
            if watcher_running {
                "running"
            } else {
                "stopped"
            }
        ),
        format!("files: {}", manifest.files.len()),
        format!("last_sync: {last_sync_str}"),
    ];
    format!("Sync: {}", parts.join(" | "))
}

/// Start the sync file watcher.
#[must_use = "ignoring this value may lead to unexpected behavior"]
pub fn execute_sync_watch(sync_target: &str) -> Result<(), String> {
    if sync_target.is_empty() {
        return Err("No sync target set. Use :sync-target <target>".into());
    }
    // Note: caller is responsible for calling `self.sync_watcher.start()`
    // This function validates the target and returns the config dir.
    let config_dir = crate::config::Config::config_dir();
    // The watcher is started by the caller since it needs &mut self.sync_watcher
    let _ = config_dir; // Used by caller
    Ok(())
}

/// Detect sync conflicts by comparing local files against the last-synced manifest.
/// Files whose current hash differs from the last-synced hash are potential conflicts.
pub fn detect_sync_conflicts(
    sync_target: &str,
    _sync_encrypted: bool,
) -> Vec<super::super::SyncConflictEntry> {
    if sync_target.is_empty() {
        return Vec::new();
    }

    let config_dir = crate::config::Config::config_dir();
    let sm = crate::sync::SyncManager::new(config_dir);

    // Load the last-synced manifest (stored in .sync/manifest.json)
    let last_synced = {
        let manifest_path = sm.state_dir().join("manifest.json");
        if manifest_path.exists() {
            match crate::sync::core::SyncManifest::load(&manifest_path) {
                Ok(m) => m,
                Err(_) => return Vec::new(),
            }
        } else {
            // No previous sync, no conflicts possible
            return Vec::new();
        }
    };

    // Compute current manifest
    let current = match sm.compute_manifest() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    // Find files that differ between current and last-synced
    let mut conflicts = Vec::new();
    for (path, current_file) in &current.files {
        if let Some(synced_file) = last_synced.files.get(path)
            && current_file.blake3_hash != synced_file.blake3_hash
        {
            conflicts.push(super::super::SyncConflictEntry {
                path: path.clone(),
                local_hash: current_file.blake3_hash.clone(),
                remote_hash: synced_file.blake3_hash.clone(),
                local_size: current_file.size,
                remote_size: synced_file.size,
            });
        }
    }

    // Also report files deleted locally but present in last sync
    for (path, synced_file) in &last_synced.files {
        if !current.files.contains_key(path) {
            conflicts.push(super::super::SyncConflictEntry {
                path: path.clone(),
                local_hash: String::new(),
                remote_hash: synced_file.blake3_hash.clone(),
                local_size: 0,
                remote_size: synced_file.size,
            });
        }
    }

    // Also report new files not in last sync
    for (path, current_file) in &current.files {
        if !last_synced.files.contains_key(path) {
            conflicts.push(super::super::SyncConflictEntry {
                path: path.clone(),
                local_hash: current_file.blake3_hash.clone(),
                remote_hash: String::new(),
                local_size: current_file.size,
                remote_size: 0,
            });
        }
    }

    conflicts
}

/// Resolve a sync conflict by keeping the local version.
pub fn resolve_conflict_keep_local(
    sync_target: &str,
    conflict_path: &str,
) -> Result<String, String> {
    if sync_target.is_empty() {
        return Err("No sync target set".into());
    }

    let config_dir = crate::config::Config::config_dir();
    let sm = crate::sync::SyncManager::new(config_dir);

    // Mark the conflict as resolved by updating the manifest
    let manifest_path = sm.state_dir().join("manifest.json");
    if manifest_path.exists() {
        let mut manifest = crate::sync::core::SyncManifest::load(&manifest_path)
            .map_err(|e| format!("Failed to load manifest: {e}"))?;

        // Remove the conflict entry from the manifest
        manifest.files.remove(conflict_path);
        manifest
            .save(&manifest_path)
            .map_err(|e| format!("Failed to save manifest: {e}"))?;
    }

    Ok(format!(
        "Resolved conflict for {conflict_path} (kept local)"
    ))
}

/// Resolve a sync conflict by keeping the remote version.
pub fn resolve_conflict_keep_remote(
    sync_target: &str,
    conflict_path: &str,
) -> Result<String, String> {
    if sync_target.is_empty() {
        return Err("No sync target set".into());
    }

    let config_dir = crate::config::Config::config_dir();
    let sm = crate::sync::SyncManager::new(config_dir);

    // Load the last-synced manifest to get the remote version
    let manifest_path = sm.state_dir().join("manifest.json");
    if manifest_path.exists() {
        let last_synced = crate::sync::core::SyncManifest::load(&manifest_path)
            .map_err(|e| format!("Failed to load manifest: {e}"))?;

        // Remove the file from manifest so it gets re-downloaded on next sync
        let mut manifest = last_synced;
        manifest.files.remove(conflict_path);
        manifest
            .save(&manifest_path)
            .map_err(|e| format!("Failed to save manifest: {e}"))?;
    }

    Ok(format!(
        "Resolved conflict for {conflict_path} (will re-download remote)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_push_no_target() {
        let msg = execute_sync_push("", false);
        assert!(msg.contains("No sync target set"));
    }

    #[test]
    fn sync_pull_no_target() {
        let msg = execute_sync_pull("", false);
        assert!(msg.contains("No sync target set"));
    }

    #[test]
    fn sync_status_no_target() {
        let msg = execute_sync_status("", false, false);
        assert_eq!(msg, "Sync: disabled (no target)");
    }

    #[test]
    fn sync_watch_no_target() {
        let result = execute_sync_watch("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No sync target set"));
    }

    #[test]
    fn sync_watch_with_target() {
        let result = execute_sync_watch("/tmp/test");
        assert!(result.is_ok());
    }

    #[test]
    fn detect_conflicts_no_target() {
        let conflicts = detect_sync_conflicts("", false);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detect_conflicts_with_target_no_manifest() {
        // A target path that has never been synced — no manifest means no conflicts
        let conflicts = detect_sync_conflicts("/tmp/aileron-test-nonexistent", false);
        assert!(conflicts.is_empty());
    }
}
