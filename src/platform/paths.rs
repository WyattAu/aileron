//! Platform-specific file paths.
//!
//! The directory resolution functions (config_dir, data_dir, cache_dir) use the
//! `directories` crate and work cross-platform without conditional compilation.
//!
//! The per-OS functions (downloads_dir, os_name, etc.) delegate to the
//! [`PlatformOps`](super::PlatformOps) trait via [`super::platform()`].
//! Prefer calling `crate::platform::platform()` directly for new code.

use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config/aileron")
        })
}

pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local/share/aileron")
        })
}

pub fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".cache/aileron")
        })
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
