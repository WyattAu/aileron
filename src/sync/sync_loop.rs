//! Sync execution loop — orchestrates the full sync pipeline.
//!
//! Pulls remote state, computes delta, encrypts, uploads, and applies changes.
//! Supports periodic sync, event-driven sync, and manual triggers.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use tracing::{debug, error, info, warn};

use super::core::{DeltaAction, SyncManager, SyncManifest};
use super::crdt::{BookmarkData, HistoryLog, LwwElementSet};
use super::crypto;
use super::transport::SyncTarget;
use super::webdav::{WebdavClient, WebdavConfig};

/// Status of a sync operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    Idle,
    InProgress,
    Success {
        files_uploaded: u64,
        files_downloaded: u64,
        files_deleted: u64,
    },
    Error(String),
}

/// Configuration for the sync loop.
#[derive(Debug, Clone)]
pub struct SyncLoopConfig {
    /// Local data directory to sync.
    pub local_dir: PathBuf,
    /// Remote sync target configuration.
    pub webdav_config: Option<WebdavConfig>,
    /// Legacy transport target (Local/SSH).
    pub legacy_target: Option<SyncTarget>,
    /// Encryption passphrase.
    pub passphrase: Option<String>,
    /// Sync interval in seconds (0 = manual only).
    pub sync_interval_secs: u64,
}

/// Manages the full sync lifecycle.
pub struct SyncLoop {
    config: SyncLoopConfig,
    manager: Arc<SyncManager>,
    status: Mutex<SyncStatus>,
    bookmarks: Mutex<LwwElementSet<BookmarkData>>,
    history: Mutex<HistoryLog>,
}

impl SyncLoop {
    /// Create a new sync loop.
    pub fn new(config: SyncLoopConfig) -> Self {
        let manager = SyncManager::new(config.local_dir.clone());
        Self {
            config,
            manager: Arc::new(manager),
            status: Mutex::new(SyncStatus::Idle),
            bookmarks: Mutex::new(LwwElementSet::new()),
            history: Mutex::new(HistoryLog::new()),
        }
    }

    /// Get the current sync status.
    pub fn status(&self) -> SyncStatus {
        self.status.lock().clone()
    }

    /// Get a reference to the bookmark CRDT.
    pub fn bookmarks(&self) -> &Mutex<LwwElementSet<BookmarkData>> {
        &self.bookmarks
    }

    /// Get a reference to the history log.
    pub fn history(&self) -> &Mutex<HistoryLog> {
        &self.history
    }

    /// Execute a full sync: pull remote, compute delta, push changes.
    pub fn sync(&self) -> SyncStatus {
        // Set status to in-progress
        *self.status.lock() = SyncStatus::InProgress;

        let result = self.execute_sync();

        let final_status = match result {
            Ok(metrics) => SyncStatus::Success {
                files_uploaded: metrics.files_uploaded,
                files_downloaded: metrics.files_downloaded,
                files_deleted: metrics.files_deleted,
            },
            Err(e) => {
                error!("Sync failed: {e}");
                SyncStatus::Error(e.to_string())
            }
        };

        *self.status.lock() = final_status.clone();
        final_status
    }

    fn execute_sync(&self) -> anyhow::Result<SyncMetrics> {
        let mut metrics = SyncMetrics::default();

        // Step 1: Create DB snapshots
        debug!("Creating database snapshots...");
        self.manager.create_db_snapshots()?;

        // Step 2: Compute local manifest
        debug!("Computing local manifest...");
        let local_manifest = self.manager.compute_manifest();

        // Step 3: Get remote manifest
        let remote_manifest = self.fetch_remote_manifest()?;

        // Step 4: Compute delta
        let delta = match &local_manifest {
            Ok(_local) => {
                let delta = self
                    .manager
                    .compute_delta(remote_manifest.as_ref().unwrap_or(&SyncManifest::default()));
                debug!("Delta: {} actions", delta.len());
                delta
            }
            Err(e) => {
                warn!("Failed to compute local manifest: {e}");
                return Err(anyhow::anyhow!("Local manifest computation failed: {e}"));
            }
        };

        // Step 5: Execute delta actions
        let local_manifest = local_manifest?;
        for action in &delta {
            match action {
                DeltaAction::Upload(path) => {
                    if self.upload_file(path, &local_manifest)? {
                        metrics.files_uploaded += 1;
                    }
                }
                DeltaAction::UploadChunks(path, chunks) => {
                    debug!("Uploading {} chunks for {}", chunks.len(), path);
                    if self.upload_file(path, &local_manifest)? {
                        metrics.files_uploaded += 1;
                    }
                }
                DeltaAction::Download(path) => {
                    if self.download_and_apply(path)? {
                        metrics.files_downloaded += 1;
                    }
                }
                DeltaAction::DeleteLocal(path) => {
                    let full_path = self.manager.local_dir().join(path);
                    if full_path.exists() {
                        std::fs::remove_file(&full_path)?;
                        metrics.files_deleted += 1;
                        debug!("Deleted local file: {path}");
                    }
                }
            }
        }

        // Step 6: Upload updated manifest
        self.manager.update_manifest(local_manifest);
        self.manager.save_manifest()?;

        // Step 7: Upload manifest to remote
        self.upload_manifest()?;

        info!(
            "Sync complete: {} uploaded, {} downloaded, {} deleted",
            metrics.files_uploaded, metrics.files_downloaded, metrics.files_deleted
        );

        Ok(metrics)
    }

    fn fetch_remote_manifest(&self) -> anyhow::Result<Option<SyncManifest>> {
        if let Some(ref webdav_config) = self.config.webdav_config {
            let client = WebdavClient::new(webdav_config.clone());
            match client.get(".sync/manifest.json")? {
                Some(data) => {
                    let manifest: SyncManifest = serde_json::from_slice(&data)?;
                    Ok(Some(manifest))
                }
                None => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    fn upload_file(&self, relative_path: &str, _manifest: &SyncManifest) -> anyhow::Result<bool> {
        let full_path = self.manager.local_dir().join(relative_path);
        if !full_path.exists() {
            return Ok(false);
        }

        let data = std::fs::read(&full_path)?;

        // Encrypt if passphrase is set
        let data = if let Some(ref passphrase) = self.config.passphrase {
            crypto::encrypt_data(&data, passphrase)?.into_bytes()
        } else {
            data
        };

        if let Some(ref webdav_config) = self.config.webdav_config {
            let client = WebdavClient::new(webdav_config.clone());
            let remote_path = format!("data/{relative_path}");
            client.ensure_collection("data")?;
            client.put(&remote_path, &data)?;
        }

        Ok(true)
    }

    fn download_and_apply(&self, relative_path: &str) -> anyhow::Result<bool> {
        let data = if let Some(ref webdav_config) = self.config.webdav_config {
            let client = WebdavClient::new(webdav_config.clone());
            let remote_path = format!("data/{relative_path}");
            match client.get(&remote_path)? {
                Some(d) => d,
                None => return Ok(false),
            }
        } else {
            return Ok(false);
        };

        // Decrypt if needed
        let data = if crypto::is_age_encrypted(&data) {
            if let Some(ref passphrase) = self.config.passphrase {
                let armored = String::from_utf8(data)?;
                crypto::decrypt_data(&armored, passphrase)?
            } else {
                warn!("Encrypted file but no passphrase provided: {relative_path}");
                return Ok(false);
            }
        } else {
            data
        };

        let full_path = self.manager.local_dir().join(relative_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full_path, &data)?;

        Ok(true)
    }

    fn upload_manifest(&self) -> anyhow::Result<()> {
        if let Some(ref webdav_config) = self.config.webdav_config {
            let manifest_path = self.manager.state_dir().join("manifest.json");
            if manifest_path.exists() {
                let client = WebdavClient::new(webdav_config.clone());
                client.upload_file(&manifest_path, ".sync/manifest.json")?;
            }
        }
        Ok(())
    }

    /// Export CRDT data to JSON for sync transfer.
    pub fn export_crdt_state(&self) -> String {
        let bookmarks = self.bookmarks.lock();
        let history = self.history.lock();

        let state = serde_json::json!({
            "bookmarks": serde_json::to_value(&*bookmarks).unwrap_or_default(),
            "history": serde_json::to_value(&*history).unwrap_or_default(),
        });

        serde_json::to_string_pretty(&state).unwrap_or_default()
    }

    /// Import CRDT data from JSON and merge.
    pub fn import_and_merge_crdt_state(&self, json: &str) -> anyhow::Result<()> {
        let value: serde_json::Value = serde_json::from_str(json)?;

        if let Some(bm_value) = value.get("bookmarks") {
            let remote_bookmarks: LwwElementSet<BookmarkData> =
                serde_json::from_value(bm_value.clone())?;
            self.bookmarks.lock().merge(&remote_bookmarks);
        }

        if let Some(hist_value) = value.get("history") {
            let remote_history: HistoryLog = serde_json::from_value(hist_value.clone())?;
            self.history.lock().merge(&remote_history);
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
struct SyncMetrics {
    files_uploaded: u64,
    files_downloaded: u64,
    files_deleted: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_display() {
        let status = SyncStatus::Idle;
        assert!(matches!(status, SyncStatus::Idle));

        let status = SyncStatus::Success {
            files_uploaded: 1,
            files_downloaded: 2,
            files_deleted: 0,
        };
        assert!(matches!(status, SyncStatus::Success { .. }));
    }

    #[test]
    fn test_sync_status_error() {
        let status = SyncStatus::Error("test error".into());
        assert!(matches!(status, SyncStatus::Error(_)));
    }

    #[test]
    fn test_sync_loop_new() {
        let dir = tempfile::tempdir().unwrap();
        let config = SyncLoopConfig {
            local_dir: dir.path().to_path_buf(),
            webdav_config: None,
            legacy_target: None,
            passphrase: None,
            sync_interval_secs: 0,
        };
        let sync_loop = SyncLoop::new(config);
        assert!(matches!(sync_loop.status(), SyncStatus::Idle));
    }

    #[test]
    fn test_bookmark_crdt_through_sync_loop() {
        let dir = tempfile::tempdir().unwrap();
        let config = SyncLoopConfig {
            local_dir: dir.path().to_path_buf(),
            webdav_config: None,
            legacy_target: None,
            passphrase: None,
            sync_interval_secs: 0,
        };
        let sync_loop = SyncLoop::new(config);

        let ts = super::super::crdt::HlcTimestamp {
            physical_time: 1000,
            device_id: "dev1".into(),
        };

        sync_loop.bookmarks().lock().upsert(
            "bm1".into(),
            BookmarkData {
                title: "Test".into(),
                url: "https://example.com".into(),
                parent_folder_id: None,
                position: 0,
            },
            ts,
        );

        assert_eq!(sync_loop.bookmarks().lock().active_count(), 1);
    }

    #[test]
    fn test_export_import_crdt_state() {
        let dir = tempfile::tempdir().unwrap();
        let config = SyncLoopConfig {
            local_dir: dir.path().to_path_buf(),
            webdav_config: None,
            legacy_target: None,
            passphrase: None,
            sync_interval_secs: 0,
        };
        let sync_loop = SyncLoop::new(config);

        let ts = super::super::crdt::HlcTimestamp {
            physical_time: 1000,
            device_id: "dev1".into(),
        };
        sync_loop.bookmarks().lock().upsert(
            "bm1".into(),
            BookmarkData {
                title: "Example".into(),
                url: "https://example.com".into(),
                parent_folder_id: None,
                position: 0,
            },
            ts,
        );

        let state = sync_loop.export_crdt_state();
        assert!(state.contains("bookmarks"));

        // Import into fresh loop
        let dir2 = tempfile::tempdir().unwrap();
        let config2 = SyncLoopConfig {
            local_dir: dir2.path().to_path_buf(),
            webdav_config: None,
            legacy_target: None,
            passphrase: None,
            sync_interval_secs: 0,
        };
        let sync_loop2 = SyncLoop::new(config2);
        sync_loop2.import_and_merge_crdt_state(&state).unwrap();

        assert_eq!(sync_loop2.bookmarks().lock().active_count(), 1);
    }

    #[test]
    fn test_sync_loop_config() {
        let config = SyncLoopConfig {
            local_dir: PathBuf::from("/tmp/test"),
            webdav_config: Some(WebdavConfig {
                base_url: "https://dav.example.com/aileron".into(),
                auth: super::super::webdav::WebdavAuth::Basic {
                    username: "user".into(),
                    password: "pass".into(),
                },
                max_retries: 3,
                retry_delay_ms: 1000,
                max_retry_delay_ms: 30000,
            }),
            legacy_target: None,
            passphrase: Some("secret".into()),
            sync_interval_secs: 300,
        };
        assert!(config.webdav_config.is_some());
        assert_eq!(config.sync_interval_secs, 300);
    }
}
