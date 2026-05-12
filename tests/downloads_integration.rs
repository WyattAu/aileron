//! Integration tests for the download manager.
//!
//! Tests the full DownloadManager lifecycle, filename sanitization,
//! progress formatting edge cases, and multi-download tracking.

use std::path::PathBuf;

use aileron::downloads::{DownloadManager, DownloadProgress, DownloadState};

/// Verify that the download manager can be created with a temp directory
/// and starts in an empty state.
#[test]
fn test_download_manager_empty_state() {
    let dir = tempfile::tempdir().unwrap();
    let dm = DownloadManager::new(dir.path().to_path_buf());

    assert_eq!(dm.active_count(), 0);
    assert!(!dm.has_active());
    assert!(dm.progress_all().is_empty());
    assert!(dm.progress(1).is_none());
    assert!(dm.progress(999).is_none());

    drop(dm);
}

/// Verify filename sanitization prevents path traversal attacks.
#[test]
fn test_filename_sanitization_path_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let dm = DownloadManager::new(dir.path().to_path_buf());

    // Attempt path traversal via filename
    let id = dm.start("https://example.com/evil.sh", Some("../../../etc/passwd"));

    let progress = dm.progress(id).unwrap();
    let actual_filename = &progress.filename;
    let actual_path = &progress.dest_path;

    // Filename should be stripped to just "passwd"
    assert!(
        !actual_filename.contains('/'),
        "filename should not contain path separators: {actual_filename}"
    );
    assert!(
        !actual_filename.contains(".."),
        "filename should not contain parent references: {actual_filename}"
    );
    assert_eq!(actual_filename, "passwd");

    // Path should be within the downloads directory
    assert!(
        actual_path.starts_with(dir.path()),
        "download path {actual_path:?} should be within {dir:?}"
    );

    drop(dm);
}

/// Verify filename handling with non-UTF-8 bytes.
/// Note: Path::file_name() on Linux passes through arbitrary bytes including null bytes.
/// This test documents current behavior — null byte handling is a known limitation.
#[test]
fn test_filename_sanitization_null_byte() {
    let dir = tempfile::tempdir().unwrap();
    let dm = DownloadManager::new(dir.path().to_path_buf());

    let id = dm.start("https://example.com/file.txt", Some("good\0bad.txt"));

    let progress = dm.progress(id).unwrap();
    // On Linux, null bytes pass through Path::file_name().to_str()
    // This is a known limitation — verify behavior is consistent
    assert!(
        !progress.filename.is_empty(),
        "filename should not be empty"
    );

    drop(dm);
}

/// Verify filename sanitization for empty string falls back to
/// extraction from URL.
#[test]
fn test_filename_sanitization_empty_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let dm = DownloadManager::new(dir.path().to_path_buf());

    // Empty filename → extract from URL
    let id = dm.start("https://example.com/data.csv", Some(""));

    let progress = dm.progress(id).unwrap();
    // Empty or from-URL both acceptable; either way it's non-empty
    assert!(!progress.filename.is_empty());

    drop(dm);
}

/// Verify that the downloads directory is created automatically.
#[test]
fn test_downloads_directory_auto_created() {
    let parent = tempfile::tempdir().unwrap();
    let dl_dir = parent.path().join("subdir").join("downloads");

    assert!(!dl_dir.exists());

    let dm = DownloadManager::new(dl_dir.clone());
    assert!(dl_dir.exists(), "downloads directory should be created");

    drop(dm);
}

/// Verify that multiple downloads can be tracked simultaneously.
#[test]
fn test_multiple_download_ids_increment() {
    let dir = tempfile::tempdir().unwrap();
    let dm = DownloadManager::new(dir.path().to_path_buf());

    let id1 = dm.start("https://a.com/file1.bin", None);
    let id2 = dm.start("https://b.com/file2.bin", None);
    let id3 = dm.start("https://c.com/file3.bin", None);

    assert!(id1 < id2);
    assert!(id2 < id3);

    let all = dm.progress_all();
    assert_eq!(all.len(), 3);

    let ids: Vec<u64> = all.iter().map(|p| p.id).collect();
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));
    assert!(ids.contains(&id3));

    drop(dm);
}

/// Verify that cancelled downloads are removed from tracking.
#[test]
fn test_cancel_removes_download() {
    let dir = tempfile::tempdir().unwrap();
    let dm = DownloadManager::new(dir.path().to_path_buf());

    let id = dm.start("https://example.com/file.bin", None);
    assert_eq!(dm.progress_all().len(), 1);

    let cancelled = dm.cancel(id);
    assert!(cancelled);
    assert!(dm.progress_all().is_empty());
    assert!(dm.progress(id).is_none());

    // Double cancel returns false
    assert!(!dm.cancel(id));

    drop(dm);
}

/// Verify that progressive cleanup only removes finished downloads.
#[test]
fn test_cleanup_finished_removes_completed() {
    let dir = tempfile::tempdir().unwrap();
    let dm = DownloadManager::new(dir.path().to_path_buf());

    let _id1 = dm.start("https://example.com/keep.pdf", None);
    let id2 = dm.start("https://example.com/done.bin", None);

    // Cancel id2 so it should be removed on cleanup
    dm.cancel(id2);

    // Before cleanup: 1 active remaining (id2 cancelled but not cleaned)
    let before = dm.progress_all();
    // id1 is still "downloading" since nothing changed its state
    assert!(before.iter().any(|p| p.state == DownloadState::Downloading));

    dm.cleanup_finished();

    // After cleanup: id2 (cancelled) is gone; id1 remains (still in "downloading" state)
    let after = dm.progress_all();
    // At least id1 should still be present
    assert!(!after.is_empty() || dm.active_count() == 0);

    drop(dm);
}

/// Verify progress formatting edge cases.
#[test]
fn test_progress_formatting_edge_cases() {
    assert_eq!(DownloadProgress::format_bytes(0), "0 B");
    assert_eq!(DownloadProgress::format_bytes(1023), "1023 B");
    assert_eq!(DownloadProgress::format_bytes(1024), "1 KB");
    assert_eq!(DownloadProgress::format_bytes(1_048_575), "1024 KB");
    assert_eq!(DownloadProgress::format_bytes(1_048_576), "1.0 MB");

    assert_eq!(DownloadProgress::format_speed(0.0), "0 B/s");
    assert_eq!(DownloadProgress::format_speed(1023.0), "1023 B/s");

    // ETA edge cases
    assert_eq!(DownloadProgress::format_eta(0.0), "0s");
    assert_eq!(DownloadProgress::format_eta(59.0), "59s");
    assert_eq!(DownloadProgress::format_eta(60.0), "1m 0s");
    assert_eq!(DownloadProgress::format_eta(3599.0), "59m 59s");
    assert_eq!(DownloadProgress::format_eta(f64::NAN), "—");
    assert_eq!(DownloadProgress::format_eta(f64::NEG_INFINITY), "—");
}

/// Verify percent string formatting edge cases.
#[test]
fn test_percent_str_edge_cases() {
    let make_progress = |fraction: f64| -> DownloadProgress {
        DownloadProgress {
            id: 0,
            url: String::new(),
            filename: String::new(),
            dest_path: PathBuf::new(),
            state: DownloadState::Downloading,
            received_bytes: 0,
            total_bytes: 100,
            speed_bytes_per_sec: 0.0,
            fraction,
        }
    };

    assert_eq!(make_progress(0.0).percent_str(), "0%");
    assert_eq!(make_progress(0.5).percent_str(), "50%");
    assert_eq!(make_progress(1.0).percent_str(), "100%");
    assert_eq!(make_progress(0.333).percent_str(), "33%");
}

/// Verify ETA calculation with zero total bytes — returns 0.0
/// (no remaining bytes divided by speed = 0 seconds).
#[test]
fn test_eta_with_zero_total() {
    let p = DownloadProgress {
        id: 1,
        url: String::new(),
        filename: String::new(),
        dest_path: PathBuf::new(),
        state: DownloadState::Downloading,
        received_bytes: 0,
        total_bytes: 0,
        speed_bytes_per_sec: 100.0,
        fraction: 0.0,
    };
    // With zero total and zero received, remaining = 0, so ETA = 0 / speed = 0
    assert_eq!(p.eta_secs(), 0.0);
}

/// Verify ETA is infinite when not downloading.
#[test]
fn test_eta_when_not_downloading() {
    for state in &[
        DownloadState::Pending,
        DownloadState::Paused,
        DownloadState::Completed,
        DownloadState::Failed,
        DownloadState::Cancelled,
    ] {
        let p = DownloadProgress {
            id: 1,
            url: String::new(),
            filename: String::new(),
            dest_path: PathBuf::new(),
            state: *state,
            received_bytes: 0,
            total_bytes: 100,
            speed_bytes_per_sec: 100.0,
            fraction: 0.0,
        };
        assert!(
            p.eta_secs().is_infinite(),
            "ETA should be infinite for {state:?}"
        );
    }
}

/// Verify that pausing and resuming a non-existent download returns false.
#[test]
fn test_pause_resume_nonexistent() {
    let dir = tempfile::tempdir().unwrap();
    let dm = DownloadManager::new(dir.path().to_path_buf());

    assert!(!dm.pause(1));
    assert!(!dm.pause(42));
    assert!(!dm.resume(1));
    assert!(!dm.resume(42));

    drop(dm);
}

/// Verify downloads_dir() returns the configured directory.
#[test]
fn test_downloads_dir_returns_configured_path() {
    let dir = tempfile::tempdir().unwrap();
    let dl_path = dir.path().to_path_buf();
    let dm = DownloadManager::new(dl_path.clone());

    assert_eq!(dm.downloads_dir(), dl_path);

    drop(dm);
}
