//! Download manager with progress tracking, pause/resume, and concurrent downloads.
//!
//! Uses `reqwest` streaming with HTTP Range headers for reliable downloads
//! with resume support. Integrates with `crate::db::downloads` for persistence.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// State of an individual download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadState {
    /// Waiting to start.
    Pending,
    /// Actively downloading.
    Downloading,
    /// Paused by user.
    Paused,
    /// Download completed successfully.
    Completed,
    /// Download failed.
    Failed,
    /// Download cancelled by user.
    Cancelled,
}

impl std::fmt::Display for DownloadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Downloading => write!(f, "downloading"),
            Self::Paused => write!(f, "paused"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl From<&str> for DownloadState {
    fn from(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "downloading" => Self::Downloading,
            "paused" => Self::Paused,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Pending,
        }
    }
}

/// Progress information for a single download.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub id: u64,
    pub url: String,
    pub filename: String,
    pub dest_path: PathBuf,
    pub state: DownloadState,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_per_sec: f64,
    /// 0.0–1.0 fraction.
    pub fraction: f64,
}

impl DownloadProgress {
    /// Format bytes as human-readable string.
    pub fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * KB;
        const GB: u64 = 1024 * MB;
        if bytes >= GB {
            format!("{:.1} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.1} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.0} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    /// Format speed as human-readable string.
    pub fn format_speed(bytes_per_sec: f64) -> String {
        if bytes_per_sec < 1024.0 {
            format!("{:.0} B/s", bytes_per_sec)
        } else if bytes_per_sec < 1024.0 * 1024.0 {
            format!("{:.0} KB/s", bytes_per_sec / 1024.0)
        } else {
            format!("{:.1} MB/s", bytes_per_sec / (1024.0 * 1024.0))
        }
    }

    /// Estimated time remaining as human-readable string.
    pub fn format_eta(seconds: f64) -> String {
        if seconds.is_infinite() || seconds.is_nan() || seconds < 0.0 {
            return "—".into();
        }
        let secs = seconds as u64;
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        }
    }

    /// Progress percentage as string.
    pub fn percent_str(&self) -> String {
        format!("{:.0}%", self.fraction * 100.0)
    }

    /// ETA in seconds.
    pub fn eta_secs(&self) -> f64 {
        if self.speed_bytes_per_sec <= 0.0 || self.state != DownloadState::Downloading {
            return f64::INFINITY;
        }
        let remaining = self.total_bytes.saturating_sub(self.received_bytes) as f64;
        remaining / self.speed_bytes_per_sec
    }
}

/// Internal tracking for a download task.
struct DownloadTask {
    url: String,
    filename: String,
    dest_path: PathBuf,
    state: AtomicBool, // true = running, false = paused/cancelled
    received: AtomicU64,
    total: AtomicU64,
    last_received: AtomicU64,
    last_time: AtomicU64, // milliseconds since epoch
    speed: AtomicU64,     // bytes/sec (smoothed)
}

/// The main download manager.
///
/// Thread-safe. Downloads run on a background tokio runtime.
/// Progress is polled from the main thread.
pub struct DownloadManager {
    downloads: RwLock<HashMap<u64, Arc<DownloadTask>>>,
    next_id: AtomicU64,
    runtime: tokio::runtime::Runtime,
    downloads_dir: PathBuf,
}

impl DownloadManager {
    /// Create a new download manager.
    ///
    /// `downloads_dir` is the default save location.
    pub fn new(downloads_dir: PathBuf) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("Failed to create download manager runtime");

        if !downloads_dir.exists() {
            let _ = std::fs::create_dir_all(&downloads_dir);
        }

        Self {
            downloads: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            runtime,
            downloads_dir,
        }
    }

    /// Get the default downloads directory.
    pub fn downloads_dir(&self) -> &Path {
        &self.downloads_dir
    }

    /// Start a new download. Returns the download ID.
    pub fn start(&self, url: &str, filename: Option<&str>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let raw_filename = filename
            .map(|s| s.to_string())
            .unwrap_or_else(|| Self::extract_filename(url));

        // Sanitize filename to prevent path traversal: strip path components
        let filename = std::path::Path::new(&raw_filename)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("download")
            .to_string();

        let dest_path = self.downloads_dir.join(&filename);

        let task = Arc::new(DownloadTask {
            url: url.to_string(),
            filename: filename.clone(),
            dest_path: dest_path.clone(),
            state: AtomicBool::new(true),
            received: AtomicU64::new(0),
            total: AtomicU64::new(0),
            last_received: AtomicU64::new(0),
            last_time: AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
            ),
            speed: AtomicU64::new(0),
        });

        {
            let mut downloads = self.downloads.write();
            downloads.insert(id, task.clone());
        }

        let url_owned = url.to_string();
        let dest = dest_path.clone();

        self.runtime.spawn(async move {
            Self::download_file(&task, &url_owned, &dest).await;
        });

        info!("Download {} started: {} -> {:?}", id, url, dest_path);
        id
    }

    /// Pause a download by ID.
    pub fn pause(&self, id: u64) -> bool {
        let downloads = self.downloads.read();
        if let Some(task) = downloads.get(&id) {
            task.state.store(false, Ordering::Relaxed);
            info!("Download {} paused", id);
            true
        } else {
            false
        }
    }

    /// Resume a download by ID (restarts from where it left off if server supports Range).
    pub fn resume(&self, id: u64) -> bool {
        let (task, url, dest) = {
            let downloads = self.downloads.read();
            if let Some(task) = downloads.get(&id) {
                task.state.store(true, Ordering::Relaxed);
                (task.clone(), task.url.clone(), task.dest_path.clone())
            } else {
                return false;
            }
        };

        info!("Download {} resumed", id);

        self.runtime.spawn(async move {
            Self::download_file(&task, &url, &dest).await;
        });

        true
    }

    /// Cancel a download by ID. Removes it from tracking.
    pub fn cancel(&self, id: u64) -> bool {
        let task = {
            let mut downloads = self.downloads.write();
            downloads.remove(&id)
        };
        if let Some(task) = task {
            task.state.store(false, Ordering::Relaxed);
            info!("Download {} cancelled", id);
            true
        } else {
            false
        }
    }

    /// Get progress for all tracked downloads.
    pub fn progress_all(&self) -> Vec<DownloadProgress> {
        let downloads = self.downloads.read();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        downloads
            .iter()
            .map(|(&id, task)| {
                let running = task.state.load(Ordering::Relaxed);
                let received = task.received.load(Ordering::Relaxed);
                let total = task.total.load(Ordering::Relaxed);

                // Update speed calculation (smoothed)
                let last_received = task.last_received.load(Ordering::Relaxed);
                let last_time = task.last_time.load(Ordering::Relaxed);
                let elapsed_ms = now_ms.saturating_sub(last_time);
                let speed = if elapsed_ms > 100 {
                    let elapsed_sec = elapsed_ms as f64 / 1000.0;
                    let delta = received.saturating_sub(last_received) as f64 / elapsed_sec;
                    // Exponential moving average
                    let prev_speed = task.speed.load(Ordering::Relaxed) as f64;
                    let new_speed = prev_speed * 0.7 + delta * 0.3;
                    task.speed.store(new_speed as u64, Ordering::Relaxed);
                    task.last_received.store(received, Ordering::Relaxed);
                    task.last_time.store(now_ms, Ordering::Relaxed);
                    new_speed
                } else {
                    task.speed.load(Ordering::Relaxed) as f64
                };

                let fraction = if total > 0 {
                    received as f64 / total as f64
                } else {
                    0.0
                };

                let state = if received > 0 && received >= total && total > 0 {
                    DownloadState::Completed
                } else if running {
                    DownloadState::Downloading
                } else if received > 0 {
                    DownloadState::Paused
                } else {
                    DownloadState::Pending
                };

                DownloadProgress {
                    id,
                    url: task.url.clone(),
                    filename: task.filename.clone(),
                    dest_path: task.dest_path.clone(),
                    state,
                    received_bytes: received,
                    total_bytes: total,
                    speed_bytes_per_sec: speed,
                    fraction,
                }
            })
            .collect()
    }

    /// Get progress for a single download.
    pub fn progress(&self, id: u64) -> Option<DownloadProgress> {
        let downloads = self.downloads.read();
        downloads.get(&id)?;
        drop(downloads);
        self.progress_all().into_iter().find(|p| p.id == id)
    }

    /// Count of active (downloading) downloads.
    pub fn active_count(&self) -> usize {
        let downloads = self.downloads.read();
        downloads
            .values()
            .filter(|t| {
                t.state.load(Ordering::Relaxed)
                    && t.received.load(Ordering::Relaxed) < t.total.load(Ordering::Relaxed)
            })
            .count()
    }

    /// Check if any download is active.
    pub fn has_active(&self) -> bool {
        self.active_count() > 0
    }

    /// Remove completed/failed downloads from tracking.
    pub fn cleanup_finished(&self) {
        let mut downloads = self.downloads.write();
        downloads.retain(|_, task| {
            let received = task.received.load(Ordering::Relaxed);
            let total = task.total.load(Ordering::Relaxed);
            let running = task.state.load(Ordering::Relaxed);
            // Keep if still downloading or paused
            running || (received > 0 && received < total)
        });
    }

    /// Extract a filename from a URL.
    fn extract_filename(url: &str) -> String {
        url::Url::parse(url)
            .ok()
            .and_then(|u| {
                u.path()
                    .rsplit('/')
                    .next()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "download".to_string())
    }

    /// Perform the actual download using reqwest streaming.
    async fn download_file(task: &DownloadTask, url: &str, dest: &Path) {
        // Check if file exists for resume
        let existing_size = std::fs::metadata(dest).ok().map(|m| m.len()).unwrap_or(0);

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent("Aileron/0.12.0")
            .build();

        let client = match client {
            Ok(c) => c,
            Err(e) => {
                warn!("Download client build failed for {}: {}", url, e);
                return;
            }
        };

        let mut request = client.get(url);
        if existing_size > 0 {
            // Request resume from where we left off
            request = request.header("Range", format!("bytes={}-", existing_size));
        }

        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("Download request failed for {}: {}", url, e);
                return;
            }
        };

        let status = response.status();
        if status.is_client_error() || status.is_server_error() {
            warn!("Download HTTP error {} for {}", status, url);
            return;
        }

        let total = response.content_length().unwrap_or(0);
        // If server returned 200 (not 206 Partial Content), start from scratch
        let is_resume = status.as_u16() == 206;
        let start_byte = if is_resume { existing_size } else { 0 };

        task.total.store(total, Ordering::Relaxed);

        // Create parent directory
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Open file for append (resume) or create (fresh)
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(is_resume)
            .truncate(!is_resume)
            .open(dest);

        let file = match file {
            Ok(f) => f,
            Err(e) => {
                warn!("Download file create failed for {}: {}", dest.display(), e);
                return;
            }
        };

        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let mut file = tokio::fs::File::from_std(file);

        let mut stream = response.bytes_stream();
        let mut received = start_byte;
        task.received.store(received, Ordering::Relaxed);

        while let Some(chunk_result) = stream.next().await {
            // Check if paused/cancelled
            if !task.state.load(Ordering::Relaxed) {
                info!("Download paused/cancelled: {} ({} bytes)", url, received);
                return;
            }

            match chunk_result {
                Ok(chunk) => {
                    if let Err(e) = file.write_all(&chunk).await {
                        warn!("Download write failed: {}", e);
                        return;
                    }
                    received += chunk.len() as u64;
                    task.received.store(received, Ordering::Relaxed);
                }
                Err(e) => {
                    warn!("Download stream error for {}: {}", url, e);
                    return;
                }
            }
        }

        if let Err(e) = file.flush().await {
            warn!("Download flush failed: {}", e);
        }

        // Mark as not running (completed)
        task.state.store(false, Ordering::Relaxed);
        info!("Download completed: {} ({} bytes)", url, received);
    }
}

impl Drop for DownloadManager {
    fn drop(&mut self) {
        // Cancel all active downloads
        for task in self.downloads.read().values() {
            task.state.store(false, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_filename_from_url() {
        assert_eq!(
            DownloadManager::extract_filename("https://example.com/file.pdf"),
            "file.pdf"
        );
        assert_eq!(
            DownloadManager::extract_filename("https://example.com/path/to/archive.tar.gz"),
            "archive.tar.gz"
        );
        assert_eq!(
            DownloadManager::extract_filename("https://example.com/"),
            "download"
        );
        assert_eq!(DownloadManager::extract_filename("not-a-url"), "download");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(DownloadProgress::format_bytes(500), "500 B");
        assert_eq!(DownloadProgress::format_bytes(1536), "2 KB");
        assert_eq!(DownloadProgress::format_bytes(1_048_576), "1.0 MB");
        assert_eq!(DownloadProgress::format_bytes(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(DownloadProgress::format_speed(500.0), "500 B/s");
        assert_eq!(DownloadProgress::format_speed(15_360.0), "15 KB/s");
        assert_eq!(DownloadProgress::format_speed(1_572_864.0), "1.5 MB/s");
    }

    #[test]
    fn test_format_eta() {
        assert_eq!(DownloadProgress::format_eta(30.0), "30s");
        assert_eq!(DownloadProgress::format_eta(90.0), "1m 30s");
        assert_eq!(DownloadProgress::format_eta(3700.0), "1h 1m");
        assert_eq!(DownloadProgress::format_eta(f64::INFINITY), "—");
        assert_eq!(DownloadProgress::format_eta(-1.0), "—");
    }

    #[test]
    fn test_download_state_display() {
        assert_eq!(DownloadState::Downloading.to_string(), "downloading");
        assert_eq!(DownloadState::Completed.to_string(), "completed");
        assert_eq!(DownloadState::Paused.to_string(), "paused");
    }

    #[test]
    fn test_download_state_from_str() {
        assert_eq!(
            DownloadState::from("downloading"),
            DownloadState::Downloading
        );
        assert_eq!(DownloadState::from("completed"), DownloadState::Completed);
        assert_eq!(DownloadState::from("unknown"), DownloadState::Pending);
    }

    #[test]
    fn test_eta_calculation() {
        let p = DownloadProgress {
            id: 1,
            url: "http://test".into(),
            filename: "test".into(),
            dest_path: PathBuf::new(),
            state: DownloadState::Downloading,
            received_bytes: 5_000_000,
            total_bytes: 10_000_000,
            speed_bytes_per_sec: 1_000_000.0,
            fraction: 0.5,
        };
        assert_eq!(p.eta_secs(), 5.0);
        assert_eq!(p.percent_str(), "50%");
    }

    #[test]
    fn test_eta_no_speed() {
        let p = DownloadProgress {
            id: 1,
            url: "http://test".into(),
            filename: "test".into(),
            dest_path: PathBuf::new(),
            state: DownloadState::Downloading,
            received_bytes: 5_000_000,
            total_bytes: 10_000_000,
            speed_bytes_per_sec: 0.0,
            fraction: 0.5,
        };
        assert!(p.eta_secs().is_infinite());
    }

    #[test]
    fn test_download_manager_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let dm = DownloadManager::new(dir.path().to_path_buf());

        // No downloads initially
        assert_eq!(dm.active_count(), 0);
        assert!(!dm.has_active());
        assert!(dm.progress_all().is_empty());

        // Manually drop before the tempdir
        drop(dm);
    }

    #[test]
    fn test_download_manager_pause_cancel_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let dm = DownloadManager::new(dir.path().to_path_buf());

        // Should not panic on nonexistent IDs
        assert!(!dm.pause(999));
        assert!(!dm.resume(999));
        assert!(!dm.cancel(999));
    }
}
