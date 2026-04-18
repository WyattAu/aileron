//! Platform abstraction layer.
//!
//! Provides a unified trait-based interface for platform-specific operations:
//! - File paths and directories
//! - Default browser/terminal detection
//! - Native dialog support
//! - OS-specific key handling
//! - Notifications

pub mod config;
pub mod paths;
pub mod traits;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

pub use config::*;
pub use paths::*;
pub use traits::PlatformOps;

/// Get the platform-specific implementation.
pub fn platform() -> &'static dyn PlatformOps {
    #[cfg(target_os = "linux")]
    {
        static INSTANCE: linux::LinuxPlatform = linux::LinuxPlatform;
        &INSTANCE
    }
    #[cfg(target_os = "macos")]
    {
        static INSTANCE: macos::MacOSPlatform = macos::MacOSPlatform;
        &INSTANCE
    }
    #[cfg(target_os = "windows")]
    {
        static INSTANCE: windows::WindowsPlatform = windows::WindowsPlatform;
        &INSTANCE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_returns_non_null() {
        let p = platform();
        let _ = p.os_name();
    }

    #[test]
    fn test_platform_os_name_known() {
        assert_ne!(platform().os_name(), "Unknown");
    }

    #[test]
    fn test_platform_downloads_dir_non_empty() {
        let dir = platform().downloads_dir();
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_platform_wry_backend_known() {
        let backend = platform().wry_backend();
        assert_ne!(backend, "unknown");
    }

    #[test]
    fn test_platform_super_key_name_non_empty() {
        assert!(!platform().super_key_name().is_empty());
    }

    #[test]
    fn test_platform_default_browser_cmd_non_empty() {
        assert!(!platform().default_browser_cmd().is_empty());
    }

    #[test]
    fn test_platform_default_terminal_cmd_non_empty() {
        assert!(!platform().default_terminal_cmd().is_empty());
    }

    #[test]
    fn test_platform_is_wayland_returns_bool() {
        let _ = platform().is_wayland();
    }

    #[test]
    fn test_platform_is_x11_returns_bool() {
        let _ = platform().is_x11();
    }

    #[test]
    fn test_platform_desktop_environment_returns_option() {
        let _ = platform().desktop_environment();
    }

    #[test]
    fn test_platform_show_notification_no_panic() {
        platform().show_notification("test", "body");
    }

    #[test]
    fn test_platform_file_open_dialog_no_panic() {
        let _ = platform().file_open_dialog("Open", &[]);
    }

    #[test]
    fn test_platform_config_overrides_returns_vec() {
        let _ = platform().config_overrides();
    }
}
