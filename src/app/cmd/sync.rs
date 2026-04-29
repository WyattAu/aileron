//! Sync command implementations.
//! Free functions that return status messages, keeping commands.rs focused on dispatch.

/// Execute a sync push to the configured target.
pub fn execute_sync_push(sync_target: &str, sync_encrypted: bool) -> String {
    if sync_target.is_empty() {
        return "No sync target set. Use :sync-target <target>".into();
    }
    let target = match crate::sync::SyncTarget::parse(sync_target) {
        Ok(t) => t,
        Err(e) => {
            return format!("Invalid sync target: {}", e);
        }
    };

    let config_dir = crate::config::Config::config_dir();
    let sm = crate::sync::SyncManager::new(config_dir);
    let staging = sm.state_dir().to_path_buf();

    if let Err(e) = std::fs::create_dir_all(&staging) {
        return format!("Failed to create staging dir: {}", e);
    }

    if let Err(e) = sm.create_db_snapshots() {
        return format!("DB snapshot failed: {}", e);
    }

    let prefix = if sync_encrypted { "(encrypted) " } else { "" };
    match crate::sync::transport::push(sm.local_dir(), &staging, &target, sync_encrypted) {
        Ok(n) => {
            let _ = sm.save_manifest();
            format!("Synced {} {}files to {}", n, prefix, target.display())
        }
        Err(e) => {
            format!("Sync push failed: {}", e)
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
            return format!("Invalid sync target: {}", e);
        }
    };

    let config_dir = crate::config::Config::config_dir();
    let sm = crate::sync::SyncManager::new(config_dir);
    let staging = sm.state_dir().join("incoming");
    if let Err(e) = std::fs::create_dir_all(&staging) {
        return format!("Failed to create staging dir: {}", e);
    }

    match crate::sync::transport::pull(sm.local_dir(), &staging, &target, sync_encrypted) {
        Ok(n) => {
            format!("Pulled {} files from {}", n, target.display())
        }
        Err(e) => {
            format!("Sync pull failed: {}", e)
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
    let parts = [
        format!("target: {}", sync_target),
        format!("encrypted: {}", sync_encrypted),
        format!(
            "watcher: {}",
            if watcher_running {
                "running"
            } else {
                "stopped"
            }
        ),
        format!("files: {}", manifest.files.len()),
    ];
    format!("Sync: {}", parts.join(" | "))
}

/// Start the sync file watcher.
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
}
