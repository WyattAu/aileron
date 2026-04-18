//! Platform-specific file paths.

use std::path::PathBuf;

/// Get the user configuration directory.
pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".config/aileron")
        })
}

/// Get the user data directory.
pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local/share/aileron")
        })
}

/// Get the cache directory.
pub fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "aileron", "Aileron")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".cache/aileron")
        })
}

/// Get the default downloads directory.
pub fn downloads_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join("Downloads")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join("Downloads")
    }
    #[cfg(target_os = "windows")]
    {
        let user_profile =
            std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\Default".into());
        PathBuf::from(user_profile).join("Downloads")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        PathBuf::from("/tmp")
    }
}

/// Get the operating system name.
pub fn os_name() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "Linux"
    }
    #[cfg(target_os = "macos")]
    {
        "macOS"
    }
    #[cfg(target_os = "windows")]
    {
        "Windows"
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "Unknown"
    }
}

/// Get the current desktop environment (Linux only).
pub fn desktop_environment() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_CURRENT_DESKTOP")
            .ok()
            .or_else(|| std::env::var("DESKTOP_SESSION").ok())
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// Check if running under Wayland.
pub fn is_wayland() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE")
                .map(|s| s == "wayland")
                .unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Check if running under X11.
pub fn is_x11() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var("DISPLAY").is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Get the default web browser command.
pub fn default_browser_cmd() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        vec!["xdg-open".into()]
    }
    #[cfg(target_os = "macos")]
    {
        vec!["open".into()]
    }
    #[cfg(target_os = "windows")]
    {
        vec!["cmd".into(), "/c".into(), "start".into()]
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        vec!["xdg-open".into()]
    }
}

/// Get the default terminal emulator command.
pub fn default_terminal_cmd() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        vec!["sh".into(), "-c".into(), "$TERM".into()]
    }
    #[cfg(target_os = "macos")]
    {
        vec!["open".into(), "-a".into(), "Terminal".into()]
    }
    #[cfg(target_os = "windows")]
    {
        vec!["cmd".into(), "/c".into(), "start".into(), "cmd".into()]
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        vec!["sh".into()]
    }
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
