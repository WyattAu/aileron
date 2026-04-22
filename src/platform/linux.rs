use std::path::PathBuf;
use std::process::Command;

use super::traits::PlatformOps;

pub struct LinuxPlatform;

impl PlatformOps for LinuxPlatform {
    fn downloads_dir(&self) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join("Downloads")
    }

    fn os_name(&self) -> &'static str {
        "Linux"
    }

    fn desktop_environment(&self) -> Option<String> {
        std::env::var("XDG_CURRENT_DESKTOP")
            .ok()
            .or_else(|| std::env::var("DESKTOP_SESSION").ok())
    }

    fn is_wayland(&self) -> bool {
        std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE")
                .map(|s| s == "wayland")
                .unwrap_or(false)
    }

    fn is_x11(&self) -> bool {
        std::env::var("DISPLAY").is_ok()
    }

    fn default_browser_cmd(&self) -> Vec<String> {
        vec!["xdg-open".into()]
    }

    fn default_terminal_cmd(&self) -> Vec<String> {
        vec!["sh".into(), "-c".into(), "$TERM".into()]
    }

    fn wry_backend(&self) -> &'static str {
        "webkitgtk"
    }

    fn config_overrides(&self) -> Vec<(&'static str, String)> {
        vec![]
    }

    fn file_open_dialog(&self, title: &str, filters: &[(&str, &str)]) -> Option<PathBuf> {
        // Skip GUI dialogs in headless/test environments
        if std::env::var("AILERON_TESTING").is_ok() {
            return None;
        }

        // Check for a display server before attempting GUI dialogs
        if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
            return None;
        }

        let dialog_cmd = if Command::new("zenity").arg("--version").output().is_ok() {
            "zenity"
        } else if Command::new("kdialog").arg("--version").output().is_ok() {
            "kdialog"
        } else {
            return None;
        };

        let mut cmd = Command::new(dialog_cmd);
        match dialog_cmd {
            "zenity" => {
                cmd.arg("--file-selection").arg("--title").arg(title);
                for (name, patterns) in filters {
                    cmd.arg("--file-filter")
                        .arg(format!("{}|{}", name, patterns));
                }
            }
            "kdialog" => {
                cmd.arg("--getopenfilename")
                    .arg("--title")
                    .arg(title)
                    .arg(".")
                    .arg(
                        filters
                            .iter()
                            .map(|(name, patterns)| format!("{} ({})", name, patterns))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
            }
            _ => unreachable!(),
        }

        cmd.output().ok().and_then(|output| {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() {
                None
            } else {
                Some(PathBuf::from(path))
            }
        })
    }

    fn show_notification(&self, title: &str, body: &str) {
        let _ = Command::new("notify-send").arg(title).arg(body).spawn();
    }

    fn super_key_name(&self) -> &'static str {
        "Super"
    }

    fn shell_command(&self, cmd: &str) -> Vec<String> {
        vec!["sh".into(), "-c".into(), cmd.into()]
    }

    fn clipboard_copy(&self, text: &str) -> bool {
        use std::process::Stdio;
        // Try Wayland first (wl-copy), then X11 (xclip), then xsel
        std::process::Command::new("wl-copy")
            .arg(text)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()
            .map(|s| s.success())
            .unwrap_or(false)
            || std::process::Command::new("xclip")
                .args(["-selection", "clipboard"])
                .arg(text)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .ok()
                .map(|s| s.success())
                .unwrap_or(false)
            || std::process::Command::new("xsel")
                .args(["--clipboard", "--input"])
                .arg(text)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .ok()
                .map(|s| s.success())
                .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linux_downloads_dir() {
        let platform = LinuxPlatform;
        let dir = platform.downloads_dir();
        assert!(dir.to_string_lossy().contains("Downloads"));
    }

    #[test]
    fn test_linux_os_name() {
        assert_eq!(LinuxPlatform.os_name(), "Linux");
    }

    #[test]
    fn test_linux_desktop_environment_returns_option() {
        let _ = LinuxPlatform.desktop_environment();
    }

    #[test]
    fn test_linux_is_wayland_returns_bool() {
        let _ = LinuxPlatform.is_wayland();
    }

    #[test]
    fn test_linux_is_x11_returns_bool() {
        let _ = LinuxPlatform.is_x11();
    }

    #[test]
    fn test_linux_default_browser_cmd() {
        let cmd = LinuxPlatform.default_browser_cmd();
        assert_eq!(cmd, vec!["xdg-open"]);
    }

    #[test]
    fn test_linux_default_terminal_cmd() {
        let cmd = LinuxPlatform.default_terminal_cmd();
        assert_eq!(cmd, vec!["sh", "-c", "$TERM"]);
    }

    #[test]
    fn test_linux_wry_backend() {
        assert_eq!(LinuxPlatform.wry_backend(), "webkitgtk");
    }

    #[test]
    fn test_linux_config_overrides_empty() {
        assert!(LinuxPlatform.config_overrides().is_empty());
    }

    #[test]
    fn test_linux_super_key_name() {
        assert_eq!(LinuxPlatform.super_key_name(), "Super");
    }

    #[test]
    fn test_linux_show_notification_no_panic() {
        LinuxPlatform.show_notification("test", "body");
    }

    #[test]
    fn test_linux_file_open_dialog_no_panic() {
        let _ = LinuxPlatform.file_open_dialog("Open", &[]);
    }

    #[test]
    fn test_linux_shell_command() {
        let cmd = LinuxPlatform.shell_command("echo hello");
        assert_eq!(cmd, vec!["sh", "-c", "echo hello"]);
    }

    #[test]
    fn test_linux_clipboard_copy_no_panic() {
        // May fail if no clipboard tool installed, but must not panic
        let _ = LinuxPlatform.clipboard_copy("test");
    }
}
