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

    fn file_open_dialog(&self, title: &str, filters: &[(&str, &str)]) -> Option<PathBuf> {
        if std::env::var("AILERON_TESTING").is_ok() {
            return None;
        }

        // Build a PowerShell script that uses Windows Forms OpenFileDialog.
        // This avoids needing raw COM FFI while still providing a native dialog.
        let filter_parts: Vec<String> = filters
            .iter()
            .map(|(name, exts)| format!("\"{name}|{exts}\""))
            .collect();
        let filter_str = if filter_parts.is_empty() {
            "All Files|*.*".to_string()
        } else {
            filter_parts.join(",")
        };

        let script = format!(
            r#"
            Add-Type -AssemblyName System.Windows.Forms
            $dialog = New-Object System.Windows.Forms.OpenFileDialog
            $dialog.Title = '{title}'
            $dialog.Filter = '{filter_str}'
            $dialog.Multiselect = $false
            if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{
                $dialog.FileName
            }}
            "#,
            title = title.replace('\'', "''"),
            filter_str = filter_str.replace('\'', "''"),
        );

        std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .and_then(|output| {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if path.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(path))
                }
            })
    }

    fn show_notification(&self, title: &str, body: &str) {
        // Use PowerShell BurntToast module if available, fall back to msg.exe
        let script = format!(
            r#"
            if (Get-Module -ListAvailable -Name BurntToast) {{
                Import-Module BurntToast
                New-BurntToastNotification -Text '{title}', '{body}'
            }} else {{
                msg.exe * /time:5 '{title}: {body}'
            }}
            "#,
            title = title.replace('\'', "''"),
            body = body.replace('\'', "''"),
        );
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    fn super_key_name(&self) -> &'static str {
        "Win"
    }

    fn shell_command(&self, cmd: &str) -> Vec<String> {
        vec!["cmd".into(), "/c".into(), cmd.into()]
    }

    fn clipboard_copy(&self, text: &str) -> bool {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child = match Command::new("powershell")
            .args(["-NoProfile", "-Command", "$input | Set-Clipboard"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return false,
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        child.wait().ok().map(|s| s.success()).unwrap_or(false)
    }

    fn clipboard_paste(&self) -> Option<String> {
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "Get-Clipboard"])
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

    #[test]
    fn test_windows_shell_command() {
        let cmd = WindowsPlatform.shell_command("echo hello");
        assert_eq!(cmd, vec!["cmd", "/c", "echo hello"]);
    }

    #[test]
    fn test_windows_clipboard_copy_no_panic() {
        let _ = WindowsPlatform.clipboard_copy("test");
    }
}
