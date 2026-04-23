use std::path::PathBuf;

use super::traits::PlatformOps;

pub struct MacOSPlatform;

impl PlatformOps for MacOSPlatform {
    fn downloads_dir(&self) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join("Downloads")
    }

    fn os_name(&self) -> &'static str {
        "macOS"
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
        vec!["open".into()]
    }

    fn default_terminal_cmd(&self) -> Vec<String> {
        vec!["open".into(), "-a".into(), "Terminal".into()]
    }

    fn wry_backend(&self) -> &'static str {
        "wkwebview"
    }

    fn config_overrides(&self) -> Vec<(&'static str, String)> {
        vec![("tab_sidebar_right", "true".into())]
    }

    fn file_open_dialog(&self, _title: &str, _filters: &[(&str, &str)]) -> Option<PathBuf> {
        None
    }

    fn show_notification(&self, _title: &str, _body: &str) {}

    fn super_key_name(&self) -> &'static str {
        "Cmd"
    }

    fn shell_command(&self, cmd: &str) -> Vec<String> {
        vec!["sh".into(), "-c".into(), cmd.into()]
    }

    fn clipboard_copy(&self, text: &str) -> bool {
        use std::process::Stdio;
        std::process::Command::new("pbcopy")
            .arg(text)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn clipboard_paste(&self) -> Option<String> {
        std::process::Command::new("pbpaste")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success() && !o.stdout.is_empty())
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macos_downloads_dir() {
        let dir = MacOSPlatform.downloads_dir();
        assert!(dir.to_string_lossy().contains("Downloads"));
    }

    #[test]
    fn test_macos_os_name() {
        assert_eq!(MacOSPlatform.os_name(), "macOS");
    }

    #[test]
    fn test_macos_desktop_environment_none() {
        assert!(MacOSPlatform.desktop_environment().is_none());
    }

    #[test]
    fn test_macos_is_wayland_false() {
        assert!(!MacOSPlatform.is_wayland());
    }

    #[test]
    fn test_macos_is_x11_false() {
        assert!(!MacOSPlatform.is_x11());
    }

    #[test]
    fn test_macos_default_browser_cmd() {
        let cmd = MacOSPlatform.default_browser_cmd();
        assert_eq!(cmd, vec!["open"]);
    }

    #[test]
    fn test_macos_default_terminal_cmd() {
        let cmd = MacOSPlatform.default_terminal_cmd();
        assert_eq!(cmd, vec!["open", "-a", "Terminal"]);
    }

    #[test]
    fn test_macos_wry_backend() {
        assert_eq!(MacOSPlatform.wry_backend(), "wkwebview");
    }

    #[test]
    fn test_macos_config_overrides_sidebar_right() {
        let overrides = MacOSPlatform.config_overrides();
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].0, "tab_sidebar_right");
        assert_eq!(overrides[0].1, "true");
    }

    #[test]
    fn test_macos_super_key_name() {
        assert_eq!(MacOSPlatform.super_key_name(), "Cmd");
    }

    #[test]
    fn test_macos_file_open_dialog_stub() {
        assert!(MacOSPlatform.file_open_dialog("Open", &[]).is_none());
    }

    #[test]
    fn test_macos_show_notification_no_panic() {
        MacOSPlatform.show_notification("test", "body");
    }

    #[test]
    fn test_macos_shell_command() {
        let cmd = MacOSPlatform.shell_command("echo hello");
        assert_eq!(cmd, vec!["sh", "-c", "echo hello"]);
    }

    #[test]
    fn test_macos_clipboard_copy_no_panic() {
        let _ = MacOSPlatform.clipboard_copy("test");
    }
}
