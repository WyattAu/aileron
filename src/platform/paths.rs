//! Platform-specific file paths.
//!
//! The directory resolution functions (config_dir, data_dir, cache_dir) use the
//! `directories` crate and work cross-platform without conditional compilation.
//!
//! The per-OS functions (downloads_dir, os_name, etc.) delegate to the
//! [`PlatformOps`](super::PlatformOps) trait via [`super::platform()`].
//! Prefer calling `crate::platform::platform()` directly for new code.

use std::path::PathBuf;

fn home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(r"C:\Users\Default"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

fn fallback_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        home_dir().join("AppData").join("Roaming").join("Aileron")
    }
    #[cfg(target_os = "macos")]
    {
        home_dir()
            .join("Library")
            .join("Application Support")
            .join("Aileron")
    }
    #[cfg(target_os = "linux")]
    {
        home_dir().join(".config").join("aileron")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        home_dir().join(".config").join("aileron")
    }
}

fn fallback_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        home_dir().join("AppData").join("Local").join("Aileron")
    }
    #[cfg(target_os = "macos")]
    {
        home_dir()
            .join("Library")
            .join("Application Support")
            .join("Aileron")
    }
    #[cfg(target_os = "linux")]
    {
        home_dir().join(".local").join("share").join("aileron")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        home_dir().join(".local").join("share").join("aileron")
    }
}

fn fallback_cache_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        home_dir()
            .join("AppData")
            .join("Local")
            .join("Aileron")
            .join("Cache")
    }
    #[cfg(target_os = "macos")]
    {
        home_dir().join("Library").join("Caches").join("Aileron")
    }
    #[cfg(target_os = "linux")]
    {
        home_dir().join(".cache").join("aileron")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        home_dir().join(".cache").join("aileron")
    }
}

pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(fallback_config_dir)
}

pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(fallback_data_dir)
}

pub fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(fallback_cache_dir)
}

pub fn downloads_dir() -> PathBuf {
    super::platform().downloads_dir()
}

pub fn os_name() -> &'static str {
    super::platform().os_name()
}

pub fn desktop_environment() -> Option<String> {
    super::platform().desktop_environment()
}

pub fn is_wayland() -> bool {
    super::platform().is_wayland()
}

pub fn is_x11() -> bool {
    super::platform().is_x11()
}

pub fn default_browser_cmd() -> Vec<String> {
    super::platform().default_browser_cmd()
}

pub fn default_terminal_cmd() -> Vec<String> {
    super::platform().default_terminal_cmd()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_dir_returns_path() {
        let dir = config_dir();
        assert!(!dir.as_os_str().is_empty());
        assert!(dir.to_string_lossy().contains("aileron"));
    }

    #[test]
    fn test_data_dir_returns_path() {
        let dir = data_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_cache_dir_returns_path() {
        let dir = cache_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_downloads_dir_returns_path() {
        let dir = downloads_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_os_name_is_known() {
        let name = os_name();
        assert_ne!(name, "Unknown");
    }

    #[test]
    fn test_default_browser_cmd_non_empty() {
        let cmd = default_browser_cmd();
        assert!(!cmd.is_empty());
    }

    #[test]
    fn test_default_terminal_cmd_non_empty() {
        let cmd = default_terminal_cmd();
        assert!(!cmd.is_empty());
    }

    #[test]
    fn test_is_wayland_returns_bool() {
        let _ = is_wayland();
    }

    #[test]
    fn test_is_x11_returns_bool() {
        let _ = is_x11();
    }

    #[test]
    fn test_desktop_environment_returns_option() {
        let _ = desktop_environment();
    }
}
