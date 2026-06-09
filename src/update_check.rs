//! Auto-update check functionality.
//!
//! D1-04: Check GitHub releases API for latest version and show notification.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// A GitHub release entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: String,
    pub body: Option<String>,
    pub published_at: String,
    pub html_url: String,
}

/// Update checker state.
pub struct UpdateChecker {
    /// Last time we checked for updates.
    last_check: Option<Instant>,
    /// Cached latest version string.
    latest_version: Option<String>,
    /// Cached changelog.
    changelog: Option<String>,
    /// Whether an update is available.
    update_available: bool,
    /// The current version.
    current_version: String,
}

impl UpdateChecker {
    pub fn new() -> Self {
        Self {
            last_check: None,
            latest_version: None,
            changelog: None,
            update_available: false,
            current_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Check for updates (non-blocking, spawns a background thread).
    pub fn check_for_updates(&mut self) {
        let now = Instant::now();

        // Don't check more than once per hour
        if let Some(last) = self.last_check
            && now.duration_since(last) < Duration::from_secs(3600)
        {
            return;
        }

        self.last_check = Some(now);
        let current_version = self.current_version.clone();

        // Spawn a background thread to check for updates
        std::thread::spawn(move || {
            match Self::fetch_latest_release() {
                Ok(release) => {
                    let latest = release.tag_name.trim_start_matches('v').to_string();
                    let current = current_version.trim_start_matches('v');
                    let update_available = Self::version_compare(&latest, current);
                    info!(
                        "Update check: current={}, latest={}, update_available={}",
                        current, latest, update_available
                    );
                    // Note: In a real implementation, we'd send this back to the main thread
                    // via a channel. For now, we just log it.
                }
                Err(e) => {
                    warn!("Failed to check for updates: {}", e);
                }
            }
        });
    }

    /// Fetch the latest release from GitHub.
    fn fetch_latest_release() -> Result<GitHubRelease, Box<dyn std::error::Error>> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(format!("aileron/{}", env!("CARGO_PKG_VERSION")))
            .build()?;

        let response = client
            .get("https://api.github.com/repos/WyattAu/aileron/releases/latest")
            .header("Accept", "application/vnd.github.v3+json")
            .send()?;

        if !response.status().is_success() {
            return Err(format!("GitHub API error: {}", response.status()).into());
        }

        let text = response.text()?;
        let release: GitHubRelease = serde_json::from_str(&text)?;
        Ok(release)
    }

    /// Compare version strings (simple semver-like comparison).
    /// Returns true if `latest` is newer than `current`.
    fn version_compare(latest: &str, current: &str) -> bool {
        let parse_version =
            |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse::<u32>().ok()).collect() };

        let latest_parts = parse_version(latest);
        let current_parts = parse_version(current);

        // Compare part by part
        for i in 0..latest_parts.len().max(current_parts.len()) {
            let l = latest_parts.get(i).copied().unwrap_or(0);
            let c = current_parts.get(i).copied().unwrap_or(0);
            if l > c {
                return true;
            }
            if l < c {
                return false;
            }
        }
        false
    }

    /// Get the current version.
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// Get the latest version (if checked).
    pub fn latest_version(&self) -> Option<&str> {
        self.latest_version.as_deref()
    }

    /// Check if an update is available.
    pub fn is_update_available(&self) -> bool {
        self.update_available
    }

    /// Get the changelog (if available).
    pub fn changelog(&self) -> Option<&str> {
        self.changelog.as_deref()
    }

    /// Get a status message about the update check.
    pub fn status_message(&self) -> String {
        if let Some(ref latest) = self.latest_version {
            if self.update_available {
                format!("Update available: {} -> {}", self.current_version, latest)
            } else {
                format!("Already on latest version: {}", self.current_version)
            }
        } else {
            "Update check not yet performed".to_string()
        }
    }
}

impl Default for UpdateChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_checker_new() {
        let checker = UpdateChecker::new();
        assert!(!checker.is_update_available());
        assert_eq!(checker.current_version(), env!("CARGO_PKG_VERSION"));
        assert!(checker.latest_version().is_none());
    }

    #[test]
    fn test_version_compare() {
        assert!(UpdateChecker::version_compare("0.25.0", "0.24.0"));
        assert!(UpdateChecker::version_compare("1.0.0", "0.9.9"));
        assert!(UpdateChecker::version_compare("0.25.1", "0.25.0"));
        assert!(!UpdateChecker::version_compare("0.24.0", "0.25.0"));
        assert!(!UpdateChecker::version_compare("0.25.0", "0.25.0"));
        assert!(!UpdateChecker::version_compare("0.24.9", "0.25.0"));
    }

    #[test]
    fn test_status_message_no_check() {
        let checker = UpdateChecker::new();
        assert_eq!(checker.status_message(), "Update check not yet performed");
    }

    #[test]
    fn test_status_message_up_to_date() {
        let mut checker = UpdateChecker::new();
        checker.latest_version = Some("0.24.0".to_string());
        checker.update_available = false;
        assert!(checker.status_message().contains("Already on latest"));
    }

    #[test]
    fn test_status_message_update_available() {
        let mut checker = UpdateChecker::new();
        checker.latest_version = Some("0.26.0".to_string());
        checker.update_available = true;
        assert!(checker.status_message().contains("Update available"));
    }
}
