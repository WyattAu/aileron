use std::path::PathBuf;

use super::traits::PlatformOps;

pub struct WindowsPlatform;

impl PlatformOps for WindowsPlatform {
    fn downloads_dir(&self) -> PathBuf {
        let user_profile =
            std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\Default".into());
        PathBuf::from(user_profile).join("Downloads")
    }

    fn os_name(&self) -> &'static str {
        "Windows"
    }

    fn desktop_environment(&self) -> Option<String> {
        None
    }

    fn is_wayland(&self) -> bool {
        false
    }

    fn is_x11(&self) -> bool {
        false
    }

    fn default_browser_cmd(&self) -> Vec<String> {
        vec!["cmd".into(), "/c".into(), "start".into()]
    }

    fn default_terminal_cmd(&self) -> Vec<String> {
        vec!["cmd".into(), "/c".into(), "start".into(), "cmd".into()]
    }

    fn wry_backend(&self) -> &'static str {
        "webview2"
    }

    fn config_overrides(&self) -> Vec<(&'static str, String)> {
        vec![("render_mode", "native".into())]
    }

    fn file_open_dialog(&self, _title: &str, _filters: &[(&str, &str)]) -> Option<PathBuf> {
        None
    }

    fn show_notification(&self, _title: &str, _body: &str) {}

    fn super_key_name(&self) -> &'static str {
        "Win"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_downloads_dir() {
        let dir = WindowsPlatform.downloads_dir();
        assert!(dir.to_string_lossy().contains("Downloads"));
    }

    #[test]
    fn test_windows_os_name() {
        assert_eq!(WindowsPlatform.os_name(), "Windows");
    }

    #[test]
    fn test_windows_desktop_environment_none() {
        assert!(WindowsPlatform.desktop_environment().is_none());
    }

    #[test]
    fn test_windows_is_wayland_false() {
        assert!(!WindowsPlatform.is_wayland());
    }

    #[test]
    fn test_windows_is_x11_false() {
        assert!(!WindowsPlatform.is_x11());
    }

    #[test]
    fn test_windows_default_browser_cmd() {
        let cmd = WindowsPlatform.default_browser_cmd();
        assert_eq!(cmd, vec!["cmd", "/c", "start"]);
    }

    #[test]
    fn test_windows_default_terminal_cmd() {
        let cmd = WindowsPlatform.default_terminal_cmd();
        assert_eq!(cmd, vec!["cmd", "/c", "start", "cmd"]);
    }

    #[test]
    fn test_windows_wry_backend() {
        assert_eq!(WindowsPlatform.wry_backend(), "webview2");
    }

    #[test]
    fn test_windows_config_overrides_render_mode() {
        let overrides = WindowsPlatform.config_overrides();
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].0, "render_mode");
        assert_eq!(overrides[0].1, "native");
    }

    #[test]
    fn test_windows_super_key_name() {
        assert_eq!(WindowsPlatform.super_key_name(), "Win");
    }

    #[test]
    fn test_windows_file_open_dialog_stub() {
        assert!(WindowsPlatform.file_open_dialog("Open", &[]).is_none());
    }

    #[test]
    fn test_windows_show_notification_no_panic() {
        WindowsPlatform.show_notification("test", "body");
    }
}
